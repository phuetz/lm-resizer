"""Failure paths for the pinned source fetched by the release guard."""
import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest import mock
import urllib.error

SPEC = importlib.util.spec_from_file_location(
    "ensure_oracle_source", Path(__file__).with_name("ensure_oracle_source.py")
)
module = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(module)


class OracleSourceTests(unittest.TestCase):
    def test_bad_existing_archive_reports_sha_without_download(self):
        with tempfile.TemporaryDirectory() as folder:
            archive = Path(folder) / "source.tar.gz"
            archive.write_bytes(b"incorrect")
            with mock.patch.object(module, "ARCHIVE", archive), mock.patch.object(
                module.urllib.request, "urlopen", side_effect=AssertionError("network used")
            ):
                with self.assertRaisesRegex(SystemExit, "SHA-256 mismatch"):
                    module.main()

    def test_missing_network_reports_pinned_url(self):
        with tempfile.TemporaryDirectory() as folder:
            archive = Path(folder) / "source.tar.gz"
            with mock.patch.object(module, "ARCHIVE", archive), mock.patch.object(
                module.urllib.request,
                "urlopen",
                side_effect=urllib.error.URLError("offline"),
            ):
                with self.assertRaisesRegex(SystemExit, "Cannot prepare pinned RTK source"):
                    module.main()


if __name__ == "__main__":
    unittest.main()
