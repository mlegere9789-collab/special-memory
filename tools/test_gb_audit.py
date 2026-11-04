#!/usr/bin/env python3
"""Tests for tools/gb_audit.py (SPEC-013, ticket T-130). Run: python3 tools/test_gb_audit.py"""
import copy
import glob
import json
import os
import re
import sys
import tempfile
import tomllib
import unittest

TOOLS = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(TOOLS)
sys.path.insert(0, TOOLS)

import gb_audit  # noqa: E402

SOURCES = os.path.join(REPO, "data", "history", "gettysburg", "sources.toml")
CANDIDATES = ["BACH", "BMI", "LOC", "LOSS", "NPS", "OR", "STUDY", "WARREN"]


def row(code, **kw):
    r = {"code": code, "title": "Probe source " + code, "kind": "study", "lineage": "probe_" + code.lower(),
         "licence_class": "public_domain", "consulted": False}
    r.update(kw)
    return r


def consulted_row(code, **kw):
    base = {"consulted": True, "edition": "probe edition", "access": "probe shelf", "licence_text": "probe licence",
            "consulted_on": "2026-01-01", "page_scheme": "printed page"}
    base.update(kw)
    return row(code, **base)


def registry_of(*rows):
    return {r["code"]: r for r in rows}


def rules_of(findings):
    return [f.rule for f in findings]


class TimeFunctions(unittest.TestCase):
    """T13-02"""

    def test_listed_stamps(self):
        cases = {"1863-07-01T00:00": 0, "1863-06-28T03:00": -4140, "1863-07-02T16:30": 2430,
                 "1863-07-03T15:00": 3780, "1863-07-14T23:59": 20159}
        for stamp, want in cases.items():
            self.assertEqual(gb_audit.t_min(stamp), want, stamp)

    def test_round_trip_every_minute_of_window(self):
        for t in range(gb_audit.WINDOW_LO, gb_audit.WINDOW_HI):
            s = gb_audit.stamp_of(t)
            if gb_audit.t_min(s) != t:
                self.fail("round trip failed at %d (%s)" % (t, s))

    def test_stamp_of_known(self):
        self.assertEqual(gb_audit.stamp_of(0), "1863-07-01T00:00")
        self.assertEqual(gb_audit.stamp_of(-1), "1863-06-30T23:59")
        self.assertEqual(gb_audit.stamp_of(20159), "1863-07-14T23:59")

    def test_hour_of_floors(self):
        self.assertEqual(gb_audit.hour_of(-1), -1)
        self.assertEqual(gb_audit.hour_of(-60), -1)
        self.assertEqual(gb_audit.hour_of(-61), -2)
        self.assertEqual(gb_audit.hour_of(0), 0)
        self.assertEqual(gb_audit.hour_of(59), 0)
        self.assertEqual(gb_audit.hour_of(60), 1)
        self.assertEqual(gb_audit.hour_of(2430), 40)

    def test_window_constants(self):
        self.assertEqual((gb_audit.WINDOW_LO, gb_audit.WINDOW_HI), (-4320, 20160))
        self.assertEqual(gb_audit.stamp_of(gb_audit.WINDOW_LO), "1863-06-28T00:00")
        self.assertEqual((gb_audit.BATTLE_LO, gb_audit.BATTLE_HI), (0, 4320))

    def test_impossible_dates_refused(self):
        for bad in ["1863-06-31T10:00", "1863-02-29T10:00", "1863-07-32T00:00", "1863-07-00T00:00",
                    "1863-13-01T00:00", "1863-07-01T24:00", "1863-07-01T12:60"]:
            with self.assertRaises(ValueError, msg=bad):
                gb_audit.t_min(bad)

    def test_malformed_stamps_refused(self):
        for bad in ["1863-07-01", "1863-07-01 10:00", "1863-7-1T10:00", "1863-07-01T10:00Z", "", "1864-07-01T10:00",
                    "1863-07-01T10:00:00", None, 5]:
            with self.assertRaises(ValueError, msg=repr(bad)):
                gb_audit.t_min(bad)

    def test_check_stamp(self):
        self.assertIsNone(gb_audit.check_stamp("1863-07-02T16:30"))
        self.assertIsNone(gb_audit.check_stamp("1863-06-28T00:00"))
        self.assertIsNone(gb_audit.check_stamp("1863-07-14T23:59"))
        self.assertIn("window", gb_audit.check_stamp("1863-06-27T23:59"))
        self.assertIn("window", gb_audit.check_stamp("1863-07-15T00:00"))
        self.assertIn("impossible", gb_audit.check_stamp("1863-06-31T10:00"))
        self.assertIn("malformed", gb_audit.check_stamp("noon"))

    def test_check_battle_day(self):
        self.assertIsNone(gb_audit.check_battle_day("1863-07-01T00:00"))
        self.assertIsNone(gb_audit.check_battle_day("1863-07-03T23:59"))
        self.assertIn("battle", gb_audit.check_battle_day("1863-06-30T23:59"))
        self.assertIn("battle", gb_audit.check_battle_day("1863-07-04T00:00"))

    def test_day_count_is_proleptic_gregorian(self):
        self.assertEqual(gb_audit.days_from_civil(1970, 1, 1), 0)
        self.assertEqual(gb_audit.days_from_civil(2000, 3, 1) - gb_audit.days_from_civil(2000, 2, 28), 2)
        self.assertEqual(gb_audit.days_from_civil(1900, 3, 1) - gb_audit.days_from_civil(1900, 2, 28), 1)


class SchemaChecks(unittest.TestCase):
    """Schema half of T13-01"""

    def test_six_schemas_and_common_pass_metaschema(self):
        self.assertEqual(gb_audit.check_schemas(), [])
        names = sorted(os.path.basename(p) for p in glob.glob(os.path.join(REPO, "data", "schemas", "gb-*.schema.json")))
        self.assertEqual(len(names), 6)
        self.assertEqual(gb_audit.GB_SCHEMA_FILES, names + ["common.schema.json"])

    def test_broken_schema_reported(self):
        with tempfile.TemporaryDirectory() as d:
            for name in gb_audit.GB_SCHEMA_FILES:
                with open(os.path.join(REPO, "data", "schemas", name), encoding="utf-8") as f:
                    text = f.read()
                with open(os.path.join(d, name), "w", encoding="utf-8") as f:
                    f.write(text)
            with open(os.path.join(d, "gb-weather.schema.json"), "w", encoding="utf-8") as f:
                json.dump({"$schema": "https://json-schema.org/draft/2020-12/schema", "type": 12}, f)
            problems = gb_audit.check_schemas(d)
            self.assertEqual(len(problems), 1)
            self.assertEqual(problems[0].rule, "GB01")
            self.assertIn("gb-weather", problems[0].path)


class SourceRegistry(unittest.TestCase):
    def test_shipped_registry_loads_clean(self):
        reg, findings = gb_audit.load_sources(SOURCES)
        self.assertEqual(findings, [])
        self.assertEqual(sorted(reg), sorted(["CWV"] + CANDIDATES))

    def test_only_cwv_is_consulted(self):
        reg, _ = gb_audit.load_sources(SOURCES)
        self.assertEqual([c for c in sorted(reg) if reg[c]["consulted"]], ["CWV"])
        cwv = reg["CWV"]
        self.assertEqual(cwv["kind"], "internal_research")
        self.assertEqual(cwv["lineage"], "web_compilation")
        self.assertEqual(cwv["licence_class"], "internal")
        for field in ("edition", "access", "licence_text", "consulted_on", "page_scheme"):
            self.assertTrue(cwv.get(field), field)

    def test_candidates_carry_no_edition_or_licence_text(self):
        reg, _ = gb_audit.load_sources(SOURCES)
        for code in CANDIDATES:
            r = reg[code]
            self.assertFalse(r["consulted"], code)
            for field in ("edition", "access", "licence_text", "consulted_on", "page_scheme", "cleared_by"):
                self.assertNotIn(field, r, "%s.%s" % (code, field))
        self.assertEqual(reg["NPS"]["licence_class"], "permission_required")

    def test_file_has_no_float(self):
        with open(SOURCES, "rb") as f:
            self.assertEqual(list(gb_audit.find_floats(tomllib.load(f))), [])

    def write_and_load(self, text):
        with tempfile.TemporaryDirectory() as d:
            p = os.path.join(d, "sources.toml")
            with open(p, "w", encoding="utf-8") as f:
                f.write(text)
            return gb_audit.load_sources(p)

    GOOD = 'schema = "gb-sources/1"\n[[source]]\ncode = "AAA"\ntitle = "t"\nkind = "study"\nlineage = "lin_a"\nlicence_class = "unknown"\nconsulted = false\n'

    def test_good_probe(self):
        reg, findings = self.write_and_load(self.GOOD)
        self.assertEqual(findings, [])
        self.assertEqual(list(reg), ["AAA"])

    def test_schema_violation_is_gb01(self):
        _, findings = self.write_and_load(self.GOOD.replace('"study"', '"novel"'))
        self.assertIn("GB01", rules_of(findings))
        self.assertTrue(all(f.severity == "E" for f in findings))

    def test_consulted_without_evidence_is_gb01(self):
        _, findings = self.write_and_load(self.GOOD.replace("consulted = false", "consulted = true"))
        self.assertIn("GB01", rules_of(findings))

    def test_float_is_gb02(self):
        _, findings = self.write_and_load(self.GOOD + "year = 1863.5\n")
        self.assertIn("GB02", rules_of(findings))

    def test_duplicate_code_is_gb03(self):
        body = self.GOOD + self.GOOD.split("\n", 1)[1]
        _, findings = self.write_and_load(body)
        self.assertEqual(rules_of(findings), ["GB03"])

    def test_unconsulted_row_with_edition_refused(self):
        _, findings = self.write_and_load(self.GOOD + 'edition = "second printing"\n')
        self.assertTrue(findings)
        self.assertTrue(all(f.severity == "E" for f in findings))

    def test_invalid_toml_reported(self):
        _, findings = self.write_and_load("schema = \n")
        self.assertEqual(rules_of(findings), ["GB01"])

    def test_missing_file_reported(self):
        reg, findings = gb_audit.load_sources(os.path.join(REPO, "no", "such", "sources.toml"))
        self.assertEqual(reg, {})
        self.assertEqual(rules_of(findings), ["GB01"])


class CitationRules(unittest.TestCase):
    """GB04 and GB05 cases of T13-06"""

    def cite(self, registry, *docs):
        cites = [("sources/%d" % i, d) for i, d in enumerate(docs)]
        return gb_audit.check_citations(registry, "us_probe", cites)

    def test_consulted_source_passes(self):
        self.assertEqual(self.cite(registry_of(consulted_row("AAA")), "AAA"), [])

    def test_missing_from_registry_is_gb04(self):
        f = self.cite(registry_of(consulted_row("AAA")), "ZZZ")
        self.assertEqual(rules_of(f), ["GB04"])
        self.assertEqual(f[0].severity, "E")
        self.assertIn("ZZZ", f[0].message)
        self.assertIn("us_probe", f[0].path)

    def test_unconsulted_source_is_gb04(self):
        f = self.cite(registry_of(row("AAA")), "AAA")
        self.assertEqual(rules_of(f), ["GB04"])

    def test_permission_required_without_clearance_is_gb05(self):
        f = self.cite(registry_of(consulted_row("AAA", licence_class="permission_required")), "AAA")
        self.assertEqual(rules_of(f), ["GB05"])

    def test_unknown_licence_without_clearance_is_gb05(self):
        f = self.cite(registry_of(consulted_row("AAA", licence_class="unknown")), "AAA")
        self.assertEqual(rules_of(f), ["GB05"])

    def test_cleared_source_passes(self):
        cleared = {"who": "counsel", "date": "2026-02-01"}
        for lc in ("permission_required", "unknown"):
            r = consulted_row("AAA", licence_class=lc, cleared_by=cleared)
            self.assertEqual(self.cite(registry_of(r), "AAA"), [], lc)

    def test_unconsulted_and_uncleared_raises_both(self):
        f = self.cite(registry_of(row("AAA", licence_class="permission_required")), "AAA")
        self.assertEqual(rules_of(f), ["GB04", "GB05"])

    def test_free_licence_classes_need_no_clearance(self):
        for lc in ("public_domain", "open_licence", "facts_only", "internal"):
            self.assertEqual(self.cite(registry_of(consulted_row("AAA", licence_class=lc)), "AAA"), [], lc)

    def test_each_citation_reported_and_sorted(self):
        f = self.cite(registry_of(row("AAA")), "ZZZ", "AAA", "AAA")
        self.assertEqual(len(f), 3)
        self.assertEqual(f, sorted(f))

    def test_shipped_registry_cwv_only(self):
        reg, _ = gb_audit.load_sources(SOURCES)
        self.assertEqual(self.cite(reg, "CWV"), [])
        for code in CANDIDATES:
            self.assertIn("GB04", rules_of(self.cite(reg, code)), code)

    def test_citations_of_walks_sources_claims_and_second_source(self):
        rec = {"id": "us_probe",
               "strength": {"engaged": {"value": 5, "confidence": "partial",
                                        "sources": [{"document": "CWV", "page": 3}],
                                        "claims": [{"document": "OR", "page": 9, "figure": 5}],
                                        "second_source": "NPS:12"}}}
        found = gb_audit.citations_of(rec)
        self.assertEqual([d for _, d in found], ["OR", "NPS", "CWV"])
        self.assertEqual(found, sorted(found))

    def test_check_record_citations_end_to_end(self):
        reg = registry_of(consulted_row("AAA"), row("BBB"))
        rec = {"id": "us_probe", "x": {"sources": [{"document": "AAA", "page": 1}, {"document": "BBB", "page": 2}]}}
        f = gb_audit.check_record_citations(reg, rec)
        self.assertEqual(rules_of(f), ["GB04"])


class ReportWriter(unittest.TestCase):
    def report(self):
        return {
            "schema": "gb-audit/1", "packet": "AP-probe", "scope": ["us_probe_brigade"], "pass": "first",
            "agent": {"role": "auditor", "session": "s-1"}, "date": "1900-01-01", "sources_used": ["CWV"],
            "counts": {"records": 2, "claims": 3},
            "findings": [
                {"rule": "GB10", "severity": "W", "record": "us_b", "path": "strength/engaged", "message": "m2",
                 "disposition": "open"},
                {"rule": "GB04", "severity": "E", "record": "us_a", "path": "sources/0", "message": "m1",
                 "disposition": "waived", "waiver_reason": "reason with \"quotes\" and a backslash \\"},
                {"rule": "GB02", "severity": "E", "record": "us_a", "path": "sources/0", "message": "m0",
                 "disposition": "fixed"},
            ],
            "reconciliation": [{"record": "us_b", "path": "p", "decision": "unresolved",
                                "claims": [{"document": "CWV", "page": 1, "figure": 5}]},
                               {"record": "us_a", "path": "p", "decision": "partial_range", "low": 1, "high": 2,
                                "claims": [{"document": "CWV", "page": 1, "figure": 1},
                                           {"document": "CWV", "page": 2, "figure": 2}]}],
            "headline": [{"record": "us_a", "path": "p", "headline": 10, "audited_low": 8, "audited_high": 12}],
        }

    def write(self, report, date="2026-11-04"):
        d = tempfile.mkdtemp()
        p = os.path.join(d, "AP-probe.first.report.toml")
        gb_audit.write_report(p, report, date)
        with open(p, "rb") as f:
            raw = f.read()
        return p, raw

    def test_writes_valid_sorted_toml_with_given_date(self):
        _, raw = self.write(self.report())
        doc = tomllib.loads(raw.decode("utf-8"))
        self.assertEqual(doc["date"], "2026-11-04")
        keys = [(f["record"], f.get("path", ""), f["rule"]) for f in doc["findings"]]
        self.assertEqual(keys, sorted(keys))
        self.assertEqual(doc["findings"][1]["waiver_reason"], 'reason with "quotes" and a backslash \\')
        self.assertEqual([r["record"] for r in doc["reconciliation"]], ["us_a", "us_b"])
        self.assertEqual(doc["reconciliation"][0]["claims"][1]["figure"], 2)
        self.assertEqual(doc["agent"], {"role": "auditor", "session": "s-1"})
        self.assertEqual(doc["pass"], "first")

    def test_output_valid_against_schema(self):
        _, raw = self.write(self.report())
        self.assertEqual(gb_audit.check_report(tomllib.loads(raw.decode("utf-8"))), [])

    def test_output_is_byte_identical_and_independent_of_input_order(self):
        _, a = self.write(self.report())
        shuffled = self.report()
        shuffled["findings"].reverse()
        shuffled["reconciliation"].reverse()
        shuffled = {k: shuffled[k] for k in reversed(list(shuffled))}
        _, b = self.write(shuffled)
        self.assertEqual(a, b)
        self.assertTrue(a.endswith(b"\n"))
        self.assertNotIn(b"\r", a)

    def test_input_not_mutated(self):
        rep = self.report()
        before = copy.deepcopy(rep)
        self.write(rep)
        self.assertEqual(rep, before)

    def test_bad_reports_refused_and_nothing_written(self):
        cases = {}
        r = self.report(); r["pass"] = "second_blind"; r["agent"]["role"] = "blind_auditor"; r["saw_prior_values"] = True
        cases["blind pass that saw values"] = r
        r = self.report(); del r["findings"][1]["waiver_reason"]
        cases["waiver without reason"] = r
        r = self.report(); r["findings"][0]["rule"] = "XX10"
        cases["bad rule code"] = r
        r = self.report(); r["unexpected"] = 1
        cases["unknown key"] = r
        r = self.report(); r["counts"]["records"] = 1.5
        cases["float"] = r
        for name, rep in cases.items():
            d = tempfile.mkdtemp()
            p = os.path.join(d, "x.toml")
            with self.assertRaises(ValueError, msg=name):
                gb_audit.write_report(p, rep, "2026-11-04")
            self.assertFalse(os.path.exists(p), name)

    def test_bad_date_refused(self):
        for bad in ("04/11/2026", "2026-11-4", "", None):
            with self.assertRaises(ValueError, msg=repr(bad)):
                self.write(self.report(), bad)

    def test_unsupported_value_types_refused(self):
        with self.assertRaises(TypeError):
            gb_audit.toml_dumps({"a": 1.5})
        with self.assertRaises(TypeError):
            gb_audit.toml_dumps({"a": None})

    def test_emitter_round_trips_nested_shapes(self):
        doc = {"b": 1, "a": "x", "flag": True, "list": [1, 2, 3], "empty": [],
               "t": {"k": "v", "sub": {"n": 5}}, "arr": [{"i": 1, "inner": [{"z": 1}, {"z": 2}]}, {"i": 2}],
               "odd key": 3}
        self.assertEqual(tomllib.loads(gb_audit.toml_dumps(doc)), doc)

    def test_emitter_escapes_control_characters(self):
        doc = {"s": "tab\there\nnewline \u007f del é"}
        self.assertEqual(tomllib.loads(gb_audit.toml_dumps(doc)), doc)


class ModuleHygiene(unittest.TestCase):
    def test_no_wall_clock_or_unordered_iteration(self):
        with open(os.path.join(TOOLS, "gb_audit.py"), encoding="utf-8") as f:
            src = f.read()
        for pat in (r"\btime\.time\b", r"\bdatetime\b", r"\.now\(", r"\.today\(", r"\bimport time\b", r"\brandom\b"):
            self.assertIsNone(re.search(pat, src), pat)

    def test_module_imports_only_stdlib_and_jsonschema(self):
        with open(os.path.join(TOOLS, "gb_audit.py"), encoding="utf-8") as f:
            src = f.read()
        mods = set(re.findall(r"^(?:import|from)\s+([A-Za-z_][A-Za-z0-9_]*)", src, re.M))
        allowed = {"json", "os", "re", "sys", "tomllib", "typing", "jsonschema", "referencing", "functools", "__future__"}
        self.assertLessEqual(mods, allowed)


if __name__ == "__main__":
    unittest.main()
