"""Wait for this EasyInput V2's transient ESP32-S3 USB port, then flash three locked images.

Run only after the user explicitly authorizes the exact hashes below. The script
never erases NVS and makes at most one esptool attempt per invocation.
"""

from __future__ import annotations

import argparse
import hashlib
from pathlib import Path
import subprocess
import sys
import time

from serial.tools import list_ports


ROOT = Path(__file__).resolve().parents[1]
FIRMWARE = ROOT / "firmware"
IMAGES = (
    ("0x0", FIRMWARE / "build/bootloader/bootloader.bin", "001ebd5cad3e5ccc7e66b13ff9076af29b510255f6580fe81379f5444bad4991"),
    ("0x8000", FIRMWARE / "build/partition_table/partition-table.bin", "7c541b70dcac8f920c2d11589f06745e1b033fa9b95b8343de2748bb8312a278"),
    ("0x10000", FIRMWARE / "build/easy_codex_input.bin", "aa1ffe4e2d5f8a10576e8273fe45b1e89f75eb91f18670e4528042115e44ce82"),
)
def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--identify", action="store_true", help="Only report the transient ESP32-S3 USB serial; never flash")
    mode.add_argument("--serial", help="ESP32-S3 USB serial confirmed for the board being flashed")
    args = parser.parse_args()
    expected_serial = args.serial.upper() if args.serial else None
    if not args.identify:
        for _, path, expected in IMAGES:
            digest = hashlib.sha256(path.read_bytes()).hexdigest()
            if digest != expected:
                raise RuntimeError(f"Image SHA-256 changed: {path.name}")
    print("status=waiting_for_esp32s3_usb_port", flush=True)
    deadline = time.monotonic() + 180
    while time.monotonic() < deadline:
        candidates = [
            port for port in list_ports.comports()
            if port.vid == 0x303A and port.pid == 0x1001
        ]
        if len(candidates) > 1:
            raise RuntimeError("Multiple ESP32-S3 USB ports found; refusing to choose")
        if candidates:
            port = candidates[0]
            if not port.serial_number:
                raise RuntimeError("ESP32-S3 USB serial is missing")
            if args.identify:
                print(f"status=identified serial={port.serial_number}", flush=True)
                return 0
            if port.serial_number.upper() != expected_serial:
                raise RuntimeError("ESP32-S3 USB serial does not match the confirmed board identity")
            print(f"status=port_found port={port.device}", flush=True)
            command = [
                sys.executable, "-m", "esptool", "--chip", "esp32s3", "-p", port.device,
                "-b", "460800", "--before", "usb_reset", "--after", "hard_reset",
                "--connect-attempts", "0", "write_flash", "--flash_mode", "dio",
                "--flash_freq", "80m", "--flash_size", "16MB",
            ]
            for offset, path, _ in IMAGES:
                command.extend((offset, str(path)))
            return subprocess.run(command, cwd=FIRMWARE, check=False).returncode
        time.sleep(0.05)
    print("status=timed_out_no_port", flush=True)
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
