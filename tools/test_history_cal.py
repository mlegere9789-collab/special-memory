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


SOURCES = (
    "data/scenarios/gb_timeline/script.toml",
    "qc/calibration/policy.toml",
    "qc/calibration/gettysburg-bands.toml",
    "qc/fixtures/history-conflicts.toml",
    "qc/fixtures/gb-known-conflicts.toml",
)


class GettysburgTables(unittest.TestCase):
    """The band, timeline and conflict tables of T-114 (SPEC-011 section 7)."""

    def check(self, **edits):
        root = tempfile.mkdtemp()
        self.addCleanup(shutil.rmtree, root)
        shutil.copytree(os.path.join(ROOT, "data", "schemas"), os.path.join(root, "data", "schemas"))
        for rel in SOURCES:
            dst = os.path.join(root, rel)
            os.makedirs(os.path.dirname(dst), exist_ok=True)
            with open(os.path.join(ROOT, rel), encoding="utf-8") as f:
                body = f.read()
            with open(dst, "w", encoding="utf-8") as f:
                f.write(edits.get(rel, lambda t: t)(body))
        return plugin.validate(root)

    def test_repository_tables_pass(self):
        self.assertEqual(self.check(), [])

    def test_other_band_families_are_not_history_bands(self):
        """A `*-bands.toml` file of another family (SPEC-007 command bands) keeps its own format."""
        other = 'family = "command"\n[[band]]\nid = "C1"\nphrase_free = true\n'
        faulty = 'schema = "history-bands/1"\n[[band]]\nid = "X-1"\n'
        root = tempfile.mkdtemp()
        self.addCleanup(shutil.rmtree, root)
        shutil.copytree(os.path.join(ROOT, "data", "schemas"), os.path.join(root, "data", "schemas"))
        for rel in SOURCES:
            dst = os.path.join(root, rel)
            os.makedirs(os.path.dirname(dst), exist_ok=True)
            shutil.copyfile(os.path.join(ROOT, rel), dst)
        cal = os.path.join(root, "qc", "calibration")
        with open(os.path.join(cal, "other-bands.toml"), "w", encoding="utf-8") as f:
            f.write(other)
        self.assertEqual(plugin.validate(root), [])
        with open(os.path.join(cal, "faulty-bands.toml"), "w", encoding="utf-8") as f:
            f.write(faulty)
        self.assertTrue(plugin.validate(root), "a history-bands file with a fault must be reported")

    def test_band_faults(self):
        bands = "qc/calibration/gettysburg-bands.toml"
        cases = {
            "edge off the widening": lambda s: s.replace("low = 7650", "low = 7000", 1),
            "phrase changed": lambda s: s.replace('phrase = "nearly"', 'phrase = "about"', 1),
            "verified without second source": lambda s: s.replace('confidence = "partial"', 'confidence = "verified"', 1),
            "float": lambda s: s.replace("low = 7650", "low = 7650.5", 1),
            "id missing": lambda s: s.replace('id = "HB-24"', 'id = "HB-25"'),
            "unknown key": lambda s: s.replace("[[band]]\n", "[[band]]\nbogus = 1\n", 1),
            "no source": lambda s: s.replace(
                'sources = [{ document = "CWV", page = 116, tag = "untagged" }]\n', "", 1),
        }
        for name, edit in cases.items():
            with self.subTest(name):
                self.assertTrue(self.check(**{bands: edit}), name)

    def test_abt_sums_are_recomputed(self):
        conflicts = "qc/fixtures/gb-known-conflicts.toml"
        for name, edit in {
            "component changed": lambda s: s.replace("parts = [3155, 14529, 5365]", "parts = [3155, 14529, 5366]"),
            "total changed": lambda s: s.replace("total = 28063, agrees = true", "total = 28064, agrees = true"),
        }.items():
            with self.subTest(name):
                self.assertTrue(self.check(**{conflicts: edit}), name)
        bands = "qc/calibration/gettysburg-bands.toml"
        problems = self.check(**{bands: lambda s: s.replace("source_value = 17684", "source_value = 17685")})
        self.assertTrue(any("HB-09" in p for p in problems), problems)

    def test_fixture_and_timeline_faults(self):
        fixtures = "qc/fixtures/history-conflicts.toml"
        script = "data/scenarios/gb_timeline/script.toml"
        cases = [
            (fixtures, lambda s: s.replace('ref = "HB-06"', 'ref = "HB-99"')),
            (fixtures, lambda s: s.replace('rule = "envelope"', 'rule = "vibes"', 1)),
            (fixtures, lambda s: s.replace('id = "CF-10"', 'id = "CF-11"')),
            (fixtures, lambda s: s.replace('ref = "tl_10"', 'ref = "tl_99"')),
            (fixtures, lambda s: s.replace("value_a = 28063", "value_a = 28063.5", 1)),
            (script, lambda s: s.replace('id = "tl_15"', 'id = "tl_16"')),
        ]
        for i, (rel, edit) in enumerate(cases):
            with self.subTest(i):
                self.assertTrue(self.check(**{rel: edit}), i)


if __name__ == "__main__":
    unittest.main()
