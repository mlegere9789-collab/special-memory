#!/usr/bin/env python3
"""Tests for the commander, time-ordering, designation, weather and hourly-schedule checks of tools/gb_audit.py
(SPEC-013, ticket T-134; T13-05 and the schedule half of T13-10). Run: python3 tools/test_gb_time.py"""
import io
import contextlib
import os
import shutil
import subprocess
import sys
import tempfile
import unittest

TOOLS = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(TOOLS)
sys.path.insert(0, TOOLS)

import gb_audit  # noqa: E402

FIX = os.path.join(REPO, "qc", "fixtures", "gb")
SRC = '[{ document = "CWV", page = 72 }]'
TAIL = 'confidence = "unverified"\nsources = %s\n' % SRC
D = "1863-07-0"


def stint(person, kind="command", frm=None, to=None, note=None):
    t = '\n[[commanders]]\nperson = { id = "%s", name = "Probe Person" }\nkind = "%s"\n' % (person, kind) + TAIL
    for k, v in (("from", frm), ("to", to), ("note", note)):
        if v is not None:
            t += '%s = "%s"\n' % (k, v)
    return t


def unit(uid, level, parent, stints, side="union", state="PA", ordinal=1, arm="infantry"):
    t = 'schema = "gb-unit/1"\nid = "%s"\nside = "%s"\nlevel = "%s"\nname = "Probe"\n' % (uid, side, level)
    if level == "regiment":
        t += 'designation = { state = "%s", ordinal = %d, arm = "%s" }\n' % (state, ordinal, arm)
    if parent:
        t += '\n[[assignments]]\nparent = "%s"\nkind = "organic"\n%s' % (parent, TAIL)
    t += "".join(stints)
    if level == "regiment":
        t += '\n[strength.engaged]\nvalue = 100\n' + TAIL
    return t


def event(eid, at, kind="other", person=None, outcome=None, remained=False, not_before=()):
    t = 'schema = "gb-event/1"\nid = "%s"\nkind = "%s"\nrole = "validation"\nsummary = "A probe event."\n' % (eid, kind)
    t += "".join('\n[[not_before]]\nevent = "%s"\ngap_min = %d\n' % nb for nb in not_before)
    if person:
        t += '\n[person_outcome]\nperson = "%s"\noutcome = "%s"\n' % (person, outcome)
        t += "remained_in_command = true\n" if remained else ""
    return t + '\n[at]\nat = "%s"\n%s' % (at, TAIL)


def arrival(aid, formation, kind, at):
    return ('schema = "gb-arrival/1"\nid = "%s"\nformation = "%s"\nevent = "%s"\n\n[at]\nat = "%s"\n%s\n'
            '[place]\nname = "Probe Ridge"\n%s') % (aid, formation, kind, at, TAIL, TAIL)


def weather(date, *segments):
    t = 'schema = "gb-weather/1"\nid = "wx_%s"\ndate = "%s-%s-%s"\n' % (date, date[:4], date[4:6], date[6:])
    for a, b in segments:
        t += '\n[[segment]]\nfrom = "%s"\nto = "%s"\nsky = "clear"\n%s' % (a, b, TAIL)
    return t


def run(*files):
    """check_corpus findings over the good tree plus (relative path, text) files: (triples, findings)."""
    with tempfile.TemporaryDirectory() as tmp:
        shutil.copytree(os.path.join(FIX, "good"), os.path.join(tmp, "tree"))
        for rel, text in files:
            p = os.path.join(tmp, "tree", rel)
            os.makedirs(os.path.dirname(p), exist_ok=True)
            with open(p, "w", encoding="utf-8") as fh:
                fh.write(text)
        found = gb_audit.check_corpus(gb_audit.load_corpus(os.path.join(tmp, "tree")))
    return [(x.rule, x.severity, x.path) for x in found], found


def u(name, stints, level="brigade", parent="us_probe_division", **kw):
    return ("units/%s.toml" % name, unit(name, level, parent, stints, **kw))


def ev(name, *a, **kw):
    return ("events/%s.toml" % name, event(name, *a, **kw))


def ar(name, *a):
    return ("arrivals/%s.toml" % name, arrival(name, *a))


class Stints(unittest.TestCase):
    """T13-05: overlap, shared endpoints, casualties, sides, temporary_higher."""

    def test_overlapping_stints_of_one_person_fail(self):
        t = run(u("us_t_a", [stint("p_x", frm=D + "1T06:00", to=D + "1T12:00")]),
                u("us_t_b", [stint("p_x", frm=D + "1T10:00", to=D + "1T18:00")], parent="us_probe_division"))[0]
        self.assertEqual(t, [("GB13", "E", "units/us_t_b.toml")])

    def test_shared_endpoint_passes(self):
        self.assertEqual(run(u("us_t_a", [stint("p_x", frm=D + "1T06:00", to=D + "1T12:00")]),
                             u("us_t_b", [stint("p_x", frm=D + "1T12:00", to=D + "1T18:00")]))[0], [])

    def test_one_minute_of_overlap_fails(self):
        t = run(u("us_t_a", [stint("p_x", frm=D + "1T06:00", to=D + "1T12:01")]),
                u("us_t_b", [stint("p_x", frm=D + "1T12:00", to=D + "1T18:00")]))[0]
        self.assertEqual([r for r, _, _ in t], ["GB13"])

    def test_acting_and_command_overlap_fail(self):
        t = run(u("us_t_a", [stint("p_x", frm=D + "1T06:00", to=D + "1T12:00")]),
                u("us_t_b", [stint("p_x", "acting", frm=D + "1T11:00", to=D + "1T18:00")]))[0]
        self.assertEqual([(r, s) for r, s, _ in t], [("GB13", "E")])

    def test_open_ended_stint_overlaps_a_later_one(self):
        t = run(u("us_t_a", [stint("p_x", frm=D + "1T06:00")]),
                u("us_t_b", [stint("p_x", frm=D + "2T06:00", to=D + "2T09:00")]))[0]
        self.assertEqual([r for r, _, _ in t], ["GB13"])

    def test_undated_stints_are_not_compared(self):
        self.assertEqual(run(u("us_t_a", [stint("p_x")]), u("us_t_b", [stint("p_x")]))[0], [])

    def test_two_command_stints_of_a_formation_overlap(self):
        t = run(u("us_t_a", [stint("p_x", frm=D + "1T06:00", to=D + "1T12:00"),
                             stint("p_y", frm=D + "1T11:00", to=D + "1T18:00")]))[0]
        self.assertEqual(t, [("GB13", "E", "units/us_t_a.toml")])

    def test_successive_command_stints_of_a_formation_pass(self):
        self.assertEqual(run(u("us_t_a", [stint("p_x", frm=D + "1T06:00", to=D + "1T12:00"),
                                          stint("p_y", frm=D + "1T12:00", to=D + "1T18:00")]))[0], [])

    def test_acting_beside_command_in_one_formation_is_not_a_formation_overlap(self):
        self.assertEqual(run(u("us_t_a", [stint("p_x", frm=D + "1T06:00", to=D + "1T12:00"),
                                          stint("p_y", "acting", frm=D + "1T11:00", to=D + "1T18:00")]))[0], [])

    def test_battery_stints_count(self):
        bat = ('schema = "gb-battery/1"\nid = "us_t_bat"\nside = "union"\nname = "Probe"\n'
               'designation = { state = "NY", label = "Q" }\n\n[[assignments]]\nparent = "us_probe_division"\n'
               'kind = "organic"\n%s%s\n[strength.engaged]\nvalue = 80\n%s\n[[guns]]\nweapon = "napoleon_12"\n'
               '[guns.count]\nvalue = 4\n%s'
               % (TAIL, stint("p_x", frm=D + "1T06:00", to=D + "1T12:00"), TAIL, TAIL))
        t = run(u("us_t_a", [stint("p_x", frm=D + "1T11:00", to=D + "1T18:00")]), ("batteries/us_t_bat.toml", bat))[0]
        self.assertEqual([r for r, _, _ in t], ["GB13"])

    def test_killed_commander_with_a_later_stint_fails(self):
        files = [ev("ev_t_kill", D + "1T11:00", "commander_casualty", "p_x", "killed"),
                 u("us_t_a", [stint("p_x", frm=D + "1T12:00", to=D + "1T18:00")])]
        self.assertEqual(run(*files)[0], [("GB13", "E", "units/us_t_a.toml")])

    def test_stint_running_past_the_death_fails_and_one_ending_at_it_passes(self):
        kill = ev("ev_t_kill", D + "1T11:00", "commander_casualty", "p_x", "killed")
        self.assertEqual(run(kill, u("us_t_a", [stint("p_x", frm=D + "1T06:00", to=D + "1T11:00")]))[0], [])
        t = run(kill, u("us_t_a", [stint("p_x", frm=D + "1T06:00", to=D + "1T11:01")]))[0]
        self.assertEqual(t, [("GB13", "E", "units/us_t_a.toml")])

    def test_mortally_wounded_and_captured_end_command(self):
        for outcome in ("mortally_wounded", "captured"):
            files = [ev("ev_t_out", D + "1T11:00", "commander_casualty", "p_x", outcome),
                     u("us_t_a", [stint("p_x", frm=D + "2T06:00", to=D + "2T09:00")])]
            self.assertEqual(run(*files)[0], [("GB13", "E", "units/us_t_a.toml")], outcome)

    def test_wounded_commander_who_remained_gives_a_warning(self):
        files = [ev("ev_t_w", D + "1T11:00", "commander_casualty", "p_x", "wounded", remained=True),
                 u("us_t_a", [stint("p_x", frm=D + "1T12:00", to=D + "1T18:00")])]
        self.assertEqual(run(*files)[0], [("GB13", "W", "units/us_t_a.toml")])

    def test_mortally_wounded_who_remained_gives_a_warning(self):
        files = [ev("ev_t_w", D + "1T11:00", "commander_casualty", "p_x", "mortally_wounded", remained=True),
                 u("us_t_a", [stint("p_x", frm=D + "1T12:00", to=D + "1T18:00")])]
        self.assertEqual(run(*files)[0], [("GB13", "W", "units/us_t_a.toml")])

    def test_wounded_commander_not_flagged_may_return(self):
        files = [ev("ev_t_w", D + "1T11:00", "commander_casualty", "p_x", "wounded"),
                 u("us_t_a", [stint("p_x", frm=D + "2T12:00", to=D + "2T18:00")])]
        self.assertEqual(run(*files)[0], [])

    def test_casualty_of_another_person_does_not_matter(self):
        files = [ev("ev_t_kill", D + "1T11:00", "commander_casualty", "p_z", "killed"),
                 u("us_t_a", [stint("p_x", frm=D + "1T12:00", to=D + "1T18:00")])]
        self.assertEqual(run(*files)[0], [])

    def test_person_on_both_sides_fails_on_the_minority_side(self):
        files = [u("us_t_a", [stint("p_x")]), u("us_t_b", [stint("p_x")]),
                 u("cs_t_a", [stint("p_x")], level="army", parent=None, side="confederate")]
        self.assertEqual(run(*files)[0], [("GB13", "E", "units/cs_t_a.toml")])

    def test_temporary_higher_over_a_command_stint_needs_a_note(self):
        base = u("us_t_a", [stint("p_x", frm=D + "1T06:00", to=D + "1T18:00")])
        higher = lambda **kw: u("us_t_corps", [stint("p_x", "temporary_higher", frm=D + "1T10:00", to=D + "1T14:00", **kw)],
                                level="corps", parent="us_probe_army")
        self.assertEqual(run(base, higher())[0], [("GB13", "W", "units/us_t_corps.toml")])
        self.assertEqual(run(base, higher(note="Took the corps while its commander was down."))[0], [])

    def test_temporary_higher_apart_from_command_is_clean(self):
        files = [u("us_t_a", [stint("p_x", frm=D + "1T06:00", to=D + "1T10:00")]),
                 u("us_t_corps", [stint("p_x", "temporary_higher", frm=D + "1T10:00", to=D + "1T14:00")],
                   level="corps", parent="us_probe_army")]
        self.assertEqual(run(*files)[0], [])

    def test_meade_boundary_stamp(self):
        """C13: the V Corps stint ends at or before the stamp the army stint begins; one minute later fails."""
        def go(corps_to):
            return run(u("us_t_vcorps", [stint("meade", frm="1863-06-28T00:00", to=corps_to)], level="corps",
                         parent="us_probe_army"),
                       u("us_t_aop", [stint("meade", frm="1863-06-28T03:00")], level="army", parent=None))[0]
        self.assertEqual(go("1863-06-28T03:00"), [])
        self.assertEqual(go("1863-06-28T02:59"), [])
        self.assertEqual([(r, s) for r, s, _ in go("1863-06-28T03:01")], [("GB13", "E")])


class Ordering(unittest.TestCase):
    """GB14: arrival and departure, not_before, parent before child."""

    def test_arrival_after_departure_fails(self):
        files = [ar("ar_t_dep", "us_t_x", "depart", D + "1T18:00"), ar("ar_t_arr", "us_t_x", "arrive", D + "1T19:00")]
        self.assertEqual(run(*files)[0], [("GB14", "E", "arrivals/ar_t_arr.toml")])

    def test_arrival_at_the_departure_stamp_and_before_pass(self):
        for arrive in (D + "1T18:00", D + "1T09:00"):
            files = [ar("ar_t_dep", "us_t_x", "depart", D + "1T18:00"), ar("ar_t_arr", "us_t_x", "arrive", arrive)]
            self.assertEqual(run(*files)[0], [], arrive)

    def test_other_formations_are_independent(self):
        files = [ar("ar_t_dep", "us_t_x", "depart", D + "1T18:00"), ar("ar_t_arr", "us_t_y", "arrive", D + "1T19:00")]
        self.assertEqual(run(*files)[0], [])

    def test_not_before_gap(self):
        ref = ev("ev_t_ref", D + "2T14:45")
        self.assertEqual(run(ref, ev("ev_t_late", D + "2T15:15", not_before=[("ev_t_ref", 30)]))[0], [])
        t = run(ref, ev("ev_t_early", D + "2T15:14", not_before=[("ev_t_ref", 30)]))[0]
        self.assertEqual(t, [("GB14", "E", "events/ev_t_early.toml")])

    def test_not_before_naming_an_absent_event_is_information(self):
        t = run(ev("ev_t_lost", D + "2T15:00", not_before=[("ev_t_nowhere", 0)]))[0]
        self.assertEqual(t, [("GB14", "I", "events/ev_t_lost.toml")])

    def test_child_arriving_over_an_hour_before_its_parent_warns(self):
        files = [ar("ar_t_div", "us_probe_division", "arrive", D + "1T10:00"),
                 ar("ar_t_brig", "us_probe_brigade", "arrive", D + "1T08:59")]
        self.assertEqual(run(*files)[0], [("GB14", "W", "arrivals/ar_t_brig.toml")])

    def test_exactly_an_hour_before_passes(self):
        files = [ar("ar_t_div", "us_probe_division", "arrive", D + "1T10:00"),
                 ar("ar_t_brig", "us_probe_brigade", "arrive", D + "1T09:00")]
        self.assertEqual(run(*files)[0], [])

    def test_child_after_parent_passes(self):
        files = [ar("ar_t_div", "us_probe_division", "arrive", D + "1T10:00"),
                 ar("ar_t_brig", "us_probe_brigade", "arrive", D + "1T10:30")]
        self.assertEqual(run(*files)[0], [])


class Designations(unittest.TestCase):
    """GB15."""

    def regs(self, *specs, brigade_side="union"):
        side = brigade_side
        pre = "cs" if side == "confederate" else "us"
        chain = (("army", None), ("corps", "army"), ("division", "corps"), ("brigade", "div"))
        files = []
        for level, parent in chain:
            n = "%s_t_%s" % (pre, "div" if level == "division" else "brig" if level == "brigade" else level)
            up = None if parent is None else "%s_t_%s" % (pre, parent)
            files.append(("units/%s.toml" % n, unit(n, level, up, [stint("p_%s_%s" % (pre, level))], side=side)))
        for i, (state, ordinal, arm) in enumerate(specs):
            n = "%s_t_r%d" % (pre, i)
            files.append(("units/%s.toml" % n, unit(n, "regiment", "%s_t_brig" % pre, [stint("p_%s_r%d" % (pre, i))], side=side,
                                                    state=state, ordinal=ordinal, arm=arm)))
        return files

    def test_duplicate_designation_fails_on_the_later_record(self):
        t = run(*self.regs(("OH", 5, "infantry"), ("OH", 5, "infantry")))[0]
        self.assertEqual(t, [("GB15", "E", "units/us_t_r1.toml")])

    def test_arm_state_or_ordinal_makes_it_distinct(self):
        for other in (("OH", 5, "cavalry"), ("NY", 5, "infantry"), ("OH", 6, "infantry")):
            self.assertEqual(run(*self.regs(("OH", 5, "infantry"), other))[0], [], other)

    def test_same_designation_on_the_two_sides_is_distinct(self):
        files = self.regs(("OH", 5, "infantry")) + self.regs(("OH", 5, "infantry"), brigade_side="confederate")
        self.assertEqual(run(*files)[0], [])

    def test_confederate_brigade_of_two_states_warns(self):
        t = run(*self.regs(("VA", 1, "infantry"), ("NC", 2, "infantry"), brigade_side="confederate"))[0]
        self.assertEqual(t, [("GB15", "W", "units/cs_t_brig.toml")])

    def test_confederate_brigade_of_one_state_and_union_brigade_of_two_pass(self):
        self.assertEqual(run(*self.regs(("VA", 1, "infantry"), ("VA", 2, "infantry"), brigade_side="confederate"))[0], [])
        self.assertEqual(run(*self.regs(("OH", 1, "infantry"), ("NY", 2, "infantry")))[0], [])


class Weather(unittest.TestCase):
    """GB16."""

    def test_good_segments_pass(self):
        w = weather("18630702", (D + "2T00:00", D + "2T12:00"), (D + "2T12:00", D + "3T06:00"))
        self.assertEqual(run(("weather/wx_18630702.toml", w))[0], [])

    def test_overlap_fails(self):
        w = weather("18630702", (D + "2T00:00", D + "2T12:01"), (D + "2T12:00", D + "2T18:00"))
        self.assertEqual(run(("weather/wx_18630702.toml", w))[0], [("GB16", "E", "weather/wx_18630702.toml")])

    def test_unordered_fails(self):
        w = weather("18630702", (D + "2T12:00", D + "2T18:00"), (D + "2T00:00", D + "2T06:00"))
        self.assertEqual(run(("weather/wx_18630702.toml", w))[0], [("GB16", "E", "weather/wx_18630702.toml")])

    def test_outside_the_day_and_following_night_fails(self):
        for a, b in ((D + "1T23:00", D + "2T06:00"), (D + "2T00:00", D + "3T06:01")):
            w = weather("18630702", (a, b))
            self.assertEqual(run(("weather/wx_18630702.toml", w))[0], [("GB16", "E", "weather/wx_18630702.toml")], (a, b))

    def test_end_of_the_following_night_is_allowed(self):
        w = weather("18630702", (D + "2T00:00", D + "3T06:00"))
        self.assertEqual(run(("weather/wx_18630702.toml", w))[0], [])


class Schedule(unittest.TestCase):
    """T13-10, schedule half."""

    def corpus(self, *files):
        tmp = tempfile.TemporaryDirectory()
        os.makedirs(os.path.join(tmp.name, "arrivals"))
        for rel, text in files:
            with open(os.path.join(tmp.name, rel), "w", encoding="utf-8") as fh:
                fh.write(text)
        self.addCleanup(tmp.cleanup)
        return gb_audit.load_corpus(tmp.name)

    def test_equal_hours_order_by_formation_then_event_rank(self):
        c = self.corpus(ar("ar_t_1", "us_b", "arrive", D + "2T10:50"), ar("ar_t_2", "us_a", "depart", D + "2T10:05"),
                        ar("ar_t_3", "us_a", "arrive", D + "2T10:59"))
        self.assertEqual(gb_audit.hourly_schedule(c), [(34, "us_a", "arrive", "ar_t_3"), (34, "us_a", "depart", "ar_t_2"),
                                                        (34, "us_b", "arrive", "ar_t_1")])

    def test_event_rank_is_arrive_deploy_withdraw_depart_then_id(self):
        c = self.corpus(*[ar("ar_t_%s" % k, "us_a", k, D + "1T09:00") for k in ("depart", "withdraw", "deploy", "arrive")],
                        ar("ar_t_arrive2", "us_a", "arrive", D + "1T09:30"))
        self.assertEqual([(e, i) for _, _, e, i in gb_audit.hourly_schedule(c)],
                         [("arrive", "ar_t_arrive"), ("arrive", "ar_t_arrive2"), ("deploy", "ar_t_deploy"),
                          ("withdraw", "ar_t_withdraw"), ("depart", "ar_t_depart")])

    def test_hour_decides_before_formation(self):
        c = self.corpus(ar("ar_t_1", "us_a", "arrive", D + "1T11:00"), ar("ar_t_2", "us_z", "arrive", D + "1T10:59"))
        self.assertEqual([i for _, _, _, i in gb_audit.hourly_schedule(c)], ["ar_t_2", "ar_t_1"])

    def test_negative_hours_before_the_battle(self):
        c = self.corpus(ar("ar_t_1", "us_a", "arrive", "1863-06-30T23:30"))
        self.assertEqual(gb_audit.hourly_schedule(c), [(-1, "us_a", "arrive", "ar_t_1")])

    def test_invalid_records_are_left_out(self):
        c = self.corpus(ar("ar_t_1", "us_a", "arrive", "1863-06-31T10:00"), ("arrivals/ar_t_2.toml", "id = 3\n"))
        self.assertEqual(gb_audit.hourly_schedule(c), [])

    def test_good_tree_schedule(self):
        c = gb_audit.load_corpus(os.path.join(FIX, "good"))
        sched = gb_audit.hourly_schedule(c)
        self.assertEqual(sched, sorted(sched, key=lambda r: (r[0], r[1], gb_audit.EVENT_RANK[r[2]], r[3])))
        self.assertIn((10, "us_probe_regiment", "arrive", "ar_probe_regiment"), sched)

    def run_cli(self, *args, hashseed="0"):
        env = dict(os.environ, PYTHONHASHSEED=hashseed)
        return subprocess.run([sys.executable, os.path.join(TOOLS, "gb_audit.py")] + list(args), capture_output=True,
                              text=True, env=env)

    def test_hourly_command_prints_the_schedule_and_is_repeatable(self):
        r0 = self.run_cli("hourly", os.path.join(FIX, "good"))
        r1 = self.run_cli("hourly", os.path.join(FIX, "good"), hashseed="1")
        self.assertEqual(r0.returncode, 0, r0.stderr)
        self.assertEqual(r0.stdout, r1.stdout)
        lines = r0.stdout.splitlines()
        self.assertTrue(lines)
        self.assertIn("1863-07-01T10:00\tus_probe_regiment\tarrive\tar_probe_regiment", lines)

    def test_hourly_command_on_the_repository_runs(self):
        self.assertEqual(self.run_cli("hourly").returncode, 0)

    def test_check_command_reports_the_new_rules(self):
        with tempfile.TemporaryDirectory() as tmp:
            shutil.copytree(os.path.join(FIX, "good"), os.path.join(tmp, "tree"))
            with open(os.path.join(tmp, "tree", "weather", "wx_18630702.toml"), "w", encoding="utf-8") as fh:
                fh.write(weather("18630702", (D + "2T00:00", D + "2T12:01"), (D + "2T12:00", D + "2T18:00")))
            r = self.run_cli("check", os.path.join(tmp, "tree"))
        self.assertEqual(r.returncode, 1)
        self.assertIn("weather/wx_18630702.toml: GB16 E", r.stdout)


class Probes(unittest.TestCase):
    """The bad probes of this ticket fail with their own code and nothing else; the good tree stays clean."""

    CODES = {"GB13": 6, "GB14": 3, "GB15": 1, "GB16": 3}

    def test_good_tree_is_clean(self):
        self.assertEqual(gb_audit.check_corpus(gb_audit.load_corpus(os.path.join(FIX, "good"))), [])

    def test_probe_counts(self):
        found = {}
        for dp, _, names in os.walk(os.path.join(FIX, "bad")):
            for n in names:
                with open(os.path.join(dp, n), encoding="utf-8") as fh:
                    code = fh.readline().strip()[len("# expect: "):]
                found[code] = found.get(code, 0) + 1
        for code, least in self.CODES.items():
            self.assertGreaterEqual(found.get(code, 0), least, code)


if __name__ == "__main__":
    unittest.main()
