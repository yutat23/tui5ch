import hashlib
from pathlib import Path
import tarfile
import tempfile
import shutil
import unittest
import uuid
from unittest.mock import patch
import zipfile

import release


class ReleaseTests(unittest.TestCase):
    def setUp(self):
        self.temp_parent = Path(tempfile.gettempdir()).resolve()
        self.root = self.temp_parent / f"tui5ch-release-test-{uuid.uuid4().hex}"
        self.root.mkdir()
        self.root_patch = patch.object(release, "ROOT", self.root)
        self.root_patch.start()
        (self.root / "Cargo.toml").write_text('[package]\nversion = "0.1.0"\n', encoding="utf-8")
        (self.root / "CHANGELOG.md").write_text(
            "## Unreleased\n\nFuture change\n\n## 0.1.0 — 2026-10-06\n\n初回公開\n\n## 0.0.1 — 2026-10-01\n\nOlder change\n",
            encoding="utf-8",
        )

    def tearDown(self):
        self.root_patch.stop()
        assert self.root.resolve().parent == self.temp_parent
        assert self.root.name.startswith("tui5ch-release-test-")
        shutil.rmtree(self.root)

    def test_release_notes_only_include_matching_version(self):
        self.assertEqual(release.release_notes("v0.1.0"), "初回公開\n")
        for tag in ["v0.2.0", "0.1.0", "v../0.1.0"]:
            with self.assertRaises(ValueError):
                release.release_notes(tag)
        (self.root / "CHANGELOG.md").write_text("## Unreleased\n\nNot released\n", encoding="utf-8")
        with self.assertRaises(ValueError):
            release.release_notes("v0.1.0")

    def test_archives_exclude_private_data_and_preserve_unix_execution(self):
        for doc in release.DOCUMENTS:
            file = self.root / doc
            file.parent.mkdir(parents=True, exist_ok=True)
            file.write_text("public document", encoding="utf-8")
        (self.root / "data").mkdir()
        (self.root / "data/cookies.sqlite3").write_text("private")
        for target, binary in [
            ("x86_64-pc-windows-msvc", "tui5ch.exe"),
            ("x86_64-unknown-linux-gnu", "tui5ch"),
            ("aarch64-apple-darwin", "tui5ch"),
        ]:
            file = self.root / "target/release" / binary
            file.parent.mkdir(parents=True, exist_ok=True)
            file.write_bytes(b"example binary")
            file.chmod(0o755)
            with patch.object(release.subprocess, "check_output", return_value=f"host: {target}\n"):
                release.package(self.root / "dist")
            ext = ".zip" if binary.endswith(".exe") else ".tar.gz"
            archive = self.root / "dist" / f"tui5ch-0.1.0-{target}{ext}"
            if ext == ".zip":
                with zipfile.ZipFile(archive) as bundle:
                    names = bundle.namelist()
            else:
                with tarfile.open(archive) as bundle:
                    names = bundle.getnames()
                    self.assertTrue(bundle.getmember(binary).mode & 0o111)
            self.assertEqual(set(names), {binary, *release.DOCUMENTS})
        release.checksums(self.root / "dist")
        rows = (self.root / "dist/SHA256SUMS").read_text().splitlines()
        self.assertEqual(len(rows), 3)
        for row in rows:
            digest, name = row.split("  ")
            self.assertEqual(digest, hashlib.sha256((self.root / "dist" / name).read_bytes()).hexdigest())


if __name__ == "__main__":
    unittest.main()
