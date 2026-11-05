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


def run(fields=None, golden=None, **params):
    """Problems found in a throw-away root holding the real schema and the given files; `params`
    maps confidence, status, morale or symptoms to the text of data/ui/<name>.toml."""
    with tempfile.TemporaryDirectory() as root:
        shutil.copytree(os.path.join(REPO, "data", "schemas"), os.path.join(root, "data", "schemas"))
        files = [("data/ui/display-fields.toml", fields), ("crates/ui-model/tests/golden/snapshot.toml", golden)]
        files += [("data/ui/%s.toml" % name, text) for name, text in params.items()]
        for rel, text in files:
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


def real(name):
    with open(os.path.join(REPO, "data", "ui", name + ".toml"), encoding="utf-8") as f:
        return f.read()


class Params(unittest.TestCase):
    def test_real_files_pass(self):
        names = ("confidence", "status", "morale", "symptoms")
        self.assertEqual(run(**{n: real(n) for n in names}), [])

    def test_placeholder_without_target_fails(self):
        problems = run(status=real("status").replace('calibration_target = "CT-015-2"\n', ""))
        self.assertTrue(any("calibration_target" in p for p in problems), problems)

    def test_target_of_another_spec_fails(self):
        self.assertTrue(run(status=real("status").replace("CT-015-2", "CT-007-2")))

    def test_bare_target_fails(self):
        self.assertTrue(run(status=real("status").replace("CT-015-2", "CT-2")))

    def test_verified_without_second_source_fails(self):
        text = real("confidence").replace('"unverified"', '"verified"')
        text = text.replace("placeholder = true", 'placeholder = false\nsources = [{ document = "CWV", page = 1 }]')
        problems = run(confidence=text)
        self.assertTrue(any("second_source" in p for p in problems), problems)

    def test_non_placeholder_without_sources_fails(self):
        self.assertTrue(run(confidence=real("confidence").replace("placeholder = true", "placeholder = false")))

    def test_float_and_unknown_key_fail(self):
        self.assertTrue(run(confidence=real("confidence").replace("age_pm_per_s = 5", "age_pm_per_s = 5.5")))
        self.assertTrue(run(confidence=real("confidence").replace("[params]", "[params]\nspare = 1")))

    def test_wrong_schema_key_fails(self):
        self.assertTrue(run(morale=real("morale").replace("ui-morale/1", "ui-morale/7")))

    def test_moderate_above_large_fails(self):
        problems = run(morale=real("morale").replace("moderate_pm = 80", "moderate_pm = 300"))
        self.assertTrue(any("moderate_pm" in p for p in problems), problems)

    def test_levels_must_differ(self):
        problems = run(symptoms=real("symptoms").replace("icon = 4", "icon = 3"))
        self.assertTrue(any("icon" in p for p in problems), problems)
        same = real("symptoms").replace("fire_slowed = true", "fire_slowed = false")
        self.assertTrue(any("symptom set" in p for p in run(symptoms=same)))

    def test_levels_must_be_zero_to_four(self):
        self.assertTrue(any("levels" in p for p in run(symptoms=real("symptoms").replace("id = 4", "id = 3"))))


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
