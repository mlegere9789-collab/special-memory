#!/usr/bin/env python3
"""Tests for tools/validate_ui.py and its plugin tools/validators/ui.py.
Run: python3 tools/test_validate_ui.py"""
import importlib.util
import os
import shutil
import tempfile
import unittest

TOOLS = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(TOOLS)


def load(path, name):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


ui = load(os.path.join(TOOLS, "validate_ui.py"), "validate_ui")
plugin = load(os.path.join(TOOLS, "validators", "ui.py"), "validators_ui")

GOOD = '[[field]]\nname = "Snapshot.tick"\nkind = "exact"\nsource = "BattleState.world.tick"\n'


def run(fields=None, golden=None):
    """Problems found in a throw-away root holding the real schema and the given files."""
    with tempfile.TemporaryDirectory() as root:
        shutil.copytree(os.path.join(REPO, "data", "schemas"), os.path.join(root, "data", "schemas"))
        for rel, text in (("data/ui/display-fields.toml", fields), ("crates/ui-model/tests/golden/snapshot.toml", golden)):
            if text is not None:
                path = os.path.join(root, *rel.split("/"))
                os.makedirs(os.path.dirname(path), exist_ok=True)
                with open(path, "w", encoding="utf-8") as f:
                    f.write(text)
        return plugin.validate(root)


class DisplayFields(unittest.TestCase):
    def test_repository_data_are_sound(self):
        self.assertEqual(ui.validate(REPO), [])
        self.assertEqual(plugin.validate(REPO), [])

    def test_nothing_to_check_is_not_a_problem(self):
        self.assertEqual(run(), [])

    def test_good_file_passes(self):
        self.assertEqual(run(fields=GOOD), [])

    def test_field_without_source_fails(self):
        problems = run(fields=GOOD.replace('source = "BattleState.world.tick"\n', ""))
        self.assertTrue(any("source" in p for p in problems), problems)

    def test_blank_source_fails(self):
        problems = run(fields=GOOD.replace("BattleState.world.tick", "  "))
        self.assertTrue(any("source" in p for p in problems), problems)

    def test_unknown_kind_fails(self):
        self.assertTrue(run(fields=GOOD.replace("exact", "guess")))

    def test_duplicate_name_fails(self):
        problems = run(fields=GOOD + "\n" + GOOD)
        self.assertTrue(any("twice" in p for p in problems), problems)


class Golden(unittest.TestCase):
    def golden(self):
        with open(os.path.join(REPO, "crates", "ui-model", "tests", "golden", "snapshot.toml"), encoding="utf-8") as f:
            return f.read()

    def test_recorded_hash_is_rederived(self):
        self.assertEqual(run(golden=self.golden()), [])

    def test_changed_field_changes_the_hash(self):
        problems = run(golden=self.golden().replace("strength = 10", "strength = 11"))
        self.assertTrue(any("hashes to" in p for p in problems), problems)


if __name__ == "__main__":
    unittest.main()
