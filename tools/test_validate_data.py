#!/usr/bin/env python3
"""Tests for the data/rules path of validate_data.py (rule R7). Run: python3 tools/test_validate_data.py"""
import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest

TOOLS = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(TOOLS)

PROBE_SCHEMA = {
    "$schema": "https://json-schema.org/draft/2020-12/schema",
    "type": "object",
    "required": ["schema", "row"],
    "properties": {
        "schema": {"const": "probe/1"},
        "row": {"type": "array", "items": {"type": "object", "required": ["speed_mm_s"],
                                           "properties": {"speed_mm_s": {"type": "integer", "minimum": 1}}}},
    },
}

GOOD = '''schema = "probe/1"
[[row]]
speed_mm_s = 1067
confidence = "unverified"
placeholder = true
calibration_target = "CT-004-1"
[[row]]
speed_mm_s = 5
confidence = "partial"
sources = [{ document = "CWV", page = 511 }]
'''


def run(rules_text=None, name="probe.toml", with_schema=True):
    """Build a throw-away root holding the real schemas plus one rules file; return (exit code, output)."""
    with tempfile.TemporaryDirectory() as root:
        shutil.copytree(os.path.join(REPO, "data", "schemas"), os.path.join(root, "data", "schemas"))
        if with_schema:
            with open(os.path.join(root, "data", "schemas", "probe.schema.json"), "w", encoding="utf-8") as f:
                json.dump(PROBE_SCHEMA, f)
        if rules_text is not None:
            os.makedirs(os.path.join(root, "data", "rules"))
            with open(os.path.join(root, "data", "rules", name), "w", encoding="utf-8") as f:
                f.write(rules_text)
        p = subprocess.run([sys.executable, os.path.join(TOOLS, "validate_data.py"), root],
                           capture_output=True, text=True, check=False)
        return p.returncode, p.stdout


class RulesPath(unittest.TestCase):
    def test_no_rules_directory_is_fine(self):
        code, out = run(None)
        self.assertEqual(code, 0, out)

    def test_good_file_passes(self):
        code, out = run(GOOD)
        self.assertEqual(code, 0, out)
        self.assertIn("1 records, 0 problems", out)

    def test_float_is_reported(self):
        code, out = run(GOOD.replace("1067", "1067.5"))
        self.assertEqual(code, 1)
        self.assertIn("float", out)

    def test_schema_violation_is_reported(self):
        code, out = run(GOOD.replace("speed_mm_s = 5", "speed_mm_s = 0"))
        self.assertEqual(code, 1)
        self.assertIn("schema", out)

    def test_placeholder_without_target_is_reported(self):
        code, out = run(GOOD.replace('calibration_target = "CT-004-1"\n', ""))
        self.assertEqual(code, 1)
        self.assertIn("provenance", out)

    def test_non_placeholder_without_sources_is_reported(self):
        code, out = run(GOOD.replace('sources = [{ document = "CWV", page = 511 }]\n', ""))
        self.assertEqual(code, 1)
        self.assertIn("provenance", out)

    def test_verified_without_second_source_is_reported(self):
        code, out = run(GOOD.replace('"partial"', '"verified"'))
        self.assertEqual(code, 1)
        self.assertIn("provenance", out)

    def test_missing_schema_is_reported(self):
        code, out = run(GOOD, with_schema=False)
        self.assertEqual(code, 1)
        self.assertIn("no schema", out)


RULES_FILES = {
    "locomotion.toml": "CT-004-",
    "terrain.toml": "CT-004-",
    "drill.toml": "CT-004-",
    "ammo.toml": "CT-005-",
    "weapons.toml": "CT-005-",
    "damage.toml": "CT-005-",
}
CT_PATTERN = "^CT-[0-9]{3}-[0-9]+$"


def real_rules(name):
    with open(os.path.join(REPO, "data", "rules", name), encoding="utf-8") as f:
        return f.read()


class CalibrationTargetFormat(unittest.TestCase):
    """TR-02: calibration targets are CT-<spec>-<n>; the bare CT-n form is refused."""

    def test_accepted_forms(self):
        for target in ("CT-004-1", "CT-005-9", "CT-011-1101", "CT-016-11", "CT-000-0"):
            code, out = run(GOOD.replace("CT-004-1", target))
            self.assertEqual(code, 0, "%s: %s" % (target, out))

    def test_refused_forms(self):
        for target in ("CT-1", "CT-12", "CT-4-1", "CT-04-1", "CT-0004-1", "CT-00a-1", "CT-a04-1",
                       "CT-004-", "CT-004", "CT-004-1a", "CT-004--1", "CT-004-1-2", "ct-004-1",
                       "XX-004-1", " CT-004-1", "CT-004-1 ", "first", ""):
            code, out = run(GOOD.replace("CT-004-1", target))
            self.assertEqual(code, 1, "%s must be refused: %s" % (target, out))
            self.assertIn("provenance", out)

    def test_every_schema_copy_has_the_new_pattern(self):
        seen = 0
        for name in ("rules-common", "ammo", "damage", "drill", "weapons"):
            with open(os.path.join(REPO, "data", "schemas", name + ".schema.json"), encoding="utf-8") as f:
                doc = json.load(f)
            found = []

            def walk(node):
                if isinstance(node, dict):
                    ct = node.get("calibration_target")
                    if isinstance(ct, dict) and "pattern" in ct:
                        found.append(ct["pattern"])
                    for v in node.values():
                        walk(v)
                elif isinstance(node, list):
                    for v in node:
                        walk(v)

            walk(doc)
            self.assertTrue(found, name)
            for pattern in found:
                self.assertEqual(pattern, CT_PATTERN, name)
                seen += 1
        self.assertGreaterEqual(seen, 5)

    def test_shipped_rows_validate_in_the_new_form(self):
        for name, prefix in RULES_FILES.items():
            text = real_rules(name)
            self.assertIn(prefix, text, name)
            self.assertNotRegex(text, r"\bCT-[0-9]+(?![0-9-])", name)
            code, out = run(text, name=name, with_schema=False)
            self.assertEqual(code, 0, "%s: %s" % (name, out))

    def test_shipped_rows_in_the_old_form_are_refused(self):
        for name, prefix in RULES_FILES.items():
            old = real_rules(name).replace('calibration_target = "' + prefix, 'calibration_target = "CT-', 1)
            self.assertNotEqual(old, real_rules(name), name)
            code, out = run(old, name=name, with_schema=False)
            self.assertEqual(code, 1, "%s: %s" % (name, out))


if __name__ == "__main__":
    unittest.main()
