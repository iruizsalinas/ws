"""Assembles the release: a source archive carrying every prebuilt module,
packed into a LuaRocks source rock. Installing it only copies files.

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
VERSIONS = ["5.1", "5.2", "5.3", "5.4", "5.5"]
SOURCES = ["ws", "scripts", "bundles", "core", "native", "tests"]
FILES = ["Cargo.toml", "Cargo.lock", "README.md", "LICENSE"]


def rockspec():
    specs = [p for p in ROOT.glob("ws-*-1.rockspec") if "scm" not in p.name]
    assert len(specs) == 1, "expected exactly one release rockspec"
    return specs[0], specs[0].name[len("ws-"):-len("-1.rockspec")]


def check_bundles(partial):
    for target in PLATFORMS:
        for version in VERSIONS:
            name = "core.dll" if target.startswith("win32") else "core.so"
            path = ROOT / "bundles" / target / version / name
            if not path.is_file() and not partial:
                raise SystemExit(f"missing bundle {target}/Lua {version}; run the full build matrix")


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
    sums = "".join(f"{hashlib.sha256(p.read_bytes()).hexdigest()}  {p.name}\n" for p in [archive, rock])
    (out / "SHA256SUMS").write_text(sums)
    print(rock)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--partial", action="store_true")
    package(parser.parse_args().partial)
