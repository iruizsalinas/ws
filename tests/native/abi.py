"""Check that direct core loading refuses incompatible Lua number ABIs.

Uses the official Lua 5.4 sources already built by the bundle jobs.
"""
import concurrent.futures
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]


def test(kind, cc):
    work = ROOT / ".build" / "abi-tests" / kind.lower()
    source = work / "src"
    shutil.copytree(ROOT / ".build/sources/lua-5.4.8/src", source, dirs_exist_ok=True)
    config = source / "luaconf.h"
    text = config.read_text()
    old = "LUA_FLOAT_DOUBLE" if kind != "INT" else "LUA_INT_LONGLONG"
    macro = "LUA_FLOAT_DEFAULT" if kind != "INT" else "LUA_INT_DEFAULT"
    line = next(line for line in text.splitlines() if line.startswith("#define " + macro))
    assert old in line
    new = "LUA_FLOAT_" + kind if kind != "INT" else "LUA_INT_INT"
    config.write_text(text.replace(line, line.replace(old, new)))
    windows = os.name == "nt"
    flags = ["-O2", "-fPIC", "-DLUA_USE_APICHECK"]
    flags += ["-DLUA_BUILD_AS_DLL"] if windows else ["-DLUA_USE_POSIX", "-DLUA_USE_DLOPEN"]
    files = [p for p in source.glob("*.c") if p.name not in ["lua.c", "luac.c", "onelua.c", "print.c"]]

    def compile_one(file):
        obj = work / (file.stem + ".o")
        subprocess.run([cc, *flags, "-c", str(file), "-o", str(obj)], check=True)
        return str(obj)

    with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
        objects = list(pool.map(compile_one, files))
    exe = work / ("lua.exe" if windows else "lua")
    if windows:
        lib = work / "liblua.dll.a"
        subprocess.run([cc, "-shared", "-static-libgcc", *objects,
                        "-Wl,--out-implib," + str(lib), "-o", str(work / "lua54.dll")], check=True)
        link = [str(lib), "-static-libgcc"]
    else:
        link = objects + ["-lm"]
        link += ["-Wl,-export_dynamic"] if sys.platform == "darwin" else ["-Wl,-E", "-ldl"]
    subprocess.run([cc, *flags, str(source / "lua.c"), *link, "-o", str(exe)], check=True)
    core = json.dumps(str(ROOT / "ws" / ("core.dll" if windows else "core.so")).replace("\\", "/"))
    expected = "64-bit integers" if kind == "INT" else "double numbers"
    code = (f'local loader = assert(package.loadlib({core}, "luaopen_ws_core")); '
            f'local ok, err = pcall(loader); assert(not ok and tostring(err):find("{expected}", 1, true), tostring(err))')
    subprocess.run([str(exe), "-e", code], cwd=ROOT, check=True)
    print("Rejected incompatible Lua ABI: " + kind, flush=True)


if __name__ == "__main__":
    for kind in ["FLOAT", "LONGDOUBLE", "INT"]:
        test(kind, os.environ.get("CC", "gcc"))
