"""Runs the Autobahn TestSuite against the Lua server and client and fails on
any case that does not conform. Needs Docker and a built ws/core module.

  python .github/scripts/autobahn.py --lua <lua>

Compression (cases 12 and 13) is not implemented, so it is excluded.
"""
import argparse
import json
from pathlib import Path
import shutil
import socket
import subprocess
import time
import urllib.request

ROOT = Path(__file__).resolve().parents[2]
CONFIG = ROOT / ".github/autobahn"
REPORTS = ROOT / ".build/autobahn"
IMAGE = "crossbario/autobahn-testsuite:25.10.1@sha256:519915fb568b04c9383f70a1c405ae3ff44ab9e35835b085239c258b6fac3074"
PASSING = {"OK", "NON-STRICT", "INFORMATIONAL"}
# Cases in the pinned image, minus compression; a short report means cases were skipped.
CASES = 301
MOUNTS = ["-v", f"{CONFIG}:/config:ro", "-v", f"{REPORTS}:/reports"]


def wait(ready, what, timeout=60):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            ready()
            return
        except OSError:
            time.sleep(0.5)
    raise SystemExit(f"timed out waiting for {what}")


def server(lua):
    """Autobahn's fuzzing client against our echo server."""
    proc = subprocess.Popen([lua, str(CONFIG / "server.lua"), "9002"], cwd=ROOT)
    def ready():
        if proc.poll() is not None:
            raise SystemExit("the echo server exited")
        socket.create_connection(("127.0.0.1", 9002), timeout=1).close()

    try:
        wait(ready, "the echo server")
        subprocess.run(["docker", "run", "--rm", "--add-host=host.docker.internal:host-gateway", *MOUNTS, IMAGE,
                        "wstest", "-m", "fuzzingclient", "-s", "/config/fuzzingclient.json"], check=True)
    finally:
        proc.kill()


def client(lua):
    """Our echo client against Autobahn's fuzzing server."""
    name = "ws-autobahn"
    subprocess.run(["docker", "run", "-d", "--rm", "--name", name, "-p", "127.0.0.1:9001:9001", *MOUNTS, IMAGE,
                    "wstest", "-m", "fuzzingserver", "-s", "/config/fuzzingserver.json"], check=True)
    try:
        # A published port accepts TCP before wstest listens; wait for its HTTP page.
        wait(lambda: urllib.request.urlopen("http://127.0.0.1:9001/", timeout=1).close(), "the fuzzing server")
        subprocess.run([lua, str(CONFIG / "client.lua"), "ws://127.0.0.1:9001"], cwd=ROOT, check=True, timeout=3600)
    finally:
        subprocess.run(["docker", "rm", "-f", name], check=False, capture_output=True)


def check(side):
    index = json.loads((REPORTS / side / "index.json").read_text())
    results = [(case, r) for cases in index.values() for case, r in cases.items()]
    failed = [(case, r) for case, r in results if r["behavior"] not in PASSING or r["behaviorClose"] not in PASSING]
    print(f"{side}: {len(results)} of {CASES} cases ran, {len(failed)} failed")
    for case, r in sorted(failed, key=lambda f: [int(p) for p in f[0].split(".")]):
        print(f"  {case}: {r['behavior']} / close {r['behaviorClose']}")
    return len(results) == CASES and not failed


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--lua", required=True)
    parser.add_argument("--side", choices=["server", "client", "both"], default="both")
    args = parser.parse_args()
    lua = str(Path(args.lua).resolve())
    sides = ["server", "client"] if args.side == "both" else [args.side]
    shutil.rmtree(REPORTS, ignore_errors=True)
    REPORTS.mkdir(parents=True)
    for side in sides:
        {"server": server, "client": client}[side](lua)
    ok = [check(side) for side in sides]
    if not all(ok):
        raise SystemExit("Autobahn found non-conforming cases; see .build/autobahn")


if __name__ == "__main__":
    main()
