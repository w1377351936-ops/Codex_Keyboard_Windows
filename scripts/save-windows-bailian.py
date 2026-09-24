"""Save a Bailian key from stdin as a current-user Windows generic credential.

Example: Get-Clipboard -Raw | python scripts/save-windows-bailian.py
The key must never be passed as a command-line argument or printed.
"""

from __future__ import annotations

import argparse
import ctypes
from ctypes import wintypes
import re
import sys


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


def save(target: str, key: str) -> None:
    if sys.platform != "win32":
        raise RuntimeError("Windows is required")
    if not re.fullmatch(r"sk-[A-Za-z0-9._-]{20,}", key):
        raise ValueError("Input is not a Bailian API Key")
    encoded = bytearray(key.encode("utf-16-le"))
    blob = (ctypes.c_ubyte * len(encoded)).from_buffer(encoded)
    credential = CREDENTIALW()
    credential.type = 1  # CRED_TYPE_GENERIC
    credential.target_name = target
    credential.user_name = "DashScope-Beijing"
    credential.blob_size = len(encoded)
    credential.blob = ctypes.cast(blob, ctypes.POINTER(ctypes.c_ubyte))
    credential.persist = 2  # CRED_PERSIST_LOCAL_MACHINE
    library = ctypes.WinDLL("Advapi32.dll", use_last_error=True)
    library.CredWriteW.argtypes = [ctypes.POINTER(CREDENTIALW), wintypes.DWORD]
    library.CredWriteW.restype = wintypes.BOOL
    try:
        if not library.CredWriteW(ctypes.byref(credential), 0):
            raise OSError(ctypes.get_last_error(), "Windows generic credential write failed")
    finally:
        ctypes.memset(ctypes.addressof(blob), 0, len(encoded))


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", default="EasyCodexInput/DASHSCOPE_API_KEY")
    args = parser.parse_args()
    key = sys.stdin.read().strip()
    save(args.target, key)
    print("Bailian key saved as a current-user generic credential; value not displayed.")


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, RuntimeError) as error:
        print(f"Save failed: {error}", file=sys.stderr)
        sys.exit(1)
