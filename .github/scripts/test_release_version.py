from pathlib import Path
import tempfile
import tomllib
import unittest

from release_version import bump_package, next_version


class ReleaseVersionTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="ws-release-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        files = {
            "Cargo.toml": '[workspace]\nmembers = ["core", "native"]\n\n[workspace.package]\nversion = "0.2.2"\nedition = "2024"\n',
            "core/Cargo.toml": '[package]\nname = "ws-core"\nversion.workspace = true\n',
            "native/Cargo.toml": '[package]\nname = "ws-native"\nversion.workspace = true\n',
            "Cargo.lock": 'version = 4\n\n[[package]]\nname = "dependency"\nversion = "0.2.2"\nsource = "registry+example"\n\n[[package]]\nname = "ws-core"\nversion = "0.2.2"\n\n[[package]]\nname = "ws-native"\nversion = "0.2.2"\n',
            "fuzz/Cargo.lock": 'version = 4\n\n[[package]]\nname = "ws-core"\nversion = "0.2.2"\n\n[[package]]\nname = "ws-fuzz"\nversion = "0.0.0"\n',
            "ws/init.lua": 'return { _VERSION = "0.2.2" }\n',
            "ws-0.2.2-1.rockspec": 'version = "0.2.2-1"\nsource = {\n url = "https://github.com/iruizsalinas/ws/archive/refs/tags/v0.2.2.tar.gz",\n dir = "ws-0.2.2",\n}\n',
        }
        for name, text in files.items():
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text, encoding="utf-8")

    def snapshot(self):
        return {p.relative_to(self.root): p.read_bytes() for p in self.root.rglob("*") if p.is_file()}

    def test_bump_choices(self):
        for bump, expected in (("patch", "0.2.3"), ("minor", "0.3.0"), ("major", "1.0.0"), ("current", "0.2.2")):
            with self.subTest(bump=bump):
                self.assertEqual(next_version("0.2.2", bump), expected)

    def test_invalid_versions_and_bumps(self):
        for version, bump in (("0.2.2-beta", "patch"), ("01.2.2", "patch"), ("0.2", "patch"), ("0.2.2", "bad")):
            with self.subTest(version=version, bump=bump), self.assertRaises(ValueError):
                next_version(version, bump)

    def test_synchronizes_all_versions_and_preserves_dependencies(self):
        self.assertEqual(bump_package(self.root, "patch"), "0.2.3")
        self.assertFalse((self.root / "ws-0.2.2-1.rockspec").exists())
        spec = (self.root / "ws-0.2.3-1.rockspec").read_text()
        self.assertIn('version = "0.2.3-1"', spec)
        self.assertIn('/v0.2.3.tar.gz', spec)
        self.assertIn('dir = "ws-0.2.3"', spec)
        self.assertIn('_VERSION = "0.2.3"', (self.root / "ws/init.lua").read_text())
        for name in ("Cargo.lock", "fuzz/Cargo.lock"):
            for package in tomllib.loads((self.root / name).read_text())["package"]:
                expected = "0.2.3" if package["name"] in ("ws-core", "ws-native") else {"dependency": "0.2.2", "ws-fuzz": "0.0.0"}[package["name"]]
                self.assertEqual(package["version"], expected)

    def test_current_is_byte_preserving(self):
        before = self.snapshot()
        self.assertEqual(bump_package(self.root, "current"), "0.2.2")
        self.assertEqual(self.snapshot(), before)

    def test_validation_fails_before_writing(self):
        for file, old, new in (("Cargo.lock", 'name = "ws-core"\nversion = "0.2.2"', 'name = "ws-core"\nversion = "0.1.0"'), ("ws/init.lua", "0.2.2", "0.1.0"), ("ws-0.2.2-1.rockspec", "v0.2.2", "v0.1.0")):
            path = self.root / file
            original = path.read_text()
            path.write_text(original.replace(old, new))
            before = self.snapshot()
            with self.subTest(file=file), self.assertRaises(ValueError):
                bump_package(self.root, "patch")
            self.assertEqual(self.snapshot(), before)
            path.write_text(original)

    def test_multiple_rockspecs_are_rejected(self):
        (self.root / "ws-0.2.3-1.rockspec").write_text('version = "0.2.3-1"')
        before = self.snapshot()
        with self.assertRaises(ValueError):
            bump_package(self.root, "patch")
        self.assertEqual(self.snapshot(), before)


if __name__ == "__main__":
    unittest.main()
