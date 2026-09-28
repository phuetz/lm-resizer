#!/usr/bin/env python3
"""Checks that materially protect the comparison protocol."""
import json
import re
import unittest
from pathlib import Path

from run import ROOT, QA, check_oracle, normalize_output, qualified_winner, rtk_invocation, tokens


class BancProtocolTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.cases = {case["id"]: case for case in json.loads((ROOT / "cases.json").read_text())}

    def test_every_original_satisfies_its_oracle(self):
        for case in self.cases.values():
            with self.subTest(case=case["id"]):
                raw = (ROOT / case["file"]).read_text()
                self.assertEqual(check_oracle(case, raw), [])

    def test_git_log_accepts_unique_abbreviation_and_detects_missing_subject(self):
        case = self.cases["git_log"]
        raw = (ROOT / case["file"]).read_text()
        shas = re.findall(r"(?m)^commit ([0-9a-f]{40})$", raw)
        self.assertEqual(len(shas), 46)
        self.assertEqual(sum(sha.startswith("aaaaaaa") for sha in shas), 1)
        abbreviated = raw.replace("a" * 40, "a" * 12)
        self.assertEqual(check_oracle(case, abbreviated), [])
        missing = check_oracle(case, abbreviated.replace("Maintenance batch 22", "[removed]"))
        self.assertIn("Maintenance batch 22", missing)
        self.assertEqual(len(missing), 1)

    def test_json_oracle_accepts_smartcrusher_and_rejects_missing_row(self):
        case = self.cases["json_large"]
        raw = (ROOT / case["file"]).read_text()
        data = json.loads(raw)
        rows = data["rows"]
        table = "[180]{amount:int,id:int,meta:json,state:string}\n" + "\n".join(
            f"{row['amount']},{row['id']},{json.dumps(row['meta'])},{row['state']}" for row in rows
        )
        compact = json.dumps({"schema": data["schema"], "count": data["count"], "rows": table})
        self.assertEqual(check_oracle(case, compact), [])
        broken = compact.replace("4299,143", "4299,999")
        self.assertGreater(len(check_oracle(case, broken)), 0)

    def test_rtk_routes_use_declared_filter_and_native_json(self):
        rtk = QA / "rtk/rtk"
        fixture = ROOT / "corpus/logs.txt"
        cmd, route, stdin = rtk_invocation(self.cases["logs"], fixture, rtk)
        self.assertEqual(cmd[-3:], ["pipe", "--filter", "log"])
        self.assertTrue(stdin)
        self.assertEqual(route, "pipe --filter log")
        cmd, route, stdin = rtk_invocation(self.cases["json_large"], ROOT / self.cases["json_large"]["file"], rtk)
        self.assertEqual(cmd[1], "json")
        self.assertFalse(stdin)
        cmd, route, stdin = rtk_invocation(self.cases["code_python"], ROOT / self.cases["code_python"]["file"], rtk)
        self.assertEqual(cmd[1], "read")

    def test_tee_path_normalization_is_invariant(self):
        first_home = Path("/tmp/long/worktree/_qa/banc/home-lm-resizer")
        second_home = Path("/tmp/b/home-lm-resizer")
        line = "test result: ok\n[full output: {}/lm-resizer/tee/{}_cargo_test.log]\n"
        first = normalize_output(line.format(first_home, "1790554222"), first_home)
        second = normalize_output(line.format(second_home, "1790554777"), second_home)
        self.assertEqual(first, second)
        self.assertEqual(tokens(first), tokens(second))

    def test_no_winner_at_zero_or_negative_savings(self):
        rows = [{"tool": "LM Resizer", "oracle_retention": 0, "error": None, "qualified_saving": 0},
                {"tool": "RTK", "oracle_retention": 1, "error": None, "qualified_saving": -0.1},
                {"tool": "Headroom", "oracle_retention": 1, "error": None, "qualified_saving": 0}]
        self.assertEqual(qualified_winner(rows), "aucun gain qualifié")
        rows[1]["qualified_saving"] = 0.2
        self.assertEqual(qualified_winner(rows), "RTK")


if __name__ == "__main__":
    unittest.main()
