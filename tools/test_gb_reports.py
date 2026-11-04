#!/usr/bin/env python3
"""Tests for the audit reports, reconcile, training class, review sample and determinism of tools/gb_audit.py
(SPEC-013, ticket T-135; T13-08, T13-09 and the training_class half of T13-10). Run: python3 tools/test_gb_reports.py"""
import ast
import copy
import io
import os
import shutil
import subprocess
import sys
import tempfile
import tomllib
import unittest
from contextlib import redirect_stderr, redirect_stdout

TOOLS = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(TOOLS)
sys.path.insert(0, TOOLS)

import gb_audit  # noqa: E402

FIX = os.path.join(REPO, "qc", "fixtures", "gb")
GOOD = os.path.join(FIX, "good")
SAMPLE = os.path.join(GOOD, "AP-probe.first.report.toml")
SRC = '[{ document = "CWV", page = 72 }]'


def report(pass_="first", role="auditor", session="s-first", packet="AP-probe", **extra):
    r = {"schema": "gb-audit/1", "packet": packet, "scope": ["us_probe_brigade"], "pass": pass_,
         "agent": {"role": role, "session": session}, "date": "2026-11-04", "sources_used": ["CWV"], "findings": []}
    r.update(extra)
    return r


def reviewed(session="s-review", reviewer_of=("s-first", "s-blind"), **extra):
    return report("review", "reviewer", session, review={"sample_size": 10, "re_derived": 10, "mismatches": 0,
                                                         "outcome": "pass", "reviewer_of": list(reviewer_of)}, **extra)


def promoting(**extra):
    return report("reconcile", "reconciler", "s-rec", counts={"promoted": 1}, reconciliation=[
        {"record": "us_probe_regiment", "path": "strength/engaged", "decision": "verified", "value": 5,
         "claims": [{"document": "CWV", "page": 1, "figure": 5}, {"document": "ALT", "page": 2, "figure": 5}]}], **extra)


def reports_of(*docs):
    return [gb_audit.Report("qc/audit/gettysburg/%s.%s.report.toml" % (d["packet"], d["pass"]), d) for d in docs]


def rules(found, severity=None):
    return sorted({f.rule for f in found if severity in (None, f.severity)})


class TrainingClass(unittest.TestCase):
    """T13-10, training_class half."""

    def test_spec_cases(self):
        asof = "1863-07-01"
        self.assertEqual(gb_audit.training_class("1862-08-01", 0, asof), "Trained")
        self.assertEqual(gb_audit.training_class("1863-05-01", 0, asof), "Raw")
        self.assertEqual(gb_audit.training_class("1861-07-01", 3, asof), "Veteran")
        self.assertEqual(gb_audit.training_class("1862-08-01", 2, asof), "Trained")

    def test_boundaries(self):
        asof = "1863-07-01"
        # 180 days is six months, 179 days is five
        self.assertEqual(gb_audit.training_class("1863-01-02", 0, asof), "Trained")
        self.assertEqual(gb_audit.training_class("1863-01-03", 0, asof), "Raw")
        # 360 days is twelve months
        self.assertEqual(gb_audit.training_class("1862-07-06", 3, asof), "Veteran")
        self.assertEqual(gb_audit.training_class("1862-07-07", 3, asof), "Trained")
        self.assertEqual(gb_audit.training_class("1860-01-01", 2, asof), "Trained")
        self.assertEqual(gb_audit.training_class("1863-06-30", 1, asof), "Trained")

    def test_as_of_before_raised_is_raw(self):
        self.assertEqual(gb_audit.training_class("1863-08-01", 0, "1863-07-01"), "Raw")

    def test_bad_dates_refused(self):
        for raised, asof in (("1862-02-30", "1863-07-01"), ("1862-08", "1863-07-01"), ("1862-08-01", "July 1"),
                             (None, "1863-07-01")):
            with self.assertRaises(ValueError, msg=repr((raised, asof))):
                gb_audit.training_class(raised, 0, asof)


class ReviewSample(unittest.TestCase):
    def test_sizes(self):
        got = [gb_audit.review_sample_size(n) for n in (0, 1, 9, 10, 11, 100, 101, 1000, 1001, 2000)]
        self.assertEqual(got, [0, 1, 9, 10, 10, 10, 11, 100, 101, 200])


class ReportIntegrity(unittest.TestCase):
    """T13-09 and rule GB19."""

    def test_valid_sample_report_passes(self):
        reps = gb_audit.load_reports(GOOD, GOOD)
        self.assertEqual([r.label for r in reps], ["AP-probe.first.report.toml"])
        self.assertEqual(gb_audit.check_reports(reps), [])

    def test_sample_report_is_what_the_writer_produces(self):
        with open(SAMPLE, "rb") as f:
            raw = f.read()
        doc = tomllib.loads(raw.decode("utf-8"))
        with tempfile.TemporaryDirectory() as tmp:
            p = os.path.join(tmp, "x.toml")
            gb_audit.write_report(p, doc, doc["date"])
            with open(p, "rb") as f:
                self.assertEqual(f.read(), raw)

    def test_full_set_is_clean(self):
        docs = [report(), report("second_blind", "blind_auditor", "s-blind", saw_prior_values=False),
                promoting(), reviewed()]
        self.assertEqual(gb_audit.check_reports(reports_of(*docs)), [])

    def test_blind_pass_that_saw_values_fails(self):
        r = report("second_blind", "blind_auditor", "s-blind", saw_prior_values=True)
        found = gb_audit.check_reports(reports_of(r))
        self.assertIn("GB19", rules(found, "E"))

    def test_reviewer_session_equal_to_an_audited_session_fails(self):
        r = reviewed(session="s-first")  # reviewer_of names the reviewer's own session
        self.assertIn("GB19", rules(gb_audit.check_reports(reports_of(r)), "E"))
        r = reviewed(session="s-other", reviewer_of=("s-blind",))
        docs = [report(session="s-other"), r]  # the first pass of the packet ran in the reviewer's session
        self.assertIn("GB19", rules(gb_audit.check_reports(reports_of(*docs)), "E"))
        docs = [report("second_blind", "blind_auditor", "s-other", saw_prior_values=False), r]
        self.assertIn("GB19", rules(gb_audit.check_reports(reports_of(*docs)), "E"))

    def test_another_packets_session_is_not_a_clash(self):
        docs = [report(packet="AP-other", session="s-review"), reviewed()]
        self.assertEqual(gb_audit.check_reports(reports_of(*docs)), [])

    def test_promotion_without_reviewer_report_fails(self):
        self.assertIn("GB19", rules(gb_audit.check_reports(reports_of(promoting())), "E"))
        docs = [promoting(), reviewed(packet="AP-other")]
        self.assertIn("GB19", rules(gb_audit.check_reports(reports_of(*docs)), "E"))
        counted = report(counts={"promoted": 2})
        self.assertIn("GB19", rules(gb_audit.check_reports(reports_of(counted)), "E"))

    def test_demotions_and_non_promoting_decisions_need_no_reviewer(self):
        r = report("reconcile", "reconciler", "s-rec", counts={"demoted": 3}, reconciliation=[
            {"record": "x", "path": "p", "decision": "partial_range", "low": 1, "high": 2,
             "claims": [{"document": "CWV", "page": 1, "figure": 1}]}])
        self.assertEqual(gb_audit.check_reports(reports_of(r)), [])

    def test_waiver_without_reason_fails(self):
        r = report(findings=[{"rule": "GB10", "severity": "W", "record": "us_a", "message": "m", "disposition": "waived"}])
        self.assertEqual(rules(gb_audit.check_reports(reports_of(r)), "E"), ["GB01"])
        r["findings"][0]["waiver_reason"] = "the volume is silent"
        self.assertEqual(gb_audit.check_reports(reports_of(r)), [])

    def test_float_and_schema_errors_fail(self):
        r = report(counts={"records": 1.5})
        self.assertIn("GB02", rules(gb_audit.check_reports(reports_of(r)), "E"))
        r = report()
        r["pass"] = "third"
        self.assertIn("GB01", rules(gb_audit.check_reports(reports_of(r)), "E"))

    def test_file_name_must_name_packet_and_pass(self):
        rep = gb_audit.Report("qc/audit/gettysburg/AP-probe.second_blind.report.toml", report())
        self.assertEqual(rules(gb_audit.check_reports([rep]), "E"), ["GB03"])

    def test_findings_are_sorted_and_input_untouched(self):
        docs = [promoting(), report("second_blind", "blind_auditor", "s-b", saw_prior_values=True)]
        before = copy.deepcopy(docs)
        found = gb_audit.check_reports(reports_of(*docs))
        self.assertEqual(found, sorted(found))
        self.assertEqual(docs, before)


class ReportLoading(unittest.TestCase):
    def test_load_is_sorted_filtered_and_labelled(self):
        with tempfile.TemporaryDirectory() as tmp:
            d = os.path.join(tmp, "qc", "audit", "gettysburg")
            os.makedirs(d)
            for name, text in (("AP-b.first.report.toml", 'packet = "AP-b"\n'), ("AP-a.first.report.toml", 'packet = "AP-a"\n'),
                               ("notes.toml", "x = 1\n"), ("AP-c.first.report.toml", "not toml [")):
                with open(os.path.join(d, name), "w", encoding="utf-8") as f:
                    f.write(text)
            reps = gb_audit.load_reports(tmp)
            self.assertEqual([r.label for r in reps], ["qc/audit/gettysburg/AP-a.first.report.toml",
                                                       "qc/audit/gettysburg/AP-b.first.report.toml",
                                                       "qc/audit/gettysburg/AP-c.first.report.toml"])
            self.assertIsNone(reps[2].doc)
            found = gb_audit.check_reports(reps)
            self.assertEqual([f.rule for f in found if f.path == reps[2].label], ["GB01"])

    def test_missing_directory_is_empty(self):
        with tempfile.TemporaryDirectory() as tmp:
            self.assertEqual(gb_audit.load_reports(tmp), [])


def write(path, text):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8") as f:
        f.write(text)


def unit_text(uid, engaged=None, claims=(), hq=None, parent="us_probe_brigade", level="regiment"):
    t = 'schema = "gb-unit/1"\nid = "%s"\nside = "union"\nlevel = "%s"\nname = "Probe"\n' % (uid, level)
    if level == "regiment":
        t += 'designation = { state = "PA", ordinal = 1, arm = "infantry" }\n'
    if parent:
        t += '\n[[assignments]]\nparent = "%s"\nkind = "organic"\nconfidence = "unverified"\nsources = %s\n' % (parent, SRC)
    if engaged is not None:
        t += '\n[strength.engaged]\nvalue = %d\nconfidence = "unverified"\nsources = %s\n' % (engaged, SRC)
        if claims:
            t += "claims = [%s]\n" % ", ".join(
                "{ document = \"%s\", page = %d, figure = %d }" % c for c in claims)
    if hq is not None:
        t += '\n[strength.headquarters]\nvalue = %d\nconfidence = "unverified"\nsources = %s\n' % (hq, SRC)
    return t


def corpus(*units):
    """A throwaway corpus: the good registry plus unit records given as (id, text)."""
    tmp = tempfile.mkdtemp()
    shutil.copy(os.path.join(GOOD, "sources.toml"), os.path.join(tmp, "sources.toml"))
    for uid, text in units:
        write(os.path.join(tmp, "units", uid + ".toml"), text)
    return gb_audit.load_corpus(tmp)


def one(claims, rid="us_probe_regiment"):
    return corpus((rid, unit_text(rid, 100, claims)))


def decide(first, second):
    rows = gb_audit.reconcile(first, second)
    return rows


class Reconcile(unittest.TestCase):
    def test_two_lineages_that_agree_are_verified(self):
        rows = decide(one([("CWV", 72, 250)]), one([("ALT", 5, 250)]))
        self.assertEqual(len(rows), 1)
        self.assertEqual((rows[0]["record"], rows[0]["path"], rows[0]["decision"], rows[0]["value"]),
                         ("us_probe_regiment", "strength/engaged", "verified", 250))
        self.assertEqual([(c["document"], c["page"], c["figure"]) for c in rows[0]["claims"]],
                         [("ALT", 5, 250), ("CWV", 72, 250)])

    def test_rows_validate_against_the_report_schema(self):
        rows = decide(one([("CWV", 72, 250)]), one([("ALT", 5, 260)]))
        rep = report(reconciliation=rows)
        self.assertEqual(gb_audit.check_report(rep), [])

    def test_same_lineage_agreement_is_not_promoted(self):
        rows = decide(one([("CWV", 72, 250)]), one([("ENC", 3, 250)]))
        self.assertEqual((rows[0]["decision"], rows[0]["value"]), ("unverified", 250))

    def test_same_claim_in_both_passes_is_one_claim(self):
        rows = decide(one([("CWV", 72, 250)]), one([("CWV", 72, 250)]))
        self.assertEqual((rows[0]["decision"], len(rows[0]["claims"])), ("unverified", 1))

    def test_unconsulted_source_is_not_promoted(self):
        rows = decide(one([("CWV", 72, 250)]), one([("UNC", 5, 250)]))
        self.assertEqual(rows[0]["decision"], "unverified")

    def test_disagreement_is_a_range(self):
        rows = decide(one([("CWV", 72, 245)]), one([("ALT", 5, 255), ("CWV", 73, 250)]))
        self.assertEqual((rows[0]["decision"], rows[0]["low"], rows[0]["high"]), ("partial_range", 245, 255))
        self.assertNotIn("value", rows[0])

    def test_disagreement_with_no_consulted_source_is_unresolved(self):
        rows = decide(one([("UNC", 5, 245)]), one([("UNC", 6, 255)]))
        self.assertEqual((rows[0]["decision"], rows[0]["low"], rows[0]["high"]), ("unresolved", 245, 255))

    def test_a_source_that_gives_a_range_is_a_range(self):
        first = corpus(("us_probe_regiment", unit_text("us_probe_regiment", 100).replace(
            'value = 100\nconfidence = "unverified"\nsources = %s\n' % SRC, 'value = 100\nconfidence = "unverified"\nsources = %s\nclaims = [{ document = "CWV", page = 72, figure = 90, figure_high = 110 }]\n' % SRC, 1)))
        rows = decide(first, corpus())
        self.assertEqual((rows[0]["decision"], rows[0]["low"], rows[0]["high"]), ("partial_range", 90, 110))

    def test_fact_in_one_corpus_only_and_sorted_output(self):
        a = corpus(("us_probe_b", unit_text("us_probe_b", 5, [("CWV", 1, 5)])),
                   ("us_probe_a", unit_text("us_probe_a", 6, [("CWV", 2, 6)])))
        b = corpus(("us_probe_c", unit_text("us_probe_c", 7, [("ALT", 3, 7)])))
        rows = decide(a, b)
        self.assertEqual([r["record"] for r in rows], ["us_probe_a", "us_probe_b", "us_probe_c"])
        self.assertEqual({r["decision"] for r in rows}, {"unverified"})

    def test_facts_without_claims_are_left_out_and_inputs_untouched(self):
        a = corpus(("us_probe_a", unit_text("us_probe_a", 6)))
        self.assertEqual(decide(a, a), [])

    def test_order_of_the_two_corpora_does_not_matter(self):
        a, b = one([("CWV", 72, 250)]), one([("ALT", 5, 250)])
        self.assertEqual(decide(a, b), decide(b, a))


def cli(*args, cwd=REPO, env=None):
    p = subprocess.run([sys.executable, os.path.join(TOOLS, "gb_audit.py")] + list(args), cwd=cwd, env=env,
                       capture_output=True)
    return p.returncode, p.stdout, p.stderr


class Determinism(unittest.TestCase):
    """T13-08."""

    def test_module_has_no_wall_clock_or_randomness(self):
        with open(os.path.join(TOOLS, "gb_audit.py"), encoding="utf-8") as f:
            tree = ast.parse(f.read())
        banned_modules = {"time", "datetime", "random", "secrets", "uuid"}
        banned_calls = {"now", "today", "utcnow", "time", "time_ns", "monotonic", "perf_counter", "process_time",
                        "ctime", "localtime", "gmtime", "random", "getrandbits", "urandom", "strftime"}
        for node in ast.walk(tree):
            if isinstance(node, ast.Import):
                self.assertFalse({a.name.split(".")[0] for a in node.names} & banned_modules, node.lineno)
            elif isinstance(node, ast.ImportFrom):
                self.assertNotIn((node.module or "").split(".")[0], banned_modules, node.lineno)
            elif isinstance(node, ast.Call):
                fn = node.func
                name = fn.attr if isinstance(fn, ast.Attribute) else getattr(fn, "id", "")
                self.assertNotIn(name, banned_calls, "line %d" % node.lineno)

    def test_check_output_is_byte_identical_across_runs_and_hash_seeds(self):
        with tempfile.TemporaryDirectory() as tmp:
            shutil.copytree(os.path.join(FIX, "bad"), os.path.join(tmp, "tree", "data", "history", "gettysburg"))
            shutil.copy(os.path.join(GOOD, "sources.toml"), os.path.join(tmp, "tree", "data", "history", "gettysburg"))
            reports = os.path.join(tmp, "tree", "qc", "audit", "gettysburg")
            write(os.path.join(reports, "AP-probe.second_blind.report.toml"),
                  gb_audit.toml_dumps(report("second_blind", "blind_auditor", "s-b", saw_prior_values=True)))
            write(os.path.join(reports, "AP-probe.reconcile.report.toml"), gb_audit.toml_dumps(promoting()))
            outs = []
            for seed in ("0", "1", "0", "1", "random"):
                env = dict(os.environ, PYTHONHASHSEED=seed)
                code, out, _ = cli("check", os.path.join(tmp, "tree"), env=env)
                self.assertEqual(code, 1)
                outs.append(out)
            self.assertGreater(len(outs[0]), 500)
            self.assertIn(b"GB19", outs[0])
            self.assertEqual(len(set(outs)), 1)

    def test_repository_check_is_repeatable(self):
        a, b = cli("check"), cli("check", env=dict(os.environ, PYTHONHASHSEED="1"))
        self.assertEqual(a, b)
        self.assertEqual(a[0], 0)

    def test_reconcile_and_report_writers_are_byte_identical(self):
        with tempfile.TemporaryDirectory() as tmp:
            a, b = os.path.join(tmp, "a"), os.path.join(tmp, "b")
            shutil.copytree(GOOD, a)
            write(os.path.join(b, "units", "us_probe_regiment.toml"), unit_text("us_probe_regiment", 100, [("ALT", 5, 250)]))
            shutil.copy(os.path.join(GOOD, "sources.toml"), b)
            outs = [cli("reconcile", a, b, env=dict(os.environ, PYTHONHASHSEED=s))[1] for s in ("0", "1", "2")]
            self.assertEqual(len(set(outs)), 1)
            self.assertIn(b"us_probe_regiment", outs[0])
            doc = tomllib.loads(outs[0].decode("utf-8"))
            self.assertEqual(gb_audit.check_report(report(reconciliation=doc["reconciliation"])), [])


class ReportCommand(unittest.TestCase):
    """The `report PACKET --date D` command."""

    def run_report(self, root, *args):
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            code = gb_audit.main(["gb_audit.py", "report"] + list(args) + ["--root", root])
        return code, out.getvalue(), err.getvalue()

    def tree(self):
        tmp = tempfile.mkdtemp()
        shutil.copytree(GOOD, os.path.join(tmp, "data", "history", "gettysburg"))
        os.remove(os.path.join(tmp, "data", "history", "gettysburg", "AP-probe.first.report.toml"))
        return tmp

    def test_writes_a_valid_sorted_report_for_a_packet(self):
        tmp = self.tree()
        code, out, _ = self.run_report(tmp, "AP-probe_brigade", "--date", "2026-11-04", "--session", "s-9")
        self.assertEqual(code, 0, out)
        path = os.path.join(tmp, "qc", "audit", "gettysburg", "AP-probe_brigade.first.report.toml")
        self.assertTrue(os.path.isfile(path))
        with open(path, "rb") as f:
            doc = tomllib.loads(f.read().decode("utf-8"))
        self.assertEqual(gb_audit.check_report(doc), [])
        self.assertEqual((doc["date"], doc["pass"], doc["agent"]), ("2026-11-04", "first", {"role": "auditor", "session": "s-9"}))
        self.assertEqual(doc["scope"], ["us_probe_brigade", "us_probe_regiment", "us_probe_regiment_b"])
        self.assertEqual(doc["sources_used"], ["CWV"])
        self.assertEqual(doc["counts"], {"records": 3, "claims": 2})
        self.assertEqual(gb_audit.check_reports(gb_audit.load_reports(tmp)), [])

    def test_headline_sets_the_audited_sum_beside_the_figure(self):
        tmp = self.tree()
        base = os.path.join(tmp, "data", "history", "gettysburg", "units")
        write(os.path.join(base, "us_probe_brigade.toml"), unit_text("us_probe_brigade", 900, parent="us_probe_division", level="brigade"))
        code, _, _ = self.run_report(tmp, "AP-probe_brigade", "--date", "2026-11-04")
        with open(os.path.join(tmp, "qc", "audit", "gettysburg", "AP-probe_brigade.first.report.toml"), "rb") as f:
            doc = tomllib.loads(f.read().decode("utf-8"))
        self.assertEqual(doc["headline"], [{"record": "us_probe_brigade", "path": "strength/engaged", "headline": 900,
                                            "audited_low": 540, "audited_high": 560}])

    def test_findings_of_the_packet_are_listed_open(self):
        tmp = self.tree()
        p = os.path.join(tmp, "data", "history", "gettysburg", "units", "us_probe_regiment.toml")
        with open(p, encoding="utf-8") as f:
            text = f.read()
        write(p, text.replace("low = 240", "low = 251"))
        code, _, _ = self.run_report(tmp, "AP-probe_brigade", "--date", "2026-11-04")
        self.assertEqual(code, 1)
        with open(os.path.join(tmp, "qc", "audit", "gettysburg", "AP-probe_brigade.first.report.toml"), "rb") as f:
            doc = tomllib.loads(f.read().decode("utf-8"))
        self.assertTrue(doc["findings"])
        self.assertEqual({f["disposition"] for f in doc["findings"]}, {"open"})
        self.assertEqual({f["record"] for f in doc["findings"]}, {"us_probe_regiment"})

    def test_blind_pass_report(self):
        tmp = self.tree()
        code, _, _ = self.run_report(tmp, "AP-probe_brigade", "--date", "2026-11-04", "--pass", "second_blind")
        self.assertEqual(code, 0)
        with open(os.path.join(tmp, "qc", "audit", "gettysburg", "AP-probe_brigade.second_blind.report.toml"), "rb") as f:
            doc = tomllib.loads(f.read().decode("utf-8"))
        self.assertIs(doc["saw_prior_values"], False)
        self.assertEqual(doc["agent"]["role"], "blind_auditor")

    def test_bad_usage_exits_2_and_writes_nothing(self):
        tmp = self.tree()
        for args in (["AP-probe_brigade"], ["AP-probe_brigade", "--date", "11/04/2026"], ["AP-nothing_here", "--date", "2026-11-04"],
                     ["probe", "--date", "2026-11-04"], ["AP-probe_brigade", "--date", "2026-11-04", "--pass", "review"],
                     ["AP-probe_brigade", "--date", "2026-11-04", "--bogus", "1"]):
            code, _, err = self.run_report(tmp, *args)
            self.assertEqual(code, 2, args)
            self.assertTrue(err, args)
        self.assertFalse(os.path.exists(os.path.join(tmp, "qc")))


class CheckCommand(unittest.TestCase):
    def test_check_prints_report_findings_and_fails_on_gb19(self):
        with tempfile.TemporaryDirectory() as tmp:
            shutil.copytree(GOOD, os.path.join(tmp, "data", "history", "gettysburg"))
            write(os.path.join(tmp, "qc", "audit", "gettysburg", "AP-probe.reconcile.report.toml"), gb_audit.toml_dumps(promoting()))
            code, out, _ = cli("check", tmp)
            self.assertEqual(code, 1)
            self.assertIn(b"qc/audit/gettysburg/AP-probe.reconcile.report.toml: GB19 E", out)


if __name__ == "__main__":
    unittest.main()
