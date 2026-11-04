#!/usr/bin/env python3
"""Tests for the chain sums, casualty, arms and terrain checks of tools/gb_audit.py (SPEC-013, ticket T-133;
T13-03, T13-04, T13-11). Run: python3 tools/test_gb_chain.py"""
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
SRC = '[{ document = "CWV", page = 72 }]'


def f(value, low=None, high=None, conf="unverified", **kw):
    """An integer fact as a dict."""
    d = {"value": value, "confidence": conf, "sources": [{"document": "CWV", "page": 72}]}
    if low is not None:
        d["low"] = low
    if high is not None:
        d["high"] = high
    d.update(kw)
    return d


def fact_text(table, value, low=None, high=None, extra=""):
    t = '[%s]\nvalue = %d\nconfidence = "unverified"\nsources = %s\n' % (table, value, SRC)
    if low is not None:
        t += "low = %d\nhigh = %d\n" % (low, high)
    return t + extra


def unit(uid, level, parent, engaged=None, present=None, hq=None, body="", ordinal=1):
    """TOML text of a union unit record; regiments get a designation and an engaged figure."""
    t = 'schema = "gb-unit/1"\nid = "%s"\nside = "union"\nlevel = "%s"\nname = "Probe"\n' % (uid, level)
    if level == "regiment":
        t += 'designation = { state = "PA", ordinal = %d, arm = "infantry" }\n' % ordinal
    t += '\n[[assignments]]\nparent = "%s"\nkind = "organic"\nconfidence = "unverified"\nsources = %s\n' % (parent, SRC)
    t += '\n[[commanders]]\nperson = { id = "probe_person_a", name = "Probe Person" }\nkind = "command"\n'
    t += 'confidence = "unverified"\nsources = %s\n' % SRC
    for name, v in (("engaged", engaged), ("present", present), ("headquarters", hq)):
        if v is not None:
            t += "\n" + fact_text("strength." + name, *v if isinstance(v, tuple) else (v,))
    return t + body


def battery(bid, parent, engaged, guns, ordinal_label="Q"):
    t = 'schema = "gb-battery/1"\nid = "%s"\nside = "union"\nname = "Probe"\ndesignation = { state = "NY", label = "%s" }\n' % (bid, ordinal_label)
    t += '\n[[assignments]]\nparent = "%s"\nkind = "organic"\nconfidence = "unverified"\nsources = %s\n' % (parent, SRC)
    t += '\n[[commanders]]\nperson = { id = "probe_person_a", name = "Probe Person" }\nkind = "command"\n'
    t += 'confidence = "unverified"\nsources = %s\n' % SRC
    t += "\n" + fact_text("strength.engaged", engaged)
    for weapon, n in guns:
        t += '\n[[guns]]\nweapon = "%s"\n' % weapon + fact_text("guns.count", n)
    return t


def arm(weapon, n):
    return '\n[[arms]]\nweapon = "%s"\n' % weapon + fact_text("arms.count", n)


def loss_text(scope, **parts):
    """A [[casualties]] table; each part is a value or a (value, extra) pair."""
    t = '\n[[casualties]]\nscope = "%s"\n' % scope
    for k in sorted(parts):
        v, extra = parts[k] if isinstance(parts[k], tuple) else (parts[k], "")
        t += fact_text("casualties.%s" % k, v, extra=extra)
    return t


def terrain(tid, geometry_type, points, extra=""):
    t = 'schema = "gb-terrain/1"\nid = "%s"\nname = "Probe"\nkind = "field"\nposition_tolerance_mm = 5000\n' % tid
    t += 'confidence = "unverified"\nsources = %s\n%s\n[geometry]\ntype = "%s"\nframe = "gb_local_v1"\npoints = %s\n' % (SRC, extra, geometry_type, points)
    return t


def run(*files):
    """check_corpus findings over the good tree plus (relative path, text) files; (rule, severity, path) triples."""
    with tempfile.TemporaryDirectory() as tmp:
        shutil.copytree(os.path.join(FIX, "good"), os.path.join(tmp, "tree"))
        for rel, text in files:
            p = os.path.join(tmp, "tree", rel)
            os.makedirs(os.path.dirname(p), exist_ok=True)
            with open(p, "w", encoding="utf-8") as fh:
                fh.write(text)
        found = gb_audit.check_corpus(gb_audit.load_corpus(os.path.join(tmp, "tree")))
    return [(x.rule, x.severity, x.path) for x in found], found


HOME = ("units/us_t_brig.toml", unit("us_t_brig", "brigade", "us_probe_division"))  # a brigade without a strength figure


def rules(*files):
    return sorted({r for r, _, _ in run(*files)[0]})


class ChainSums(unittest.TestCase):
    """T13-03."""

    def test_interval_sum_passes_inside_the_parent_range(self):
        self.assertIsNone(gb_audit.chain_sum_problem(f(110, 100, 120), [f(45, 40, 50), f(62, 60, 65)]))

    def test_interval_sum_fails_when_the_parent_is_out_of_reach(self):
        self.assertIsNotNone(gb_audit.chain_sum_problem(f(135, 130, 140), [f(45, 40, 50), f(62, 60, 65)]))
        self.assertIsNotNone(gb_audit.chain_sum_problem(f(60, 50, 70), [f(45, 40, 50), f(62, 60, 65)]))

    def test_touching_ranges_pass(self):
        self.assertIsNone(gb_audit.chain_sum_problem(f(110, 100, 100 + 60), [f(60), f(50)]))
        self.assertIsNone(gb_audit.chain_sum_problem(f(120, 120, 130), [f(60, 50, 60), f(60, 60, 70)]))

    def test_headquarters_is_added(self):
        self.assertIsNone(gb_audit.chain_sum_problem(f(110), [f(60), f(40)], f(10)))
        self.assertIsNotNone(gb_audit.chain_sum_problem(f(100), [f(60), f(40)], f(10)))

    def test_all_verified_is_exact(self):
        v = dict(conf="verified")
        self.assertIsNone(gb_audit.chain_sum_problem(f(100, **v), [f(60, **v), f(40, **v)]))
        self.assertIsNotNone(gb_audit.chain_sum_problem(f(101, **v), [f(60, **v), f(40, **v)]))
        self.assertIsNotNone(gb_audit.chain_sum_problem(f(99, **v), [f(60, **v), f(40, **v)]))

    def test_one_unverified_child_keeps_the_interval_test(self):
        v = dict(conf="verified")
        self.assertIsNone(gb_audit.chain_sum_problem(f(101, 100, 102, **v), [f(60, **v), f(40)]))

    def test_corpus_sum_fails_and_passes(self):
        ok = [("units/us_t_brig.toml", unit("us_t_brig", "brigade", "us_probe_division", engaged=(100, 90, 120))),
              ("units/us_t_a.toml", unit("us_t_a", "regiment", "us_t_brig", engaged=(40, 40, 50), ordinal=11)),
              ("units/us_t_b.toml", unit("us_t_b", "regiment", "us_t_brig", engaged=(62, 60, 65), ordinal=12))]
        self.assertEqual(run(*ok)[0], [])
        bad = [("units/us_t_brig.toml", unit("us_t_brig", "brigade", "us_probe_division", engaged=(135, 130, 140)))] + ok[1:]
        triples = run(*bad)[0]
        self.assertEqual(triples, [("GB10", "E", "units/us_t_brig.toml")])

    def test_present_is_summed_apart_from_engaged(self):
        files = [("units/us_t_brig.toml", unit("us_t_brig", "brigade", "us_probe_division", engaged=100, present=500)),
                 ("units/us_t_a.toml", unit("us_t_a", "regiment", "us_t_brig", engaged=100, present=200, ordinal=11))]
        self.assertEqual([(r, p) for r, _, p in run(*files)[0]], [("GB10", "units/us_t_brig.toml")])

    def test_child_without_the_figure_gives_an_info_finding_and_no_sum(self):
        files = [("units/us_t_brig.toml", unit("us_t_brig", "brigade", "us_probe_division", engaged=100, present=10)),
                 ("units/us_t_a.toml", unit("us_t_a", "regiment", "us_t_brig", engaged=100, ordinal=11))]
        triples, found = run(*files)
        self.assertEqual(triples, [("GB10", "I", "units/us_t_brig.toml")])
        self.assertIn("present", found[0].message)

    def test_battery_strength_joins_the_parent_sum(self):
        files = [("units/us_t_div.toml", unit("us_t_div", "division", "us_probe_corps", engaged=100)),
                 ("batteries/us_t_bat.toml", battery("us_t_bat", "us_t_div", 150, [("napoleon_12", 4)]))]
        self.assertEqual([(r, p) for r, _, p in run(*files)[0]], [("GB10", "units/us_t_div.toml")])

    def test_parent_of_record_is_the_first_organic_assignment(self):
        extra = '\n[[assignments]]\nparent = "us_t_other"\nkind = "attached"\nconfidence = "unverified"\nsources = %s\n' % SRC
        files = [("units/us_t_brig.toml", unit("us_t_brig", "brigade", "us_probe_division", engaged=100)),
                 ("units/us_t_other.toml", unit("us_t_other", "brigade", "us_probe_division", engaged=10)),
                 ("units/us_t_a.toml", unit("us_t_a", "regiment", "us_t_brig", engaged=100, ordinal=11, body=extra))]
        self.assertEqual(run(*files)[0], [])

    def test_guns_sum_against_parent_arms(self):
        grp = unit("us_t_group", "brigade", "us_probe_division", body=arm("napoleon_12", 10))
        ok = [("units/us_t_group.toml", grp),
              ("batteries/us_t_b1.toml", battery("us_t_b1", "us_t_group", 50, [("napoleon_12", 6)], "A")),
              ("batteries/us_t_b2.toml", battery("us_t_b2", "us_t_group", 50, [("napoleon_12", 4)], "B"))]
        self.assertEqual(run(*ok)[0], [])
        bad = ok[:2] + [("batteries/us_t_b2.toml", battery("us_t_b2", "us_t_group", 50, [("napoleon_12", 5)], "B"))]
        self.assertEqual([(r, p) for r, _, p in run(*bad)[0]], [("GB10", "units/us_t_group.toml")])


class Casualties(unittest.TestCase):
    """T13-04."""

    def test_pct_rounds_half_up(self):
        self.assertEqual((gb_audit.pct(1153, 1885), gb_audit.pct(397, 496), gb_audit.pct(215, 262)), (61, 80, 82))
        self.assertEqual((gb_audit.pct(1, 2), gb_audit.pct(0, 9), gb_audit.pct(9, 9)), (50, 0, 100))

    def test_recorded_rate_must_match_within_one(self):
        self.assertIsNone(gb_audit.rate_problem(1153, 1885, 61))
        self.assertIsNone(gb_audit.rate_problem(1153, 1885, 62))
        self.assertIsNone(gb_audit.rate_problem(1153, 1885, 60))
        self.assertIsNotNone(gb_audit.rate_problem(1153, 1885, 63))
        self.assertIsNotNone(gb_audit.rate_problem(1153, 1885, 59))
        self.assertIsNotNone(gb_audit.rate_problem(5, 0, 0))

    def row(self, scope="battle", **kw):
        return dict({"scope": scope}, **{k: f(v) if isinstance(v, int) else v for k, v in kw.items()})

    def test_components_against_total(self):
        ok = self.row(killed=10, wounded=30, missing_captured=10, total=50)
        self.assertEqual(gb_audit.casualty_problems([ok], f(500)), [])
        bad = self.row(killed=10, wounded=30, missing_captured=10, total=51)
        self.assertEqual(len(gb_audit.casualty_problems([bad], f(500))), 1)

    def test_components_above_total_fail_and_partial_components_may_fall_short(self):
        self.assertEqual(len(gb_audit.casualty_problems([self.row(killed=30, wounded=30, total=50)], f(500))), 1)
        self.assertEqual(gb_audit.casualty_problems([self.row(killed=10, wounded=30, total=50)], f(500)), [])

    def test_exact_when_all_verified(self):
        v = dict(conf="verified")
        row = {"scope": "battle", "killed": f(10, **v), "wounded": f(30, **v), "missing_captured": f(10, **v), "total": f(51, **v)}
        self.assertEqual(len(gb_audit.casualty_problems([row], None)), 1)

    def test_range_in_total_lets_components_fit(self):
        row = {"scope": "battle", "killed": f(10), "wounded": f(30), "missing_captured": f(10), "total": f(51, 50, 55)}
        self.assertEqual(gb_audit.casualty_problems([row], None), [])

    def test_day_scopes_against_battle_scope(self):
        days = [self.row("day1", total=20), self.row("day2", total=30), self.row("day3", total=10)]
        self.assertEqual(gb_audit.casualty_problems([self.row("battle", total=60)] + days, f(500)), [])
        self.assertEqual(len(gb_audit.casualty_problems([self.row("battle", total=61)] + days, f(500))), 1)

    def test_fewer_days_may_fall_short_but_not_exceed(self):
        days = [self.row("day1", total=20), self.row("day2", total=30)]
        self.assertEqual(gb_audit.casualty_problems([self.row("battle", total=60)] + days, f(500)), [])
        self.assertEqual(len(gb_audit.casualty_problems([self.row("battle", total=40)] + days, f(500))), 1)

    def test_day_scope_without_a_battle_row_is_not_judged(self):
        self.assertEqual(gb_audit.casualty_problems([self.row("day1", total=20)], f(500)), [])

    def test_losses_above_engaged_high_fail(self):
        self.assertEqual(gb_audit.casualty_problems([self.row(total=300)], f(250, 240, 300)), [])
        self.assertEqual(len(gb_audit.casualty_problems([self.row(total=301)], f(250, 240, 300))), 1)
        self.assertEqual(len(gb_audit.casualty_problems([self.row(wounded=301)], f(250, 240, 300))), 1)
        self.assertEqual(gb_audit.casualty_problems([self.row(total=301)], None), [])

    def test_c03_sum_conflict_is_reported_not_hidden(self):
        claims = [{"document": "CWV", "page": 84, "figure": 6555}, {"document": "CWV", "page": 84, "figure": 8892}]
        parts = dict(killed=1123, wounded=4019, missing_captured=3750)
        flagged = self.row("day3", total=f(6555, flag="sum conflict", claims=claims), **parts)
        self.assertEqual(gb_audit.casualty_problems([flagged], f(12500)), [])
        bare = self.row("day3", total=f(6555, flag="sum conflict"), **parts)
        self.assertEqual(len(gb_audit.casualty_problems([bare], f(12500))), 1)
        unflagged = self.row("day3", total=f(6555, claims=claims), **parts)
        self.assertEqual(len(gb_audit.casualty_problems([unflagged], f(12500))), 1)
        half = self.row("day3", total=f(6555, flag="x", claims=claims[:1]), **parts)
        self.assertEqual(len(gb_audit.casualty_problems([half], f(12500))), 1)

    def test_corpus_findings(self):
        def code(body, eng=500):
            return run(HOME, ("units/us_t_a.toml", unit("us_t_a", "regiment", "us_t_brig", engaged=eng, body=body, ordinal=11)))[0]
        self.assertEqual(code(loss_text("battle", killed=10, wounded=20, total=30)), [])
        self.assertEqual([r for r, _, _ in code(loss_text("battle", killed=10, wounded=20, total=31, missing_captured=0))], ["GB11"])
        self.assertEqual([r for r, _, _ in code(loss_text("battle", total=30) + loss_text("day1", total=40))], ["GB11"])
        self.assertEqual([r for r, _, _ in code(loss_text("battle", total=30), eng=5)], ["GB11"])


class Arms(unittest.TestCase):
    """T13-11 (weapons) and GB12."""

    def regiment(self, body, eng=100):
        return run(HOME, ("units/us_t_a.toml", unit("us_t_a", "regiment", "us_t_brig", engaged=eng, body=body, ordinal=11)))[0]

    def test_known_weapons_pass(self):
        self.assertEqual(self.regiment(arm("rifle_58", 80) + arm("revolver_44", 10)), [])

    def test_unknown_weapon_is_gb12(self):
        self.assertEqual([(r, s) for r, s, _ in self.regiment(arm("blunderbuss_99", 80))], [("GB12", "E")])

    def test_arms_above_engaged_high_are_gb12(self):
        self.assertEqual([r for r, _, _ in self.regiment(arm("rifle_58", 60) + arm("smoothbore_69", 41))], ["GB12"])
        self.assertEqual(self.regiment(arm("rifle_58", 60) + arm("smoothbore_69", 40)), [])

    def test_battery_guns(self):
        def run_b(weapon, n, eng=100):
            return run(("batteries/us_t_bat.toml", battery("us_t_bat", "us_probe_division", eng, [(weapon, n)])))[0]
        self.assertEqual(run_b("napoleon_12", 6), [])
        self.assertEqual([r for r, _, _ in run_b("mortar_13", 6)], ["GB12"])
        self.assertEqual([r for r, _, _ in run_b("napoleon_12", 8, eng=7)], ["GB12"])

    def test_weapon_tables_are_read_from_the_corpus_root_when_present(self):
        with tempfile.TemporaryDirectory() as tmp:
            os.makedirs(os.path.join(tmp, "data", "rules"))
            with open(os.path.join(tmp, "data", "rules", "weapons.toml"), "w", encoding="utf-8") as fh:
                fh.write('schema = "weapons/1"\n[[weapon]]\nid = "only_one"\n')
            os.makedirs(os.path.join(tmp, "data", "history", "gettysburg", "units"))
            with open(os.path.join(tmp, "data", "history", "gettysburg", "units", "us_t_army.toml"), "w", encoding="utf-8") as fh:
                fh.write('schema = "gb-unit/1"\nid = "us_t_army"\nside = "union"\nlevel = "army"\nname = "P"\n'
                         '\n[[commanders]]\nperson = { id = "probe_person_a", name = "P" }\nkind = "command"\nconfidence = "unverified"\n'
                         'sources = %s\n' % SRC + arm("rifle_58", 1))
            found = gb_audit.check_corpus(gb_audit.load_corpus(tmp))
        self.assertIn("GB12", [x.rule for x in found])


class Terrain(unittest.TestCase):
    """T13-11 (terrain classes) and GB18."""

    def t(self, tid, gtype, points, extra=""):
        return run(("terrain/%s.toml" % tid, terrain(tid, gtype, points, extra)))[0]

    def test_known_class_and_good_polygon_pass(self):
        sq = "[[0, 0], [1000, 0], [1000, 1000]]"
        self.assertEqual(self.t("t_field", "polygon", sq, 'terrain_class = "woods"'), [])

    def test_unknown_class_is_gb18(self):
        self.assertEqual([(r, s) for r, s, _ in self.t("t_field", "polygon", "[[0, 0], [1, 0], [1, 1]]", 'terrain_class = "quicksand"')],
                         [("GB18", "E")])

    def test_polygon_needs_three_points(self):
        self.assertEqual([r for r, _, _ in self.t("t_field", "polygon", "[[0, 0], [1000, 0]]")], ["GB18"])
        self.assertEqual(self.t("t_line", "polyline", "[[0, 0], [1000, 0]]"), [])

    def test_coordinates_outside_the_frame_are_gb18(self):
        lo, hi = gb_audit.FRAME_MIN_MM, gb_audit.FRAME_MAX_MM
        self.assertEqual(self.t("t_edge", "polyline", "[[%d, %d], [%d, %d]]" % (lo, lo, hi, hi)), [])
        self.assertEqual([r for r, _, _ in self.t("t_west", "polyline", "[[%d, 0], [5, 5]]" % (lo - 1))], ["GB18"])
        self.assertEqual([r for r, _, _ in self.t("t_north", "polyline", "[[0, 0], [5, %d]]" % (hi + 1))], ["GB18"])

    def test_arrival_place_outside_the_frame_is_gb18(self):
        with open(os.path.join(FIX, "good", "arrivals", "ar_probe_regiment.toml"), encoding="utf-8") as fh:
            text = fh.read().replace("ar_probe_regiment", "ar_t_far")
        def at(x):
            return text.replace("[place]", '[place]\nframe = "gb_local_v1"\nx_mm = %d\ny_mm = 0' % x)
        self.assertEqual(rules(("arrivals/ar_t_far.toml", text)), [])
        self.assertEqual(rules(("arrivals/ar_t_far.toml", at(gb_audit.FRAME_MAX_MM))), [])
        self.assertEqual(rules(("arrivals/ar_t_far.toml", at(gb_audit.FRAME_MAX_MM + 1))), ["GB18"])


class Probes(unittest.TestCase):
    """The bad probes for the new rules exist, fail with their code and leave the good tree otherwise clean."""

    def codes(self):
        out = {}
        for dp, _, names in sorted(os.walk(os.path.join(FIX, "bad"))):
            for n in sorted(names):
                with open(os.path.join(dp, n), encoding="utf-8") as fh:
                    out.setdefault(fh.readline().strip()[len("# expect: "):], []).append(n)
        return out

    def test_each_new_rule_has_probes(self):
        codes = self.codes()
        for rule, least in (("GB10", 2), ("GB11", 3), ("GB12", 3), ("GB18", 3)):
            self.assertGreaterEqual(len(codes.get(rule, [])), least, rule)

    def test_good_tree_carries_a_chain_and_a_gun_sum(self):
        c = gb_audit.load_corpus(os.path.join(FIX, "good"))
        self.assertEqual(gb_audit.check_corpus(c), [])
        docs = {r.doc["id"]: r.doc for r in c.records if r.kind in ("units", "batteries")}
        self.assertIn("engaged", docs["us_probe_sum_brigade"]["strength"])
        self.assertTrue(docs["us_probe_sum_group"].get("arms"))
        self.assertEqual(sorted(d for d in docs if d.startswith("us_probe_sum_bat")), ["us_probe_sum_bat_a", "us_probe_sum_bat_b"])


if __name__ == "__main__":
    unittest.main()
