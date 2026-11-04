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
calibration_target = "CT-1"
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
        code, out = run(GOOD.replace('calibration_target = "CT-1"\n', ""))
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


if __name__ == "__main__":
    unittest.main()
