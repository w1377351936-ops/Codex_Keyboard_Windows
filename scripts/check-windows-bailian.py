"""Check the current user's Bailian credential and optionally run one tiny TTS call.

The API key is read from Windows Credential Manager, never from argv or env.
No key, signed audio URL, or response body is printed.
"""

from __future__ import annotations

import argparse
import array
import base64
import ctypes
from ctypes import wintypes
import io
import json
import sys
import urllib.error
import urllib.parse
import urllib.request
import wave


TARGET = "EasyCodexInput/DASHSCOPE_API_KEY"
CRED_TYPE_GENERIC = 1
TTS_URL = "https://dashscope.aliyuncs.com/api/v1/services/audio/tts/SpeechSynthesizer"


class FILETIME(ctypes.Structure):
    _fields_ = [("low", wintypes.DWORD), ("high", wintypes.DWORD)]


class CREDENTIALW(ctypes.Structure):
    _fields_ = [
        ("flags", wintypes.DWORD),
        ("type", wintypes.DWORD),
        ("target_name", wintypes.LPWSTR),
        ("comment", wintypes.LPWSTR),
        ("last_written", FILETIME),
        ("blob_size", wintypes.DWORD),
        ("blob", ctypes.POINTER(ctypes.c_ubyte)),
        ("persist", wintypes.DWORD),
        ("attribute_count", wintypes.DWORD),
        ("attributes", ctypes.c_void_p),
        ("target_alias", wintypes.LPWSTR),
        ("user_name", wintypes.LPWSTR),
    ]


def read_key() -> str:
    if sys.platform != "win32":
        raise RuntimeError("Windows is required")
    library = ctypes.WinDLL("Advapi32.dll", use_last_error=True)
    library.CredReadW.argtypes = [
        wintypes.LPCWSTR,
        wintypes.DWORD,
        wintypes.DWORD,
        ctypes.POINTER(ctypes.POINTER(CREDENTIALW)),
    ]
    library.CredReadW.restype = wintypes.BOOL
    library.CredFree.argtypes = [ctypes.c_void_p]
    library.CredFree.restype = None
    credential = ctypes.POINTER(CREDENTIALW)()
    if not library.CredReadW(TARGET, CRED_TYPE_GENERIC, 0, ctypes.byref(credential)):
        raise OSError(ctypes.get_last_error(), "Windows credential could not be read")
    try:
        record = credential.contents
        if not record.blob or record.blob_size == 0 or record.blob_size > 4096:
            raise ValueError(f"Credential has an invalid length ({record.blob_size} bytes)")
        raw = ctypes.string_at(record.blob, record.blob_size)
        key = raw.decode("utf-16-le").rstrip("\x00")
        if not key.startswith("sk-") or len(key) < 20 or any(c.isspace() for c in key):
            raise ValueError("Credential is not a Bailian API Key")
        return key
    finally:
        library.CredFree(credential)


def tts_smoke(key: str, verify_wav: bool) -> bytes | None:
    body = json.dumps(
        {
            "model": "qwen-audio-3.0-tts-flash",
            "input": {
                "text": "测试。",
                "voice": "longanfengyue",
                "format": "wav",
                "sample_rate": 48000,
            },
        },
        ensure_ascii=False,
    ).encode("utf-8")
    request = urllib.request.Request(
        TTS_URL,
        data=body,
        headers={
            "Authorization": "Bearer " + key,
            "Content-Type": "application/json",
        },
        method="POST",
    )
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            data = json.load(response)
    except urllib.error.HTTPError as error:
        raise RuntimeError(f"TTS request returned HTTP {error.code}") from None
    output = data.get("output") if isinstance(data, dict) else None
    audio = output.get("audio") if isinstance(output, dict) else None
    if not isinstance(audio, dict) or not isinstance(audio.get("url"), str):
        code = data.get("code") if isinstance(data, dict) else None
        raise RuntimeError(f"TTS response has no audio URL (code={code})")
    url = audio["url"]
    print("TTS request succeeded; audio URL received (not displayed).")
    if not verify_wav:
        return None
    parsed = urllib.parse.urlsplit(url)
    if parsed.scheme not in ("http", "https") or parsed.hostname != "dashscope-result-bj.oss-cn-beijing.aliyuncs.com":
        raise RuntimeError(f"TTS audio URL has an unexpected destination: {parsed.scheme}://{parsed.hostname}")
    if parsed.scheme == "http":
        url = parsed._replace(scheme="https").geturl()
    try:
        with urllib.request.urlopen(url, timeout=30) as response:
            data = response.read(1_000_001)
    except urllib.error.HTTPError as error:
        raise RuntimeError(f"TTS audio download returned HTTP {error.code}") from None
    if len(data) > 1_000_000:
        raise RuntimeError("TTS smoke audio exceeds its one-megabyte limit")
    try:
        with wave.open(io.BytesIO(data), "rb") as wav:
            channels = wav.getnchannels()
            rate = wav.getframerate()
            sample_width = wav.getsampwidth()
            pcm = wav.readframes(len(data))
            frames = len(pcm) // (channels * sample_width)
    except (EOFError, wave.Error) as error:
        raise RuntimeError(f"TTS audio is not a valid WAV ({error})") from None
    if (channels, rate, sample_width) != (1, 48000, 2) or frames == 0 or len(pcm) % 2 != 0:
        raise RuntimeError(
            f"TTS WAV format mismatch: channels={channels}, rate={rate}, bytes_per_sample={sample_width}, frames={frames}"
        )
    print(f"TTS audio verified: 48 kHz mono PCM16 WAV, {frames} frames.")
    return pcm


def asr_smoke(key: str, pcm: bytes) -> None:
    samples = array.array("h")
    samples.frombytes(pcm)
    reduced = samples[::3]
    output = io.BytesIO()
    with wave.open(output, "wb") as wav:
        wav.setnchannels(1)
        wav.setsampwidth(2)
        wav.setframerate(16000)
        wav.writeframes(reduced.tobytes())
    data_uri = "data:audio/wav;base64," + base64.b64encode(output.getvalue()).decode("ascii")
    body = json.dumps(
        {
            "model": "qwen3-asr-flash",
            "messages": [
                {
                    "role": "user",
                    "content": [
                        {"type": "input_audio", "input_audio": {"data": data_uri}}
                    ],
                }
            ],
            "stream": False,
            "asr_options": {"language": "zh", "enable_itn": True},
        }
    ).encode("utf-8")
    request = urllib.request.Request(
        "https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions",
        data=body,
        headers={"Authorization": "Bearer " + key, "Content-Type": "application/json"},
        method="POST",
    )
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            result = json.load(response)
    except urllib.error.HTTPError as error:
        raise RuntimeError(f"ASR request returned HTTP {error.code}") from None
    try:
        transcript = result["choices"][0]["message"]["content"]
    except (KeyError, IndexError, TypeError) as error:
        raise RuntimeError(f"ASR response has no transcript ({type(error).__name__})") from None
    if not isinstance(transcript, str) or "测试" not in transcript:
        raise RuntimeError("ASR did not recognize the generated test phrase")
    print("ASR request succeeded; generated test phrase recognized.")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tts-smoke", action="store_true", help="synthesize one short phrase")
    parser.add_argument("--verify-wav", action="store_true", help="download and inspect the smoke-test WAV")
    parser.add_argument("--asr-smoke", action="store_true", help="recognize the generated test phrase")
    args = parser.parse_args()
    key = read_key()
    print("Windows credential is readable and has Bailian key format.")
    if args.verify_wav and not args.tts_smoke:
        parser.error("--verify-wav requires --tts-smoke")
    if args.asr_smoke and not (args.tts_smoke and args.verify_wav):
        parser.error("--asr-smoke requires --tts-smoke --verify-wav")
    if args.tts_smoke:
        pcm = tts_smoke(key, args.verify_wav)
        if args.asr_smoke:
            assert pcm is not None
            asr_smoke(key, pcm)


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, RuntimeError) as error:
        print(f"Check failed: {error}", file=sys.stderr)
        sys.exit(1)
