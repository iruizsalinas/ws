"""Builds a reference Lua for CI: release bundles link against it and tests run on it."""
import argparse
import concurrent.futures
import hashlib
import os
from pathlib import Path
import platform
import shutil
import subprocess
import tarfile
import urllib.request

ROOT = Path(__file__).resolve().parents[2]
# Official lua.org releases and their SHA-256 digests.
RELEASES = {
    "5.1": ("5.1.5", "2640fc56a795f29d28ef15e13c34a47e223960b0240e8cb0a82d9b0738695333"),
    "5.2": ("5.2.4", "b9e2e4aad6789b3b63a056d442f7b39f0ecfca3ae0f1fc0ae4e9614401b69f4b"),
    "5.3": ("5.3.6", "fc5fd69bb8736323f026672b1b7235da613d7177e72558893a0bdcd320466d60"),
    "5.4": ("5.4.8", "4f18ddae154e793e46eeab727c59ef1c0c0c2b744e7b94219710d76f530629ae"),
    "5.5": ("5.5.0", "57ccc32bbbd005cab75bcc52444052535af691789dba2b9016d5c50640d68b3d"),
}


def build(version, cc="gcc", jobs=4):
    release, digest = RELEASES[version]
    work = ROOT / ".build"
    archive = work / "downloads" / ("lua-" + release + ".tar.gz")
    archive.parent.mkdir(parents=True, exist_ok=True)
    if not archive.exists():
        with urllib.request.urlopen("https://www.lua.org/ftp/" + archive.name, timeout=120) as response:
            archive.write_bytes(response.read())
    assert hashlib.sha256(archive.read_bytes()).hexdigest() == digest, "Lua checksum mismatch"
    with tarfile.open(archive) as tf:
        tf.extractall(work / "sources", filter="data")
    source = work / "sources" / ("lua-" + release) / "src"
    out = work / "lua" / version
    for folder in ["bin", "include", "lib", "objects"]:
        (out / folder).mkdir(parents=True, exist_ok=True)
    windows = os.name == "nt"
    system = platform.system()
    flags = ["-O2", "-fPIC"]
    if windows:
        flags += ["-DLUA_BUILD_AS_DLL"]
    else:
        flags += ["-DLUA_USE_POSIX", "-DLUA_USE_DLOPEN"]
    files = [p for p in source.glob("*.c") if p.name not in ["lua.c", "luac.c", "print.c", "onelua.c"]]

    def compile_one(file):
        obj = out / "objects" / (file.stem + ".o")
        subprocess.run([cc, *flags, "-c", str(file), "-o", str(obj)], check=True)
        return str(obj)

    with concurrent.futures.ThreadPoolExecutor(max_workers=jobs) as pool:
        objects = list(pool.map(compile_one, files))
    if windows:
        library = out / "lib/liblua.dll.a"
        dll = out / "bin" / ("lua" + version.replace(".", "") + ".dll")
        subprocess.run([cc, "-shared", "-static-libgcc", *objects, "-Wl,--out-implib," + str(library), "-o", str(dll)], check=True)
        exe = out / "bin/lua.exe"
        subprocess.run([cc, "-O2", "-DLUA_BUILD_AS_DLL", str(source / "lua.c"), str(library), "-static-libgcc", "-o", str(exe)], check=True)
    else:
        library = out / "lib/liblua.a"
        subprocess.run(["ar", "rcs", str(library), *objects], check=True)
        exe = out / "bin/lua"
        link = ["-Wl,-export_dynamic"] if system == "Darwin" else ["-Wl,-E", "-ldl"]
        subprocess.run([cc, "-O2", *flags, str(source / "lua.c"), str(library), "-lm", *link, "-o", str(exe)], check=True)
    for header in source.glob("*.h"):
        shutil.copyfile(header, out / "include" / header.name)
    subprocess.run([str(exe), "-v"], check=True)
    print(out)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("version", choices=sorted(RELEASES))
    parser.add_argument("--cc", default=os.environ.get("CC", "gcc"))
    args = parser.parse_args()
    build(args.version, args.cc)
