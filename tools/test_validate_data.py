#!/usr/bin/env python3
"""Tests for validate_data.py: the data/rules path (rule R7) and the validator plugins in tools/validators/.
Run: python3 tools/test_validate_data.py"""
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


def run(rules_text=None, name="probe.toml", with_schema=True, plugins=None):
    """Build a throw-away root holding the real schemas plus one rules file; return (exit code, output).

    `plugins` maps file names to source text; when given, they are the only validator plugins the run discovers.
    """
    with tempfile.TemporaryDirectory() as root:
        shutil.copytree(os.path.join(REPO, "data", "schemas"), os.path.join(root, "data", "schemas"))
        if with_schema:
            with open(os.path.join(root, "data", "schemas", "probe.schema.json"), "w", encoding="utf-8") as f:
                json.dump(PROBE_SCHEMA, f)
        if rules_text is not None:
            os.makedirs(os.path.join(root, "data", "rules"))
            with open(os.path.join(root, "data", "rules", name), "w", encoding="utf-8") as f:
                f.write(rules_text)
        env = dict(os.environ)
        if plugins is not None:
            vdir = os.path.join(root, "plugins-under-test")
            os.makedirs(vdir)
            for fname, source in plugins.items():
                with open(os.path.join(vdir, fname), "w", encoding="utf-8") as f:
                    f.write(source)
            env["HD_VALIDATORS_DIR"] = vdir
        p = subprocess.run([sys.executable, os.path.join(TOOLS, "validate_data.py"), root],
                           capture_output=True, text=True, check=False, env=env)
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


CLEAN = "def validate(root):\n    return []\n"
FAILING = "def validate(root):\n    return ['first thing wrong', 'second thing wrong']\n"


class Plugins(unittest.TestCase):
    def test_plugin_problems_are_printed_and_counted(self):
        code, out = run(GOOD, plugins={"alpha.py": FAILING})
        self.assertEqual(code, 1, out)
        self.assertIn("alpha: first thing wrong", out)
        self.assertIn("alpha: second thing wrong", out)
        self.assertIn("1 records, 2 problems", out)

    def test_clean_plugin_leaves_the_run_green_and_output_unchanged(self):
        code, out = run(GOOD, plugins={"alpha.py": CLEAN})
        self.assertEqual(code, 0, out)
        self.assertEqual(out, run(GOOD, plugins={})[1])
        self.assertEqual(out.strip(), "validate_data: 1 records, 0 problems")

    def test_plugins_run_in_name_order_and_counts_add_up(self):
        code, out = run(GOOD.replace("1067", "1067.5"), plugins={"b.py": FAILING, "a.py": "def validate(root):\n    return ['x']\n"})
        self.assertEqual(code, 1)
        self.assertLess(out.index("a: x"), out.index("b: first thing wrong"))
        self.assertIn("1 records, 5 problems", out)

    def test_plugin_receives_the_data_root(self):
        src = ("import os\ndef validate(root):\n"
               "    ok = os.path.isfile(os.path.join(root, 'data', 'rules', 'probe.toml'))\n"
               "    return [] if ok else ['root was ' + root]\n")
        code, out = run(GOOD, plugins={"rooted.py": src})
        self.assertEqual(code, 0, out)

    def test_underscore_and_non_python_files_are_not_plugins(self):
        code, out = run(GOOD, plugins={"_helper.py": "raise SystemExit('not a plugin')", "notes.txt": "x", "ok.py": CLEAN})
        self.assertEqual(code, 0, out)

    def test_broken_plugins_fail_the_run(self):
        broken = {
            "syntax.py": "def validate(root:\n",
            "raises.py": "def validate(root):\n    raise RuntimeError('boom')\n",
            "nofunc.py": "x = 1\n",
            "wrongtype.py": "def validate(root):\n    return 'not a list'\n",
            "wrongitem.py": "def validate(root):\n    return [1]\n",
            "importerr.py": "import module_that_does_not_exist\n",
        }
        for fname, src in broken.items():
            with self.subTest(plugin=fname):
                code, out = run(GOOD, plugins={fname: src, "zz_clean.py": CLEAN})
                self.assertEqual(code, 1, out)
                self.assertIn("%s: plugin failed" % fname[:-3], out)
                self.assertIn("1 records, 1 problems", out)

    def test_repository_plugins_are_discovered_and_clean(self):
        code, out = run(GOOD)
        self.assertEqual(code, 0, out)
        sys.path.insert(0, TOOLS)
        try:
            import validate_data
        finally:
            sys.path.pop(0)
        names = [n for n, _ in validate_data.discover_plugins(os.path.join(TOOLS, "validators"))]
        self.assertIn("example", names)


if __name__ == "__main__":
    unittest.main()
