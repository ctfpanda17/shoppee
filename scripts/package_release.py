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
    for file in sorted((root / "companion-extension").rglob("*")):
        if file.is_file():
            z.write(file, file.relative_to(root).as_posix())
    command = ".\\shoppee-core.exe" if platform.startswith("windows") else "./shoppee-core"
    quickstart = f"""shoppee-core v{version} ({platform})

Double-click shoppee-core.exe (Windows) or run the command below to open the local UI.
Keep the terminal window open while the app is running.

Local graphical interface:
  {command} ui

Browser SKU selection (Chrome / Edge):
  1. Load the companion-extension folder at chrome://extensions or edge://extensions
     using Developer mode > Load unpacked.
  2. Open the local UI in that same browser. Click Connect browser / 連接瀏覽器.
  3. Paste the product link, read real options, choose SKU and quantity, then start.
  4. The task stops at checkout. Review shipping-inclusive total and place the order yourself.
  If your default browser is Safari/Firefox, copy the full local URL printed by
  the application into Chrome/Edge. Keep the local UI tab open during the task.

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

Browser mode uses rendered Shopee product controls to select options and go to
checkout; shipping errors, disabled options and an over-budget/unknown cart footer total stop the task.
No final order or payment is submitted. No passwords/cookies leave your browser.
Page-only mode and local mock mode remain available. Mock prices are not real
product prices. Existing performance reports remain the v0.1.0 baseline.
See README.md and RELEASE_NOTES.md for details and limits.
"""
    z.writestr("QUICKSTART.txt", quickstart)
    if platform.startswith("windows"):
        z.writestr("START_UI.cmd", "@echo off\r\ncd /d \"%~dp0\"\r\nshoppee-core.exe ui\r\nif errorlevel 1 pause\r\n")
        z.writestr("HELP.cmd", "@echo off\r\ncd /d \"%~dp0\"\r\nshoppee-core.exe --help\r\npause\r\n")
print(archive.name)
