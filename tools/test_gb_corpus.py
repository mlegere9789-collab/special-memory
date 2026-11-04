#!/usr/bin/env python3
"""Tests for the corpus loader and structural rules of tools/gb_audit.py (SPEC-013, ticket T-131; T13-01 probes).
Run: python3 tools/test_gb_corpus.py"""
import os
import shutil
import sys
import tempfile
import unittest

TOOLS = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(TOOLS)
sys.path.insert(0, TOOLS)

import gb_audit  # noqa: E402

FIX = os.path.join(REPO, "qc", "fixtures", "gb")


def files_under(d):
    return sorted(os.path.join(dp, n) for dp, _, ns in os.walk(d) for n in ns if n.endswith(".toml"))


def expected_code(path):
    with open(path, encoding="utf-8") as f:
        first = f.readline().strip()
    assert first.startswith("# expect: GB"), path
    return first[len("# expect: "):]


def corpus_with(*extra):
    """A temporary copy of the good tree plus extra (relative path, text) files; returns (tempdir, corpus)."""
    tmp = tempfile.TemporaryDirectory()
    shutil.copytree(os.path.join(FIX, "good"), os.path.join(tmp.name, "tree"))
    for rel, text in extra:
        p = os.path.join(tmp.name, "tree", rel)
        os.makedirs(os.path.dirname(p), exist_ok=True)
        with open(p, "w", encoding="utf-8") as f:
            f.write(text)
    return tmp, gb_audit.load_corpus(os.path.join(tmp.name, "tree"))


def read(rel):
    with open(os.path.join(FIX, "good", rel), encoding="utf-8") as f:
        return f.read()


class Probes(unittest.TestCase):
    """T13-01: every good record validates; every bad probe fails with its expected code."""

    def test_good_tree_is_clean(self):
        c = gb_audit.load_corpus(os.path.join(FIX, "good"))
        self.assertGreaterEqual(len(c.records), 10)
        self.assertEqual(gb_audit.check_corpus(c), [])

    def test_every_kind_has_a_good_record(self):
        c = gb_audit.load_corpus(os.path.join(FIX, "good"))
        self.assertEqual(sorted({r.kind for r in c.records}), sorted(gb_audit.GB_SCHEMAS))

    def test_each_bad_probe_fails_with_its_code(self):
        bad = files_under(os.path.join(FIX, "bad"))
        self.assertGreaterEqual(len(bad), 14)
        for path in bad:
            rel = os.path.relpath(path, os.path.join(FIX, "bad"))
            with open(path, encoding="utf-8") as f:
                text = f.read()
            tmp, c = corpus_with((rel, text))
            with tmp:
                found = gb_audit.check_corpus(c)
                mine = [f for f in found if f.path == rel.replace(os.sep, "/")]
                self.assertIn(expected_code(path), [f.rule for f in mine], "%s: %s" % (rel, found))
                if expected_code(path) != "GB03":  # a duplicate id is reported on the later of the two files
                    self.assertEqual([f for f in found if f not in mine], [], rel)

    def test_every_rule_in_scope_has_a_probe(self):
        codes = {expected_code(p) for p in files_under(os.path.join(FIX, "bad"))}
        for rule in ("GB01", "GB02", "GB03", "GB07", "GB08", "GB09"):
            self.assertIn(rule, codes)


class Loader(unittest.TestCase):
    def test_walk_is_sorted_and_labels_are_relative(self):
        c = gb_audit.load_corpus(os.path.join(FIX, "good"))
        keys = [(r.kind, r.label) for r in c.records]
        self.assertEqual(keys, sorted(keys))
        self.assertTrue(all(not os.path.isabs(r.label) and "\\" not in r.label for r in c.records))
        self.assertEqual({r.stem for r in c.records if r.kind == "weather"}, {"wx_18630701"})

    def test_unreadable_toml_is_gb01(self):
        tmp, c = corpus_with(("units/us_broken.toml", "id = = 1\n"))
        with tmp:
            self.assertEqual([(f.path, f.rule) for f in gb_audit.check_corpus(c)], [("units/us_broken.toml", "GB01")])

    def test_repository_corpus_loads_and_is_sorted(self):
        c = gb_audit.load_corpus(REPO)
        found = gb_audit.check_corpus(c)
        self.assertEqual(found, sorted(found))
        self.assertEqual([f for f in found if f.severity == "E"], [])

    def test_output_is_repeatable(self):
        tmp, c = corpus_with(("units/us_x.toml", "id = 1\n"))
        with tmp:
            self.assertEqual(gb_audit.check_corpus(c), gb_audit.check_corpus(gb_audit.load_corpus(c.root)))


class Rules(unittest.TestCase):
    def rules(self, rel, text):
        tmp, c = corpus_with((rel, text))
        with tmp:
            return [(f.path, f.rule) for f in gb_audit.check_corpus(c)]

    def test_shared_endpoint_of_organic_assignments_is_not_an_overlap(self):
        t = read("units/us_probe_brigade.toml")
        t = t.replace("us_probe_brigade", "us_probe_brigade_two")
        t = t.replace('kind = "organic"', 'kind = "organic"\nfrom = "1863-07-01T00:00"\nto = "1863-07-02T00:00"')
        t += '\n[[assignments]]\nparent = "us_probe_division"\nkind = "organic"\nconfidence = "unverified"\n' \
             'sources = [{ document = "CWV", page = 72 }]\nfrom = "1863-07-02T00:00"\nto = "1863-07-03T00:00"\n'
        self.assertEqual(self.rules("units/us_probe_brigade_two.toml", t), [])
        self.assertEqual(self.rules("units/us_probe_brigade_two.toml", t.replace('from = "1863-07-02T00:00"\nto = "1863-07-03T00:00"',
                                                                                  'from = "1863-07-01T23:59"\nto = "1863-07-03T00:00"')),
                         [("units/us_probe_brigade_two.toml", "GB09")])

    def test_battery_attaches_to_any_group_level_but_not_a_regiment(self):
        t = read("batteries/us_probe_battery.toml").replace("us_probe_battery", "us_probe_battery_two")
        for parent in ("us_probe_army", "us_probe_division", "us_probe_brigade"):
            self.assertEqual(self.rules("batteries/us_probe_battery_two.toml", t.replace("us_probe_corps", parent)), [], parent)
        self.assertEqual(self.rules("batteries/us_probe_battery_two.toml", t.replace("us_probe_corps", "us_probe_regiment")),
                         [("batteries/us_probe_battery_two.toml", "GB09")])

    def test_time_fact_must_sit_inside_its_early_late(self):
        t = read("arrivals/ar_probe_regiment.toml").replace("ar_probe_regiment", "ar_probe_two").replace('late = "1863-07-01T10:30"',
                                                                                                      'late = "1863-07-01T10:10"')
        self.assertIn(("arrivals/ar_probe_two.toml", "GB07"), self.rules("arrivals/ar_probe_two.toml", t))

    def test_time_claims_must_sit_inside_early_late(self):
        t = read("events/ev_probe_attack.toml").replace("ev_probe_attack", "ev_probe_two").replace('late = "1863-07-02T16:30"',
                                                                                                'late = "1863-07-02T16:15"')
        self.assertIn(("events/ev_probe_two.toml", "GB07"), self.rules("events/ev_probe_two.toml", t))

    def test_non_battle_event_may_fall_after_the_battle_days(self):
        t = read("events/ev_probe_attack.toml").replace("ev_probe_attack", "ev_probe_retreat").replace('kind = "attack_begins"', 'kind = "withdrawal"')
        for a, b in (("1863-07-02T16:00", "1863-07-04T16:00"), ("1863-07-02T16:30", "1863-07-04T16:30")):
            t = t.replace(a, b)
        self.assertEqual(self.rules("events/ev_probe_retreat.toml", t), [])

    def test_weather_date_and_segment_order(self):
        t = read("weather/wx_18630701.toml")
        self.assertEqual(self.rules("weather/wx_18630631.toml", t.replace("wx_18630701", "wx_18630631").replace('"1863-07-01"', '"1863-06-31"')),
                         [("weather/wx_18630631.toml", "GB08")])
        self.assertEqual(self.rules("weather/wx_18630702.toml", t.replace("wx_18630701", "wx_18630702").replace('"1863-07-01"', '"1863-07-02"')
                                    .replace('from = "1863-07-01T06:00"\nto = "1863-07-01T18:00"', 'from = "1863-07-02T18:00"\nto = "1863-07-02T06:00"')),
                         [("weather/wx_18630702.toml", "GB08")])


if __name__ == "__main__":
    unittest.main()
