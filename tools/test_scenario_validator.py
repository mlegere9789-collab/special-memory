#!/usr/bin/env python3
"""Self-tests for tools/validators/scenario.py (SPEC-008 acceptance T8-02)."""
import importlib.util
import os
import shutil
import sys
import tempfile
import unittest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
spec = importlib.util.spec_from_file_location("scenario_plugin", os.path.join(ROOT, "tools", "validators", "scenario.py"))
plugin = importlib.util.module_from_spec(spec)
spec.loader.exec_module(plugin)

SMALL = os.path.join(ROOT, "data", "scenarios", "fid-small.toml")
FIXTURES = os.path.join(ROOT, "qc", "fixtures", "scenario-bad")


def read(path):
    with open(path, encoding="utf-8") as f:
        return f.read()


class Sandbox:
    """A repository root whose data/scenarios holds only the files given; the rest is linked."""

    def __init__(self, files):
        self.dir = tempfile.mkdtemp()
        data = os.path.join(self.dir, "data")
        os.makedirs(os.path.join(data, "scenarios"))
        for name in ("schemas", "rules", "history"):
            os.symlink(os.path.join(ROOT, "data", name), os.path.join(data, name))
        for rel, text in files.items():
            path = os.path.join(data, "scenarios", rel)
            os.makedirs(os.path.dirname(path), exist_ok=True)
            with open(path, "w", encoding="utf-8") as f:
                f.write(text)

    def validate(self):
        return plugin.validate(self.dir)

    def close(self):
        shutil.rmtree(self.dir)


class ScenarioPlugin(unittest.TestCase):
    def check(self, text, name="fid-small.toml"):
        box = Sandbox({name: text})
        self.addCleanup(box.close)
        return box.validate()

    def test_shipped_data_and_fixtures_are_clean(self):
        self.assertEqual(plugin.validate(ROOT), [])

    def test_each_rule_has_a_fixture_flagged_with_exactly_that_rule(self):
        ctx = plugin.Ctx(ROOT)
        seen = set()
        for name in sorted(os.listdir(FIXTURES)):
            if not name.endswith(".toml"):
                continue
            rules = {r for r, _ in plugin.check_file(ctx, os.path.join(FIXTURES, name))}
            self.assertEqual(rules, {name[:3].upper()}, name)
            seen.add(name[:3].upper())
        self.assertEqual(seen, {"V%02d" % n for n in range(1, 13)})

    def test_sidecar_directories_are_not_scenarios(self):
        box = Sandbox({"fid-small.toml": read(SMALL), "gettysburg/weather.toml": "this is not = [valid"})
        self.addCleanup(box.close)
        self.assertEqual(box.validate(), [])

    def test_unknown_weapon_and_commander_are_references(self):
        text = read(SMALL).replace('weapon = "carbine_52"', 'weapon = "pistol_99"').replace("john_buford", "nobody_here")
        found = self.check(text)
        self.assertEqual(len(found), 2, found)
        self.assertTrue(all("V03" in p for p in found), found)

    def test_unknown_key_and_float_are_v01(self):
        self.assertTrue(any("V01" in p for p in self.check(read(SMALL).replace("tick_hz = 20", "tick_hz = 20\nmystery = 1"))))
        self.assertTrue(any("V01" in p for p in self.check(read(SMALL).replace("wet_pm = 0", "wet_pm = 0.5", 1))))

    def test_strength_and_file_name(self):
        self.assertTrue(any("V07" in p for p in self.check(read(SMALL).replace("men = 2700\ntol_men", "men = 2000\ntol_men"))))
        self.assertTrue(any("V02" in p for p in self.check(read(SMALL), name="other.toml")))

    def test_terrain_rows_hash_matches_the_rust_loader(self):
        # The same literal is asserted by crates/battle-rules/tests/t8_01_scenario.rs.
        self.assertEqual(plugin.rows_hash(plugin.rows_of(".W\n..\n")), 0xD51F0FCC896BEF1A)
        self.assertEqual(plugin.rows_hash(plugin.rows_of(".W\r\n..")), 0xD51F0FCC896BEF1A)

    def test_terrain_source_file(self):
        text = read(SMALL)
        a = text.index('rows = """')
        b = text.index('"""', a + 10) + 4
        rows = text[a + 11:b - 4]
        digest = "%016x" % plugin.rows_hash(plugin.rows_of(rows))
        sourced = text[:a] + 'source = "terrain/pike.rows"\nsource_hash = "%s"\n' % digest + text[b:]
        box = Sandbox({"fid-small.toml": sourced, "terrain/pike.rows": rows})
        self.addCleanup(box.close)
        self.assertEqual(box.validate(), [])
        box2 = Sandbox({"fid-small.toml": sourced, "terrain/pike.rows": rows + "........\n"})
        self.addCleanup(box2.close)
        self.assertTrue(any("V10" in p for p in box2.validate()))


if __name__ == "__main__":
    sys.exit(unittest.main(verbosity=1))
