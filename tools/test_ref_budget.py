#!/usr/bin/env python3
"""The committed output of qc/fixtures/ref_budget.py (the Python side of T10-08) is current and
satisfies the budget rule of SPEC-010 section 7.
Run: python3 tools/test_ref_budget.py"""
import contextlib
import importlib.util
import io
import os
import unittest

TOOLS = os.path.dirname(os.path.abspath(__file__))
FIXTURES = os.path.join(os.path.dirname(TOOLS), "qc", "fixtures")


def load(path):
    spec = importlib.util.spec_from_file_location("ref_budget", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


ref = load(os.path.join(FIXTURES, "ref_budget.py"))


class RefBudget(unittest.TestCase):
    def test_committed_output_is_current(self):
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            ref.main()
        with open(os.path.join(FIXTURES, "ref_budget.csv"), encoding="utf-8") as f:
            self.assertEqual(f.read(), out.getvalue())

    def test_rows_obey_the_rule(self):
        with open(os.path.join(FIXTURES, "ref_budget.csv"), encoding="utf-8") as f:
            rows = [line.split(",") for line in f.read().splitlines()]
        self.assertGreaterEqual(len(rows), 6)
        for kind, *nums in rows:
            budget, c, s, n, h, cu, su = (int(x) for x in nums)
            self.assertTrue(100 <= h <= 1200)
            self.assertTrue(1 <= cu <= c and 1 <= su <= s)
            if kind == "plan" and (cu, su) == (c, s):
                self.assertEqual(h, min(max(budget // (c * s * max(n, 1)), 100), 1200))

    def test_dropping_keeps_samples_first(self):
        h, c, s = ref.plan(6_000_000, 3, 2, 50_000)
        self.assertEqual((h, c, s), (120, 1, 1))
        h, c, s = ref.plan(2_000_000, 2, 1, 30_000)
        self.assertEqual((h, c, s), (100, 1, 1))


if __name__ == "__main__":
    unittest.main()
