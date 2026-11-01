#!/usr/bin/env python3
"""Package a CI-built binary without embedding credentials or local paths."""
import os
from pathlib import Path
import tomllib
import zipfile

root = Path(__file__).resolve().parent.parent
version = tomllib.loads((root / "Cargo.toml").read_text())["package"]["version"]
target = os.environ["BUILD_TARGET"]
platform = os.environ["BUILD_PLATFORM"]
assert platform in ("linux-x86_64", "windows-x86_64", "macos-arm64")
name = "shoppee-core.exe" if platform.startswith("windows") else "shoppee-core"
binary = root / "target" / target / "release" / name
assert binary.is_file(), f"Missing built binary for {target}"
output = root / "dist"
output.mkdir(exist_ok=True)
archive = output / f"shoppee-core-v{version}-{platform}.zip"
with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as z:
    z.write(binary, name)
    for file in ("README.md", "LICENSE", "RELEASE_NOTES.md", "reports/ANALYSIS.md"):
        z.write(root / file, file)
    command = ".\\shoppee-core.exe" if platform.startswith("windows") else "./shoppee-core"
    quickstart = f"""shoppee-core v{version} ({platform})

Double-click shoppee-core.exe (Windows) or run the command below to open the local UI.
Keep the terminal window open while the app is running.

Local graphical interface:
  {command} ui

This app also provides command-line tools. Open PowerShell/Terminal in this folder.
For macOS/Linux, run chmod +x shoppee-core if necessary.

Terminal 1 - local mock:
  {command} mock --release-after-ms 3000
Terminal 2 - monitor and prepare a quote:
  {command} run --product coffee --max-price-minor 10000 --currency TWD
Benchmark while the mock is running:
  {command} bench --samples 100
Help:
  {command} --help

curl 7.70+ is needed for bench/probe. ui/run/mock do not require curl.
Windows: Windows 10/11 x64; the Microsoft Visual C++ runtime may be required.
Linux: x86_64, glibc 2.35+ (built on Ubuntu 22.04).
macOS: Apple Silicon, macOS 11+; the binary is not notarized.

Shopee mode opens the official product page at the scheduled time for manual
purchase. No automatic Shopee order/payment API is connected. Mock mode tests
local monitoring and quote confirmation; mock prices are not real product prices.
See README.md and RELEASE_NOTES.md for details and limits.
"""
    z.writestr("QUICKSTART.txt", quickstart)
    if platform.startswith("windows"):
        z.writestr("START_UI.cmd", "@echo off\r\ncd /d \"%~dp0\"\r\nshoppee-core.exe ui\r\nif errorlevel 1 pause\r\n")
        z.writestr("HELP.cmd", "@echo off\r\ncd /d \"%~dp0\"\r\nshoppee-core.exe --help\r\npause\r\n")
print(archive.name)
