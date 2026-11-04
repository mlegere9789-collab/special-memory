#!/usr/bin/env python3
"""Tests for the evidence ladder (GB04 to GB06) and the known-conflict fixtures (GB17) of tools/gb_audit.py
(SPEC-013, ticket T-132; T13-06 and T13-07). Run: python3 tools/test_gb_ladder.py"""
import os
import shutil
import sys
import tempfile
import tomllib
import unittest

TOOLS = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(TOOLS)
sys.path.insert(0, TOOLS)

import gb_audit  # noqa: E402

FIXTURE_DIR = os.path.join(REPO, "qc", "fixtures")
FIXTURE = os.path.join(FIXTURE_DIR, "gb-known-conflicts.toml")
GOOD = os.path.join(FIXTURE_DIR, "gb", "good")


def src(code, lineage, consulted=True, licence="public_domain", **kw):
    r = {"code": code, "title": "t", "kind": "study", "lineage": lineage, "licence_class": licence, "consulted": consulted}
    r.update(kw)
    return r


REG = {r["code"]: r for r in (src("CWV", "web"), src("ENC", "web"), src("ALT", "alt"), src("BET", "bet"),
                              src("UNC", "unc", consulted=False), src("PERM", "perm", licence="permission_required"),
                              src("CLR", "clr", licence="permission_required", cleared_by={"who": "c", "date": "2026-01-01"}))}


def claim(doc, page, figure, **kw):
    return dict({"document": doc, "page": page, "figure": figure}, **kw)


def fact(value=250, claims=None, second="ALT:5", confidence="verified", **kw):
    claims = [claim("CWV", 72, 250), claim("ALT", 5, 250)] if claims is None else claims
    f = {"value": value, "confidence": confidence, "sources": [{"document": "CWV", "page": 72}], "claims": claims}
    if second:
        f["second_source"] = second
    f.update(kw)
    return f


def tfact(claims, at="1863-07-02T16:00", second="ALT:5", **kw):
    f = {"at": at, "confidence": "verified", "sources": [{"document": "CWV", "page": 72}], "claims": claims, "second_source": second}
    f.update(kw)
    return f


def tclaim(doc, page, at, **kw):
    return dict({"document": doc, "page": page, "at": at}, **kw)


class Ladder(unittest.TestCase):
    """T13-06: verified fails and passes as the ladder says."""

    def problems(self, f):
        return gb_audit.evidence_problems(REG, f)

    def test_two_agreeing_consulted_lineages_with_matching_second_source_pass(self):
        self.assertEqual(self.problems(fact()), [])
        self.assertEqual(gb_audit.evidence_level(REG, fact()), "verified")

    def test_second_source_may_name_either_claim(self):
        self.assertEqual(self.problems(fact(second="ALT:5")), [])
        self.assertEqual(self.problems(fact(second="CWV:72")), [])

    def test_one_claim_fails(self):
        self.assertTrue(self.problems(fact(claims=[claim("ALT", 5, 250)])))

    def test_two_claims_of_one_lineage_fail(self):
        f = fact(claims=[claim("CWV", 72, 250), claim("ENC", 5, 250)], second="ENC:5")
        self.assertTrue(self.problems(f))

    def test_two_disagreeing_lineages_fail(self):
        f = fact(claims=[claim("CWV", 72, 250), claim("ALT", 5, 260)])
        self.assertTrue(self.problems(f))

    def test_unconsulted_source_fails(self):
        f = fact(claims=[claim("CWV", 72, 250), claim("UNC", 5, 250)], second="UNC:5")
        self.assertTrue(self.problems(f))

    def test_uncleared_source_fails_and_cleared_passes(self):
        f = fact(claims=[claim("CWV", 72, 250), claim("PERM", 5, 250)], second="PERM:5")
        self.assertTrue(self.problems(f))
        f = fact(claims=[claim("CWV", 72, 250), claim("CLR", 5, 250)], second="CLR:5")
        self.assertEqual(self.problems(f), [])

    def test_cwv_alone_fails(self):
        f = fact(claims=[claim("CWV", 72, 250), claim("CWV", 73, 250)], second="CWV:73")
        self.assertTrue(self.problems(f))

    def test_second_source_must_match_a_claim(self):
        for second in ("ALT:9", "BET:5", "ALT", None):
            self.assertTrue(self.problems(fact(second=second)), second)

    def test_value_must_equal_the_claims(self):
        self.assertTrue(self.problems(fact(value=251)))

    def test_claim_with_its_own_range_is_not_a_point(self):
        f = fact(claims=[claim("CWV", 72, 250), claim("ALT", 5, 250, figure_high=260)])
        self.assertTrue(self.problems(f))

    def test_times_agree_within_the_tolerance(self):
        ok = tfact([tclaim("CWV", 72, "1863-07-02T16:00"), tclaim("ALT", 5, "1863-07-02T16:15")])
        self.assertEqual(self.problems(ok), [])
        late = tfact([tclaim("CWV", 72, "1863-07-02T16:00"), tclaim("ALT", 5, "1863-07-02T16:16")])
        self.assertTrue(self.problems(late))
        span = tfact([tclaim("CWV", 72, "1863-07-02T16:00", at_late="1863-07-02T16:20"), tclaim("ALT", 5, "1863-07-02T16:00")])
        self.assertTrue(self.problems(span))

    def test_time_value_must_sit_inside_the_claims(self):
        f = tfact([tclaim("CWV", 72, "1863-07-02T16:00"), tclaim("ALT", 5, "1863-07-02T16:10")], at="1863-07-02T16:30")
        self.assertTrue(self.problems(f))

    def test_lineage_lookup(self):
        self.assertEqual(gb_audit.lineage_of(REG, "CWV"), "web")
        self.assertEqual(gb_audit.lineage_of(REG, "ENC"), "web")
        self.assertIsNone(gb_audit.lineage_of(REG, "ZZZ"))

    def test_claims_agree(self):
        self.assertTrue(gb_audit.claims_agree([claim("A", 1, 5), claim("B", 2, 5)]))
        self.assertFalse(gb_audit.claims_agree([claim("A", 1, 5), claim("B", 2, 6)]))
        self.assertTrue(gb_audit.claims_agree([]))

    def test_second_source_matching(self):
        self.assertTrue(gb_audit.second_source_matches(fact()))
        self.assertFalse(gb_audit.second_source_matches(fact(second="ALT:6")))
        self.assertFalse(gb_audit.second_source_matches(fact(second=None)))

    def test_level_of_unverified_and_partial(self):
        one = fact(claims=[claim("CWV", 72, 250)], confidence="partial")
        self.assertEqual(gb_audit.evidence_level(REG, one), "partial")
        unconsulted = fact(claims=[claim("UNC", 5, 250)], confidence="partial")
        self.assertEqual(gb_audit.evidence_level(REG, unconsulted), "unverified")
        clash = fact(claims=[claim("CWV", 72, 250), claim("ALT", 5, 260)], confidence="partial")
        self.assertEqual(gb_audit.evidence_level(REG, clash), "unverified")
        self.assertEqual(gb_audit.evidence_level(REG, dict(clash, low=250, high=260)), "partial")

    def test_demotion_on_disagreement(self):
        f = fact()
        self.assertEqual(gb_audit.demoted(REG, f)["confidence"], "verified")
        f["claims"].append(claim("BET", 3, 260))
        f["low"], f["high"] = 250, 260
        d = gb_audit.demoted(REG, f)
        self.assertEqual(d["confidence"], "partial")
        self.assertEqual(f["confidence"], "verified")  # the input is not changed
        f2 = fact(claims=[claim("CWV", 72, 250), claim("ALT", 5, 260)])
        self.assertEqual(gb_audit.demoted(REG, f2)["confidence"], "unverified")


def corpus_with(*extra):
    tmp = tempfile.TemporaryDirectory()
    shutil.copytree(GOOD, os.path.join(tmp.name, "tree"))
    for rel, text in extra:
        p = os.path.join(tmp.name, "tree", rel)
        os.makedirs(os.path.dirname(p), exist_ok=True)
        with open(p, "w", encoding="utf-8") as f:
            f.write(text)
    return tmp, gb_audit.load_corpus(os.path.join(tmp.name, "tree"))


class CorpusRules(unittest.TestCase):
    def rules(self, rel, text):
        tmp, c = corpus_with((rel, text))
        with tmp:
            return [f.rule for f in gb_audit.check_corpus(c) if f.path == rel]

    def test_good_verified_probe_is_clean(self):
        tmp, c = corpus_with()
        with tmp:
            self.assertIn("units/us_probe_verified.toml", [r.label for r in c.records])
            self.assertEqual(gb_audit.check_corpus(c), [])

    def test_citation_rules_run_in_the_corpus(self):
        with open(os.path.join(GOOD, "units", "us_probe_verified.toml"), encoding="utf-8") as f:
            text = f.read().replace("us_probe_verified", "us_probe_x")
        self.assertEqual(self.rules("units/us_probe_x.toml", text), [])
        self.assertEqual(sorted(set(self.rules("units/us_probe_x.toml", text.replace('document = "ALT"', 'document = "UNC"').replace("ALT:5", "UNC:5")))),
                         ["GB04", "GB06"])
        self.assertIn("GB05", self.rules("units/us_probe_x.toml", text.replace('document = "ALT"', 'document = "PERM"').replace("ALT:5", "PERM:5")))
        self.assertIn("GB04", self.rules("units/us_probe_x.toml", text.replace('document = "ALT"', 'document = "NOPE"').replace("ALT:5", "NOPE:5")))

    def test_bad_probes_cover_the_ladder_rules(self):
        codes = set()
        for dp, _, names in os.walk(os.path.join(FIXTURE_DIR, "gb", "bad")):
            for n in names:
                with open(os.path.join(dp, n), encoding="utf-8") as f:
                    codes.add(f.readline().strip())
        for rule in ("GB04", "GB05", "GB06"):
            self.assertIn("# expect: " + rule, codes)

    def test_without_a_registry_citations_are_not_judged(self):
        with tempfile.TemporaryDirectory() as d:
            shutil.copytree(os.path.join(GOOD, "units"), os.path.join(d, "units"))
            self.assertEqual(gb_audit.check_corpus(gb_audit.load_corpus(d)), [])


def load_rows():
    with open(FIXTURE, "rb") as f:
        return tomllib.load(f)["conflict"]


def put(doc, path, node):
    """Set `node` at `path` in `doc`; casualties are a list of scope tables."""
    parts, cur, i = path.split("/"), doc, 0
    while i < len(parts) - 1:
        if parts[i] == "casualties":
            item = {"scope": parts[i + 1]}
            cur.setdefault("casualties", []).append(item)
            cur, i = item, i + 2
            continue
        cur = cur.setdefault(parts[i], {})
        i += 1
    if i < len(parts):
        cur[parts[i]] = node
    return doc


def make_node(claims, confidence="partial", flag=True, ranged=True):
    """A fact node holding `claims` (from a fixture row) with a range that spans them."""
    if "at" in claims[0]:
        spans = [gb_audit.claim_span(c) for c in claims]
        lo, hi = min(a for a, _ in spans), max(b for _, b in spans)
        n = {"at": gb_audit.stamp_of(lo), "confidence": confidence, "claims": claims}
        if ranged:
            n.update(early=gb_audit.stamp_of(lo), late=gb_audit.stamp_of(hi))
    else:
        lo, hi = min(c["figure"] for c in claims), max(c.get("figure_high", c["figure"]) for c in claims)
        n = {"value": lo, "confidence": confidence, "claims": claims}
        if ranged:
            n.update(low=lo, high=hi)
    if flag:
        n["flag"] = "recorded conflict"
    return n


def corpus_of(docs):
    """A corpus of in-memory records; the id gives the folder."""
    recs = []
    for d in docs:
        kind = "events" if d["id"].startswith("ev_") else "weather" if d["id"].startswith("wx_") else "units"
        recs.append(gb_audit.Record(kind, "%s/%s.toml" % (kind, d["id"]), d["id"], d))
    return gb_audit.Corpus("/nowhere", gb_audit.SCHEMA_DIR, {}, recs, [])


def groups_of(row):
    out = {}
    for c in row["claims"]:
        out.setdefault((c.get("record", row["record"]), c.get("path", row["path"])), []).append(
            {k: v for k, v in c.items() if k not in ("record", "path")})
    return out


def write_fixture(rows):
    d = tempfile.TemporaryDirectory()
    with open(os.path.join(d.name, "gb-known-conflicts.toml"), "w", encoding="utf-8") as f:
        f.write(gb_audit.toml_dumps({"conflict": rows}))
    return d


def check_row(row, docs):
    d = write_fixture([row])
    with d:
        return gb_audit.check_fixtures(corpus_of(docs), d.name)


def docs_for(row, only_primary=False, **kw):
    docs = {}
    for (rec, path), claims in sorted(groups_of(row).items()):
        if only_primary and (rec, path) != (row["record"], row["path"]):
            continue
        node = make_node(claims, **kw)
        put(docs.setdefault(rec, {"id": rec}), path, [node] if path == "segment" else node)
    if row.get("not_before") and row["record"] in docs:
        docs[row["record"]]["not_before"] = [{"event": row["not_before"], "gap_min": 0}]
    return list(docs.values())


ROWS = {r["id"]: r for r in load_rows()}
BANNING = ("headline", "range", "time", "sum_conflict", "onset")


class Fixtures(unittest.TestCase):
    """T13-07: C01 to C14 load and are enforced."""

    def test_all_rows_load_in_order(self):
        self.assertEqual(list(ROWS), ["C%02d" % i for i in range(1, 15)])
        for r in ROWS.values():
            for k in ("id", "kind", "record", "path", "claims", "rule"):
                self.assertIn(k, r, r["id"])

    def test_fixture_file_holds_only_untagged_cwv_integer_claims(self):
        with open(FIXTURE, "rb") as f:
            doc = tomllib.load(f)
        self.assertEqual(list(gb_audit.find_floats(doc)), [])
        for r in ROWS.values():
            for cl in r["claims"]:
                self.assertEqual(cl["document"], "CWV", r["id"])
                self.assertEqual(cl["tag"], "untagged", r["id"])

    def test_shipped_fixtures_pass_on_the_repository_and_the_good_tree(self):
        self.assertEqual(gb_audit.check_fixtures(gb_audit.load_corpus(REPO), FIXTURE_DIR), [])
        self.assertEqual(gb_audit.check_fixtures(gb_audit.load_corpus(GOOD), FIXTURE_DIR), [])

    def test_missing_fixture_file_checks_nothing(self):
        with tempfile.TemporaryDirectory() as d:
            self.assertEqual(gb_audit.check_fixtures(gb_audit.load_corpus(GOOD), d), [])

    def test_control_numbers(self):
        self.assertEqual([gb_audit.pct(1153, 1885), gb_audit.pct(397, 496), gb_audit.pct(215, 262)], [61, 80, 82])
        for s in ROWS["C02"]["sums"]:
            self.assertEqual(sum(s["parts"]), s["total"])
        self.assertEqual(sum(ROWS["C03"]["sums"][0]["parts"]), 8892)
        self.assertEqual(ROWS["C07"]["high"], 20444 * 30 // 100)

    def test_integrity_of_a_row_is_checked(self):
        bad = [dict(ROWS["C02"], low=22000), dict(ROWS["C04"], late="1863-07-02T16:45"),
               dict(ROWS["C11"], rates=[{"loss": 1153, "base": 1885, "pct": 62}]),
               dict(ROWS["C02"], sums=[{"parts": [1, 2], "total": 4, "agrees": True}]),
               dict(ROWS["C03"], sums=[{"parts": [1, 2], "total": 3, "agrees": False}]),
               dict(ROWS["C07"], counts=[{"what": "x", "stated": 5, "listed": 5}]),
               dict(ROWS["C04"], kind="nonsense"), dict(ROWS["C13"], ends_by="1863-06-31T03:00"),
               dict(ROWS["C04"], rule="X1")]
        for row in bad:
            found = check_row(row, [])
            self.assertEqual([f.rule for f in found], ["GB17"], row)
            self.assertEqual({f.severity for f in found}, {"E"})

    def test_no_record_means_no_finding(self):
        for row in ROWS.values():
            self.assertEqual(check_row(row, []), [], row["id"])

    def test_a_verified_record_raises_gb17_for_every_banning_row(self):
        for row in ROWS.values():
            if row["kind"] not in BANNING:
                continue
            with self.subTest(row=row["id"]):
                found = check_row(row, docs_for(row, confidence="verified"))
                self.assertTrue([f for f in found if f.rule == "GB17" and "verified" in f.message], found)
                self.assertTrue(all(f.path.endswith(".toml") for f in found))

    def test_the_same_records_unverified_and_spanning_pass(self):
        for row in ROWS.values():
            if row["kind"] not in BANNING:
                continue
            with self.subTest(row=row["id"]):
                self.assertEqual(check_row(row, docs_for(row, confidence="partial")), [])

    def test_headline_and_onset_are_never_verified_even_with_one_claim(self):
        for rid in ("C01", "C14"):
            row = dict(ROWS[rid], claims=ROWS[rid]["claims"][:1])
            found = check_row(row, docs_for(row, confidence="verified"))
            self.assertTrue([f for f in found if "verified" in f.message], rid)

    def test_range_must_span_the_claims(self):
        for rid, low_key, patch in (("C02", "low", 24000), ("C02", "high", 28000), ("C07", "high", 6000), ("C09", "low", 160)):
            with self.subTest(row=rid, key=low_key):
                row = ROWS[rid]
                docs = docs_for(row, only_primary=True)
                node = [n for n in _nodes(docs[0])][0]
                node[low_key] = patch
                self.assertTrue([f for f in check_row(row, docs) if "span" in f.message])
                del node[low_key]
                self.assertTrue([f for f in check_row(row, docs) if "span" in f.message])

    def test_time_range_must_span_the_claims(self):
        for rid, key, patch in (("C04", "early", "1863-07-02T16:10"), ("C04", "late", "1863-07-02T16:20"), ("C06", "late", "1863-07-01T11:00")):
            with self.subTest(row=rid, key=key):
                docs = docs_for(ROWS[rid], only_primary=True)
                [n for n in _nodes(docs[0])][0][key] = patch
                self.assertTrue([f for f in check_row(ROWS[rid], docs) if "span" in f.message])

    def test_sum_conflict_needs_a_flag_and_both_claims(self):
        row = ROWS["C03"]
        self.assertEqual(check_row(row, docs_for(row)), [])
        no_flag = docs_for(row, flag=False)
        self.assertTrue([f for f in check_row(row, no_flag) if "flag" in f.message])
        docs = docs_for(row)
        [n for n in _nodes(docs[0])][0]["claims"] = [row["claims"][0]]
        self.assertTrue([f for f in check_row(row, docs) if "8892" in f.message])

    def test_step_off_must_not_precede_the_message(self):
        row = ROWS["C05"]
        docs = docs_for(row, only_primary=True)
        del docs[0]["not_before"]
        self.assertTrue([f for f in check_row(row, docs) if "not_before" in f.message])
        docs[0]["not_before"] = [{"event": "ev_alexander_message", "gap_min": 0}]
        self.assertEqual(check_row(row, docs), [])

    def test_present_and_engaged_are_both_kept(self):
        row = ROWS["C10"]
        both = {"id": row["record"], "strength": {"present": {"value": 5000}, "engaged": {"value": 3430}}}
        self.assertEqual(check_row(row, [both]), [])
        for drop in ("present", "engaged"):
            one = {"id": row["record"], "strength": {k: v for k, v in both["strength"].items() if k != drop}}
            self.assertTrue([f for f in check_row(row, [one]) if drop in f.message], drop)
        self.assertEqual(check_row(row, [{"id": row["record"], "name": "x"}]), [])

    def test_meade_stint_ends_at_or_before_the_stamp(self):
        row = ROWS["C13"]

        def corps(**stint):
            return [{"id": "us_v_corps", "commanders": [dict({"person": {"id": "meade"}, "kind": "command"}, **stint),
                                                         {"person": {"id": "sykes"}, "kind": "command"}]}]
        self.assertEqual(check_row(row, corps(to="1863-06-28T03:00")), [])
        self.assertEqual(check_row(row, corps(to="1863-06-28T02:00")), [])
        self.assertTrue(check_row(row, corps(to="1863-06-28T03:01")))
        self.assertTrue(check_row(row, corps()))

    def test_roster_members_are_brigades_without_simultaneous_commanders(self):
        row = ROWS["C12"]
        rid = row["members"][0]["record"]

        def brigade(level="brigade", stints=()):
            return [{"id": rid, "level": level, "commanders": [dict({"person": {"id": p}, "kind": "command"}, **t) for p, t in stints]}]
        a = ("a", {"from": "1863-07-01T00:00", "to": "1863-07-02T00:00"})
        self.assertEqual(check_row(row, brigade()), [])
        self.assertEqual(check_row(row, brigade(stints=[a, ("b", {"from": "1863-07-02T00:00", "to": "1863-07-03T00:00"})])), [])
        self.assertTrue([f for f in check_row(row, brigade(stints=[a, ("b", {"from": "1863-07-01T12:00"})])) if "simultaneous" in f.message])
        self.assertTrue(check_row(row, brigade(level="division")))

    def test_check_fixtures_output_is_sorted_and_repeatable(self):
        docs = docs_for(ROWS["C04"], only_primary=True, confidence="verified")
        a, b = check_row(ROWS["C04"], docs), check_row(ROWS["C04"], docs)
        self.assertEqual(a, b)
        self.assertEqual(a, sorted(a))


def _nodes(doc):
    for _, n in gb_audit._fact_nodes(doc):
        yield n


class Plugin(unittest.TestCase):
    def test_validator_plugin_runs_the_fixtures(self):
        import importlib.util
        spec = importlib.util.spec_from_file_location("gb_plugin_t132", os.path.join(TOOLS, "validators", "gettysburg.py"))
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        with tempfile.TemporaryDirectory() as root:
            shutil.copytree(os.path.join(REPO, "data", "schemas"), os.path.join(root, "data", "schemas"))
            os.makedirs(os.path.join(root, "data", "history", "gettysburg", "units"))
            self.assertEqual(module.check_gettysburg(root)[1], [])
            os.makedirs(os.path.join(root, "qc", "fixtures"))
            with open(os.path.join(root, "qc", "fixtures", "gb-known-conflicts.toml"), "w", encoding="utf-8") as f:
                f.write(gb_audit.toml_dumps({"conflict": [dict(ROWS["C04"], kind="nonsense")]}))
            problems = module.check_gettysburg(root)[1]
            self.assertEqual(len(problems), 1, problems)
            self.assertIn("GB17", problems[0])


if __name__ == "__main__":
    unittest.main()
