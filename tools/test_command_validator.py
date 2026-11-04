#!/usr/bin/env python3
"""Tests for tools/validators/command.py against the officers fixture shared with the Rust test T7-01.
Run: python3 tools/test_command_validator.py"""
import importlib.util
import os
import shutil
import tempfile
import unittest

TOOLS = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(TOOLS)
GOOD = os.path.join(REPO, "qc", "fixtures", "command", "officers_good.toml")

spec = importlib.util.spec_from_file_location("command_validator", os.path.join(TOOLS, "validators", "command.py"))
plugin = importlib.util.module_from_spec(spec)
spec.loader.exec_module(plugin)


def run(officers_text=None, table_text=None):
    """Build a throw-away root with the real schemas and the given files; return the plugin's problems."""
    with tempfile.TemporaryDirectory() as root:
        shutil.copytree(os.path.join(REPO, "data", "schemas"), os.path.join(root, "data", "schemas"))
        if officers_text is not None:
            os.makedirs(os.path.join(root, "data", "scenarios", "fam"))
            with open(os.path.join(root, "data", "scenarios", "fam", "officers.toml"), "w", encoding="utf-8") as f:
                f.write(officers_text)
        if table_text is not None:
            os.makedirs(os.path.join(root, "data", "rules"))
            with open(os.path.join(root, "data", "rules", "command.toml"), "w", encoding="utf-8") as f:
                f.write(table_text)
        return plugin.validate(root)


def good():
    with open(GOOD, encoding="utf-8") as f:
        return f.read()


def table():
    with open(os.path.join(REPO, "data", "rules", "command.toml"), encoding="utf-8") as f:
        return f.read()


class Officers(unittest.TestCase):
    def mutated(self, old, new):
        text = good()
        self.assertIn(old, text)
        return run(text.replace(old, new, 1))

    def assertProblem(self, problems, word):
        self.assertTrue(any(word in p for p in problems), problems)

    def test_good_fixture_and_absent_data_pass(self):
        self.assertEqual(run(good()), [])
        self.assertEqual(run(), [])

    def test_cycle(self):
        self.assertProblem(self.mutated("level = 2\nparent = 2", "level = 2\nparent = 4"), "cycle")

    def test_level_is_parent_level_plus_one(self):
        self.assertProblem(self.mutated("level = 2\nparent = 2", "level = 3\nparent = 2"), "parent level + 1")

    def test_two_armies(self):
        self.assertProblem(self.mutated("id = 2\nside = 0\nlevel = 1\nparent = 1", "id = 2\nside = 0\nlevel = 0"), "two armies")

    def test_regiment_owned_twice(self):
        self.assertProblem(self.mutated("regiment = 1", "regiment = 0"), "owned twice")

    def test_links_and_deputies(self):
        self.assertProblem(self.mutated("a = 6\nb = 5", "a = 6\nb = 6"), "himself")
        self.assertProblem(self.mutated("a = 6\nb = 5", "a = 6\nb = 99"), "does not exist")
        self.assertProblem(self.mutated("a = 6\nb = 5", "a = 1\nb = 10"), "crosses sides")
        self.assertProblem(self.mutated("deputy = 3", "deputy = 14"), "deputy is on the other side")
        self.assertProblem(run(good() + "\n[[link]]\nmedium = \"flags\"\na = 5\nb = 6\n"), "repeated")

    def test_schema_faults(self):
        self.assertProblem(self.mutated("post_range_pm = 1500", "post_range_pm = 0"), "schema")
        self.assertProblem(self.mutated("hq_y_mm = 2000000\n", ""), "schema")
        self.assertProblem(self.mutated("regiment = 2\n", ""), "schema")
        self.assertProblem(self.mutated("hq_x_mm", "rank = 3\nhq_x_mm"), "schema")
        self.assertProblem(self.mutated("level = 1\nparent = 1", "level = 1"), "schema")


class Table(unittest.TestCase):
    def test_shipped_table_passes(self):
        self.assertEqual(run(table_text=table()), [])

    def test_cross_row_faults(self):
        t = table()
        self.assertTrue(any("flags" in p for p in run(table_text=t.replace('id = "flags"', 'id = "courier"', 1))))
        self.assertTrue(any("speed" in p for p in run(table_text=t.replace("speed_mm_s = 3500", "speed_mm_s = 0", 1))))
        self.assertTrue(any("spec 007" in p for p in run(table_text=t.replace("CT-007-3", "CT-004-3", 1))))


if __name__ == "__main__":
    unittest.main()
