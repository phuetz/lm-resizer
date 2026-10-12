"""The benchmark must compare the final visible view without cutting it."""
import importlib.util
from pathlib import Path
import unittest

MODULE = Path(__file__).with_name("parity_rtk.py")
SPEC = importlib.util.spec_from_file_location("parity_rtk", MODULE)
parity_rtk = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(parity_rtk)


class VisibleBodyTests(unittest.TestCase):
    def test_status_prefix_and_diagnostic_tail_survive(self):
        output = "[FAIL] Command failed (exit code: 2)\nE   assert 42 == 43\n"
        report = {"output": output, "filtered_bytes": 19, "tee_hint": None}
        self.assertEqual(parity_rtk.visible_body(report), output.encode())

    def test_only_the_exact_tee_trailer_is_removed(self):
        output = "E   assert 42 == 43\n[tee:abc123] lm-resizer tee read abc123\n"
        report = {"output": output, "filtered_bytes": 1, "tee_hint": "[raw: abc123]"}
        self.assertEqual(parity_rtk.visible_body(report), b"E   assert 42 == 43\n")


if __name__ == "__main__":
    unittest.main()
