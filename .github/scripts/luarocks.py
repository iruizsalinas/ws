"""Installs LuaRocks into .build for CI install tests; never used by end users."""
import argparse
import hashlib
import os
from pathlib import Path
import subprocess
import tarfile
import zipfile

from fetch import download

ROOT = Path(__file__).resolve().parents[2]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("version")
args = parser.parse_args()
dest = ROOT / ".build/luarocks"
dest.mkdir(exist_ok=True)
if os.name == "nt":
    archive = ROOT / ".build/downloads/luarocks.zip"
    url = "https://luarocks.github.io/luarocks/releases/luarocks-3.13.0-windows-64.zip"
    digest = "0897ade5d459d55cd1962a948153745a6749feb345403c68aaa9207388557ab9"
else:
    archive = ROOT / ".build/downloads/luarocks.tar.gz"
    url = "https://luarocks.org/releases/luarocks-3.13.0.tar.gz"
    digest = "245bf6ec560c042cb8948e3d661189292587c5949104677f1eecddc54dbe7e37"
if not archive.exists():
    download(url, archive)
assert hashlib.sha256(archive.read_bytes()).hexdigest() == digest
if os.name == "nt":
    with zipfile.ZipFile(archive) as zf:
        zf.extractall(dest)
else:
    with tarfile.open(archive) as tf:
        tf.extractall(ROOT / ".build/sources", filter="data")
    source = ROOT / ".build/sources/luarocks-3.13.0"
    lua = ROOT / ".build/lua" / args.version
    subprocess.run(["sh", "configure", "--prefix=" + str(dest), "--with-lua=" + str(lua), "--with-lua-version=" + args.version], cwd=source, check=True)
    subprocess.run(["make", "install"], cwd=source, check=True)
