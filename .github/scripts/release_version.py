import argparse
import os
from pathlib import Path
import re
import tomllib


def next_version(version, bump):
    if bump not in ("patch", "minor", "major", "current"):
        raise ValueError("choose patch, minor, major, or current")
    if not re.fullmatch(r"(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)", version):
        raise ValueError("expected a stable version")
    parts = list(map(int, version.split(".")))
    if bump != "current":
        index = {"major": 0, "minor": 1, "patch": 2}[bump]
        parts[index] += 1
        parts[index + 1:] = [0] * (2 - index)
    return ".".join(map(str, parts))


def replace_once(text, pattern, replacement):
    updated, count = re.subn(pattern, lambda match: replacement(match), text, flags=re.MULTILINE)
    if count != 1:
        raise ValueError(f"expected exactly one version reference: {pattern}")
    return updated


def update_lock(text, names, before, after):
    packages = tomllib.loads(text)["package"]
    for name in names:
        entries = [p for p in packages if p["name"] == name and "source" not in p]
        if len(entries) != 1 or entries[0]["version"] != before:
            raise ValueError(f"lockfile version mismatch for {name}")
        pattern = rf'(\[\[package\]\]\s*\nname = "{re.escape(name)}"\s*\nversion = "){re.escape(before)}(")'
        text = replace_once(text, pattern, lambda m: m[1] + after + m[2])
    return text


def bump_package(root, bump):
    manifest_path = root / "Cargo.toml"
    manifest_text = manifest_path.read_text(encoding="utf-8")
    manifest = tomllib.loads(manifest_text)
    before = manifest["workspace"]["package"]["version"]
    after = next_version(before, bump)
    specs = list(root.glob("ws-[0-9]*-1.rockspec"))
    if len(specs) != 1 or specs[0].name != f"ws-{before}-1.rockspec":
        raise ValueError("expected exactly one rockspec matching the workspace version")
    spec_path = specs[0]
    new_spec = root / f"ws-{after}-1.rockspec"
    if new_spec != spec_path and new_spec.exists():
        raise ValueError("target rockspec already exists")

    names = []
    for member in manifest["workspace"]["members"]:
        package = tomllib.loads((root / member / "Cargo.toml").read_text(encoding="utf-8"))["package"]
        if package["version"] != {"workspace": True}:
            raise ValueError("workspace members must inherit the release version")
        names.append(package["name"])
    changes = {}
    changes[manifest_path] = replace_once(
        manifest_text,
        rf'(\[workspace\.package\]\s*\nversion = "){re.escape(before)}(")',
        lambda m: m[1] + after + m[2],
    )
    lock_path = root / "Cargo.lock"
    changes[lock_path] = update_lock(lock_path.read_text(encoding="utf-8"), names, before, after)
    fuzz_lock = root / "fuzz" / "Cargo.lock"
    fuzz_text = fuzz_lock.read_text(encoding="utf-8")
    fuzz_names = [p["name"] for p in tomllib.loads(fuzz_text)["package"] if p["name"] in names and "source" not in p]
    if not fuzz_names:
        raise ValueError("expected workspace packages in fuzz lockfile")
    changes[fuzz_lock] = update_lock(fuzz_text, fuzz_names, before, after)

    spec = spec_path.read_text(encoding="utf-8")
    for pattern, replacement in (
        (rf'(^version\s*=\s*"){re.escape(before)}-1(")', lambda m: m[1] + after + "-1" + m[2]),
        (rf'(https://github\.com/iruizsalinas/ws/archive/refs/tags/v){re.escape(before)}(\.tar\.gz)', lambda m: m[1] + after + m[2]),
        (rf'(\bdir\s*=\s*"ws-){re.escape(before)}(")', lambda m: m[1] + after + m[2]),
    ):
        spec = replace_once(spec, pattern, replacement)
    changes[new_spec] = spec
    lua_path = root / "ws" / "init.lua"
    changes[lua_path] = replace_once(
        lua_path.read_text(encoding="utf-8"),
        rf'(_VERSION\s*=\s*"){re.escape(before)}(")',
        lambda m: m[1] + after + m[2],
    )
    if bump != "current":
        for path, text in changes.items():
            path.write_text(text, encoding="utf-8", newline="\n")
        if new_spec != spec_path:
            spec_path.unlink()
    return after


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("bump", choices=("patch", "minor", "major", "current"))
    args = parser.parse_args()
    version = bump_package(Path(__file__).resolve().parents[2], args.bump)
    if output := os.environ.get("GITHUB_OUTPUT"):
        with open(output, "a", encoding="utf-8") as stream:
            stream.write(f"version={version}\ntag=v{version}\nspec=ws-{version}-1.rockspec\n")
    print(f"Release version: {version}")
