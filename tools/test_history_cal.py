#!/usr/bin/env python3
"""Self-test of tools/validators/history_cal.py (SPEC-011): the repository data pass, broken copies fail.

Run: python3 tools/test_history_cal.py
"""
import importlib.util
import os
import shutil
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
spec = importlib.util.spec_from_file_location("history_cal_plugin", os.path.join(HERE, "validators", "history_cal.py"))
plugin = importlib.util.module_from_spec(spec)
spec.loader.exec_module(plugin)


def sandbox(script=None, policy=None):
    """A temporary root holding the schemas, optionally with replaced script and policy text."""
    root = tempfile.mkdtemp()
    shutil.copytree(os.path.join(ROOT, "data", "schemas"), os.path.join(root, "data", "schemas"))
    for rel, text in (("data/scenarios/toy_replay/script.toml", script), ("qc/calibration/policy.toml", policy)):
        src = os.path.join(ROOT, rel)
        dst = os.path.join(root, rel)
        os.makedirs(os.path.dirname(dst), exist_ok=True)
        with open(src, encoding="utf-8") as f:
            body = f.read()
        with open(dst, "w", encoding="utf-8") as f:
            f.write(body if text is None else text(body))
    return root


class HistoryCal(unittest.TestCase):
    def check(self, **kw):
        root = sandbox(**kw)
        self.addCleanup(shutil.rmtree, root)
        return plugin.validate(root)

    def test_repository_data_pass(self):
        self.assertEqual(plugin.validate(ROOT), [])

    def test_script_faults(self):
        cases = {
            "float": lambda s: s.replace("at_s = 10\n", "at_s = 10.5\n", 1),
            "unknown key": lambda s: s.replace("[[trigger]]\n", "[[trigger]]\nbogus = 1\n"),
            "no provenance": lambda s: s.replace('calibration_target = "CT-011-1101"\n', "", 1),
            "time outside": lambda s: s.replace("at_s = 5\n", "at_s = 500\n"),
            "after": lambda s: s.replace("lo_s = 20\n", 'lo_s = 20\nafter = "nope"\n'),
            "duplicate id": lambda s: s.replace('id = "o_r1_line"', 'id = "o_bde_move"'),
        }
        for name, edit in cases.items():
            with self.subTest(name):
                self.assertTrue(self.check(script=edit), name)

    def test_policy_faults(self):
        cases = {
            "float": lambda s: s.replace("value = 900\n", "value = 900.5\n"),
            "wrong target": lambda s: s.replace("CT-011-1102", "CT-011-1103", 1),
            "missing row": lambda s: s.replace('id = "fit_rounds"', 'id = "fit_roundz"'),
            "not placeholder": lambda s: s.replace("placeholder = true", "placeholder = false", 1),
        }
        for name, edit in cases.items():
            with self.subTest(name):
                self.assertTrue(self.check(policy=edit), name)


if __name__ == "__main__":
    unittest.main()
