import io
import tarfile
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory

from scripts.check_vendor_drift import archive_files, unexpected_drift


class VendorDriftTests(unittest.TestCase):
    def test_only_allowlisted_differences_are_accepted(self):
        with TemporaryDirectory() as directory:
            root = Path(directory)
            upstream = root / "upstream"
            vendor = root / "vendor"
            upstream.mkdir()
            vendor.mkdir()
            (upstream / "same.rs").write_text("same", encoding="utf-8")
            (vendor / "same.rs").write_text("same", encoding="utf-8")
            (upstream / "patched.rs").write_text("old", encoding="utf-8")
            (vendor / "patched.rs").write_text("new", encoding="utf-8")

            self.assertEqual(unexpected_drift(upstream, vendor, {"patched.rs"}), [])

            (vendor / "same.rs").write_text("drift", encoding="utf-8")
            self.assertEqual(
                unexpected_drift(upstream, vendor, {"patched.rs"}), ["same.rs"]
            )

    def test_archive_symlink_matches_windows_git_materialization(self):
        archive_buffer = io.BytesIO()
        with tarfile.open(fileobj=archive_buffer, mode="w:gz") as archive:
            regular = tarfile.TarInfo("gpui-box-revision/src/lib.rs")
            regular_contents = b"pub fn example() {}\n"
            regular.size = len(regular_contents)
            archive.addfile(regular, io.BytesIO(regular_contents))

            symlink = tarfile.TarInfo("gpui-box-revision/crates/example/LICENSE")
            symlink.type = tarfile.SYMTYPE
            symlink.linkname = "../../LICENSE-APACHE"
            archive.addfile(symlink)

        archive_buffer.seek(0)
        files = archive_files(archive_buffer)

        self.assertEqual(files["src/lib.rs"], regular_contents)
        self.assertEqual(files["crates/example/LICENSE"], b"../../LICENSE-APACHE")


if __name__ == "__main__":
    unittest.main()
