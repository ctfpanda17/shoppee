#!/usr/bin/env python3
"""Reproduce loopback-only evidence. Requires release binary, curl and OpenSSL."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import socket
import shutil
import subprocess
import tempfile
import time
import urllib.request

ROOT = Path(__file__).resolve().parent.parent
BINARY = ROOT / "target" / "release" / ("shoppee-core.exe" if os.name == "nt" else "shoppee-core")

def port():
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]

def ready(url):
    deadline = time.monotonic() + 5
    while time.monotonic() < deadline:
        try:
            with urllib.request.urlopen(url, timeout=0.2):
                return
        except (OSError, ValueError):
            time.sleep(0.02)
    raise RuntimeError("local mock did not start")

def run(*args):
    return json.loads(subprocess.check_output([str(BINARY), *map(str,args)], text=True))

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--samples", type=int, default=100)
    args = parser.parse_args()
    output = ROOT / "reports"
    output.mkdir(exist_ok=True)
    for delay in (0, 10):
        bind = f"127.0.0.1:{port()}"
        proc = subprocess.Popen([str(BINARY), "mock", "--bind", bind, "--release-after-ms", "0", "--delay-ms", str(delay)], stderr=subprocess.DEVNULL)
        try:
            ready(f"http://{bind}/stats")
            result = run("bench", "--base", f"http://{bind}/", "--samples", args.samples, "--interval-ms", 25)
            result["injected_delay_ms"] = delay
            (output / f"loopback-{delay}ms.json").write_text(json.dumps(result, indent=2)+"\n")
            if delay == 10:
                workflow = run("run", "--base", f"http://{bind}/", "--at-unix-ms", int(time.time()*1000)+500)
                (output / "workflow.json").write_text(json.dumps(workflow,indent=2)+"\n")
        finally:
            proc.terminate()
            proc.wait(timeout=5)
    with tempfile.TemporaryDirectory(prefix="shoppee-tls-") as temp:
        cert, key = Path(temp)/"cert.pem", Path(temp)/"key.pem"
        subprocess.run(["openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "1", "-subj", "/CN=127.0.0.1", "-addext", "subjectAltName=IP:127.0.0.1", "-keyout", str(key), "-out", str(cert)], check=True, capture_output=True)
        bind = f"127.0.0.1:{port()}"
        proc = subprocess.Popen(["openssl", "s_server", "-accept", bind, "-cert", str(cert), "-key", str(key), "-www"],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
        try:
            deadline = time.monotonic()+5
            while True:
                try:
                    with socket.create_connection(("127.0.0.1",int(bind.split(":")[1])),timeout=.1):
                        break
                except OSError:
                    if time.monotonic()>deadline:
                        raise
                    time.sleep(.02)
            result = run("probe", "--url", f"https://{bind}/", "--ca-cert", cert, "--samples", args.samples, "--interval-ms",25)
            (output/"loopback-tls.json").write_text(json.dumps(result,indent=2)+"\n")
        finally:
            proc.terminate()
            proc.wait(timeout=5)
    source_files = [ROOT/"Cargo.toml",ROOT/"Cargo.lock",*sorted((ROOT/"src").glob("*.rs"))]
    source_hash = hashlib.sha256(b"".join(p.relative_to(ROOT).as_posix().encode()+b"\0"+p.read_bytes() for p in source_files)).hexdigest()
    metadata = {"source_sha256":source_hash,"profile":"release; opt-level 3; LTO disabled; one codegen unit", "platform":platform.platform(), "architecture":platform.machine(), "samples_per_lane":args.samples,
                "rustc":subprocess.check_output([shutil.which("rustc") or str(Path.home()/".cargo/bin/rustc"),"--version"],text=True).strip(),
                "curl":subprocess.check_output(["curl","--version"],text=True).splitlines()[0],
                "scope":"loopback only; no Shopee requests; TLS probe uses OpenSSL -www, HTTP benchmark uses Axum",
                "captured_unix_ms":int(time.time()*1000)}
    (output/"environment.json").write_text(json.dumps(metadata,indent=2)+"\n")
    print("Wrote reports/loopback-{0,10}ms.json, loopback-tls.json, workflow.json, environment.json")

if __name__ == "__main__":
    main()
