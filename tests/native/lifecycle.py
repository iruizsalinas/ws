"""Build an embedding host and check module lifetime across repeated lua_close."""
import argparse
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]


def test(lua, cc):
    lua = Path(lua).resolve()
    prefix = lua.parent.parent
    out = ROOT / '.build/native-tests'
    out.mkdir(parents=True, exist_ok=True)
    windows = os.name == 'nt'
    exe = out / ('lifecycle.exe' if windows else 'lifecycle')
    library = prefix / 'lib' / ('liblua.dll.a' if windows else 'liblua.a')
    assert library.is_file(), f'reference Lua library not found: {library}'
    flags = ['-std=c99', '-O2', '-Wall', '-Wextra', '-Werror', '-I', str(prefix / 'include')]
    if windows:
        flags += ['-static-libgcc']
    else:
        flags += ['-Wl,-export_dynamic'] if sys.platform == 'darwin' else ['-Wl,-E', '-ldl']
    cmd = [cc, *flags, 'tests/native/lifecycle.c', str(library), '-o', str(exe)]
    if not windows:
        cmd += ['-lm']
    subprocess.run(cmd, cwd=ROOT, check=True)
    env = dict(os.environ, PATH=str(lua.parent) + os.pathsep + os.environ.get('PATH', ''))
    subprocess.run([str(exe)], cwd=ROOT, env=env, timeout=180, check=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--lua', required=True)
    parser.add_argument('--cc', default=os.environ.get('CC', 'gcc'))
    args = parser.parse_args()
    test(args.lua, args.cc)
