"""Assembles the release: a source archive carrying every platform's
prebuilt module, packed into a LuaRocks source rock. Installing it only
copies files.

  python .github/scripts/package.py [--partial]

--partial packages whatever bundles exist, for local install tests only.
"""
import argparse
import hashlib
from pathlib import Path
import shutil
import tarfile
import tempfile
import zipfile

ROOT = Path(__file__).resolve().parents[2]
PLATFORMS = ["linux-x86_64", "linux-arm64", "macosx-x86_64", "macosx-arm64", "win32-x86_64"]
SOURCES = ["ws", "bundles"]
FILES = ["scripts/native.lua", "README.md", "LICENSE"]
# luarocks.org refuses request bodies over 15 MiB (nginx client_max_body_size
# 15m); the rest leaves room for the upload's multipart encoding.
MAX_ROCK = 15 * 1024 * 1024 - 64 * 1024


def rockspec():
    specs = list(ROOT.glob("ws-[0-9]*-1.rockspec"))
    assert len(specs) == 1, "expected exactly one release rockspec"
    return specs[0], specs[0].name[len("ws-"):-len("-1.rockspec")]


def check_bundles(partial):
    for target in PLATFORMS:
        name = "core.dll" if target.startswith("win32") else "core.so"
        if not (ROOT / "bundles" / target / name).is_file() and not partial:
            raise SystemExit(f"missing bundle for {target}; run the full build matrix")


def package(partial):
    spec, version = rockspec()
    check_bundles(partial)
    out = ROOT / "dist" / ("local" if partial else "")
    out.mkdir(parents=True, exist_ok=True)
    stage = Path(tempfile.mkdtemp(dir=out)) / f"ws-{version}"
    ignore = shutil.ignore_patterns("target", "__pycache__")
    for name in SOURCES:
        if (ROOT / name).exists():
            shutil.copytree(ROOT / name, stage / name, ignore=ignore)
    # A local development build must not shadow the bundles.
    for local in ["core.so", "core.dll"]:
        (stage / "ws" / local).unlink(missing_ok=True)
    for name in FILES + [spec.name]:
        (stage / name).parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(ROOT / name, stage / name)
    # The archive name and folder match the rockspec's source url and dir.
    archive = out / f"v{version}.tar.gz"
    with tarfile.open(archive, "w:gz") as tf:
        tf.add(stage, arcname=stage.name)
    shutil.rmtree(stage.parent)
    rock = out / f"ws-{version}-1.src.rock"
    with zipfile.ZipFile(rock, "w", zipfile.ZIP_DEFLATED) as zf:
        zf.write(spec, spec.name)
        zf.write(archive, archive.name)
    size = rock.stat().st_size
    if size > MAX_ROCK:
        raise SystemExit(f"{rock.name} is {size} bytes; luarocks.org accepts at most {MAX_ROCK}")
    sums ="".join(f"{hashlib.sha256(p.read_bytes()).hexdigest()}  {p.name}\n" for p in [archive, rock])
    (out / "SHA256SUMS").write_text(sums)
    print(rock)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--partial", action="store_true")
    package(parser.parse_args().partial)
