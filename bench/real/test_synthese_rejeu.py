#!/usr/bin/env python3
"""Le verdict de rejouer.sh doit refuser ce qui n'est pas la barrière demandée."""
import copy
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

import synthese_rejeu as synthese

SCRIPT = Path(__file__).with_name("synthese_rejeu.py")


def results(n=61, median=30.0, mean=35.0, oracle_median=15.81, **override):
    row = dict(parity=True, tee_verified=True, producer_exit_preserved=True)
    data = {
        "lm_sha256": "a" * 64,
        "corpus_sha256": "b" * 64,
        "median_goal_met": median >= oracle_median,
        "summary": {
            "lm_total_tokens": {"cases": n, "median": median, "mean": mean},
            "rtk_tokens": {"cases": n, "median": oracle_median, "mean": 32.6},
            "headroom_tokens": {"cases": n, "median": 0.0, "mean": 1.9},
        },
        "cases": [copy.deepcopy(row) for _ in range(n)],
    }
    data.update(override)
    return data


class Verdict(unittest.TestCase):
    def failures(self, data, **kwargs):
        return synthese.verdict(data, **kwargs)[1]

    def test_real_candidate_numbers_pass(self):
        self.assertEqual(self.failures(results(median=25.1827, mean=34.6819)), [])
        self.assertEqual(self.failures(results()), [])

    def test_one_capture_is_not_the_corpus(self):
        # Le contre-exemple de la contre-revue : une capture, médiane 16, moyenne 0, « tenu ».
        data = results(n=1, median=16.0, mean=0.0)
        failures = self.failures(data)
        self.assertTrue(any("1 captures" in f for f in failures), failures)
        self.assertTrue(any("médiane" in f for f in failures), failures)
        self.assertTrue(any("moyenne" in f for f in failures), failures)

    def test_median_below_the_published_floor_fails(self):
        self.assertTrue(self.failures(results(median=25.17)))
        self.assertTrue(self.failures(results(median=20.0)))

    def test_mean_below_the_published_floor_fails(self):
        failures = self.failures(results(mean=34.67))
        self.assertEqual(len(failures), 1, failures)
        self.assertIn("moyenne", failures[0])

    def test_median_below_the_oracle_fails_even_above_the_floor(self):
        data = results(median=30.0, oracle_median=31.0)
        self.assertTrue(any("oracle" in f for f in self.failures(data)))

    def test_lost_recovery_or_exit_code_fails(self):
        data = results()
        data["cases"][3]["tee_verified"] = False
        data["cases"][4]["producer_exit_preserved"] = False
        failures = self.failures(data)
        self.assertTrue(any("brut" in f for f in failures), failures)
        self.assertTrue(any("code" in f for f in failures), failures)

    def test_strict_view_differences_alone_do_not_fail(self):
        data = results()
        for row in data["cases"][:20]:
            row["parity"] = False
        self.assertEqual(self.failures(data), [])

    def test_thresholds_can_be_raised(self):
        self.assertTrue(self.failures(results(median=30.0), mediane_min=31.0))


class CommandLine(unittest.TestCase):
    def run_script(self, data, *extra):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "results.json"
            path.write_text(json.dumps(data), encoding="utf-8")
            return subprocess.run(
                [sys.executable, "-I", str(SCRIPT), str(path), *extra],
                capture_output=True, text=True, check=False,
            )

    def test_exit_codes_follow_the_verdict(self):
        good = self.run_script(results())
        self.assertEqual(good.returncode, 0, good.stdout + good.stderr)
        self.assertIn("VERDICT : tenu", good.stdout)
        bad = self.run_script(results(n=1, median=16.0, mean=0.0))
        self.assertEqual(bad.returncode, 1)
        self.assertIn("NON tenu", bad.stdout)


if __name__ == "__main__":
    unittest.main()
