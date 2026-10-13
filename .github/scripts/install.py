"""Installs the packaged rock from a local registry, with no compiler or
Rust on PATH, then runs the Lua test suite against the installed copy.

  python .github/scripts/install.py --lua <lua> --luarocks <luarocks>
"""
import argparse
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]
TOOLS = ["sh", "ls", "uname", "mkdir", "cp", "mv", "rm", "find", "chmod", "tar", "gzip", "gunzip",
         "unzip", "zip", "pwd", "sed", "test", "wget", "curl", "tr", "dirname", "basename", "cat"]
FORBIDDEN = ["gcc", "cc", "clang", "make", "cmake", "cargo", "rustc"]


def run(*cmd, env=None, cwd=ROOT):
    print("+", " ".join(map(str, cmd)), flush=True)
    subprocess.run(list(map(str, cmd)), cwd=cwd, env=env, check=True)


def isolated_path(lua, luarocks, work):
    """A PATH with the interpreter, LuaRocks and archive tools only."""
    if os.name == "nt":
        dirs = [lua.parent, luarocks.parent, Path(os.environ["SystemRoot"]) / "System32"]
        # Some LuaRocks builds unpack with 7-Zip; allow its own folder only.
        dirs += [Path(p) for p in os.environ.get("WS_ARCHIVER_DIR", "").split(os.pathsep) if p]
        return os.pathsep.join(map(str, dirs))
    bin_dir = work / "tools"
    bin_dir.mkdir()
    for name in TOOLS:
        exe = shutil.which(name)
        if exe:
            (bin_dir / name).symlink_to(exe)
    (bin_dir / "lua").symlink_to(lua)
    return str(bin_dir)


def main(lua, luarocks):
    lua, luarocks = Path(lua).resolve(), Path(luarocks).resolve()
    rock = next((ROOT / "dist/local").glob("ws-*-1.src.rock"))
    version = rock.name[len("ws-"):-len(".src.rock")]
    abi = subprocess.check_output([str(lua), "-e", 'io.write(_VERSION:match("%d+%.%d+"))'], text=True)
    work = Path(tempfile.mkdtemp(prefix="install-", dir=ROOT / ".build"))
    server = work / "server"
    server.mkdir()
    shutil.copyfile(rock, server / rock.name)
    manifest = f'repository = {{ ws = {{ ["{version}"] = {{ {{ arch = "src" }} }} }} }}\nmodules = {{}}\ncommands = {{}}\n'
    for name in ["manifest"] + [f"manifest-5.{v}" for v in range(1, 6)]:
        (server / name).write_text(manifest)
    config = work / "config.lua"
    config.write_text(f'rocks_servers = {{ "{server.as_posix()}" }}\n')
    env = dict(os.environ, LUAROCKS_CONFIG=str(config), PATH=isolated_path(lua, luarocks, work))
    for name in FORBIDDEN:
        assert shutil.which(name, path=env["PATH"]) is None, f"{name} leaked into the install test"
    tree = work / "tree"
    flags = [f"--tree={tree}", f"--lua-dir={lua.parent.parent}", f"--lua-version={abi}"]
    run(luarocks, *flags, "install", "ws", env=env)
    # The tests run from a copy, so only the installed modules can load.
    shutil.copytree(ROOT / "tests", work / "tests")
    env["LUA_PATH"] = f"{tree}/share/lua/{abi}/?.lua;{tree}/share/lua/{abi}/?/init.lua"
    env["LUA_CPATH"] = f"{tree}/lib/lua/{abi}/?." + ("dll" if os.name == "nt" else "so")
    run(lua, "tests/run.lua", env=env, cwd=work)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--lua", required=True)
    parser.add_argument("--luarocks", required=True)
    args = parser.parse_args()
    main(args.lua, args.luarocks)
