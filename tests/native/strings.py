"""Run string ownership fault tests against any supported Lua runtime."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]


def test(lua, cc):
    if Path(lua).is_file():
        lua = str(Path(lua).resolve())
    out = ROOT / ".build" / "native-tests"
    out.mkdir(parents=True, exist_ok=True)
    windows = os.name == "nt"
    module = out / ("strings.dll" if windows else "strings.so")
    flags = ["-std=c11", "-O2", "-Wall", "-Wextra", "-Werror", "-shared"]
    if windows:
        flags += ["-static"]
    else:
        flags += ["-fPIC", "-pthread"]
        flags += ["-undefined", "dynamic_lookup"] if sys.platform == "darwin" else ["-ldl"]
    subprocess.run([cc, *flags, "tests/native/strings.c", "native/luaapi.c", "-o", str(module)],
                   cwd=ROOT, check=True)
    path = json.dumps(module.as_posix(), ensure_ascii=False)
    code = f'assert(assert(package.loadlib({path}, "luaopen_ws_strings"))())'
    subprocess.run([lua, "-e", code], cwd=ROOT, check=True)
    print("String ownership fault tests passed", flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--lua", required=True)
    parser.add_argument("--cc", default=os.environ.get("CC", "gcc"))
    args = parser.parse_args()
    test(args.lua, args.cc)
