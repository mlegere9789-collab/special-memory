#!/usr/bin/env python3
"""Tests for tools/validators/survey.py (T14-15: the sources.toml rules, on small fixtures and on the real file).
Run: python3 tools/test_survey.py"""
import importlib.util
import json
import os
import subprocess
import sys
import tomllib
import tempfile
import unittest

TOOLS = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(TOOLS)
spec = importlib.util.spec_from_file_location("survey_plugin", os.path.join(TOOLS, "validators", "survey.py"))
survey = importlib.util.module_from_spec(spec)
spec.loader.exec_module(survey)
with open(os.path.join(REPO, "data", "schemas", "survey-sources.schema.json"), encoding="utf-8") as f:
    SCHEMA = json.load(f)

H = "ab" * 32
HEAD = 'schema = "survey-sources/1"\nsurvey = "probe"\n'
BASE = 'id = {id}\nkind = "dem"\nprovider = "p"\nproduct = "q"\n'
UNCONSULTED = BASE + 'consulted = false\ncoverage_status = "unchecked"\nlicence_status = "unverified"\n'
CLEARED = (BASE + 'consulted = true\ncoverage_status = "passed"\nlicence_status = "cleared"\nacquired = "2019-05-01"\n'
           'retrieved = "2026-01-02"\ndata_sha256 = "%s"\nindex_query_sha256 = "%s"\nlicence_basis = "federal work"\n'
           'licence_evidence_sha256 = "%s"\ncleared_by = "reviewer_1"\nvertical_rmse_mm = 100\n' % (H, H, H))


def write(path, text):
    with open(path, "w", encoding="utf-8") as f:
        f.write(text)


def manifest(*blocks):
    return HEAD + "".join("\n[[source]]\n" + b.format(id=i + 1) for i, b in enumerate(blocks))


def run(text, strict=False):
    with tempfile.TemporaryDirectory() as d:
        p = os.path.join(d, "sources.toml")
        write(p, text)
        return survey.check_sources(p, SCHEMA, strict=strict, rel="sources.toml")


class Fixtures(unittest.TestCase):
    def test_unconsulted_sources_pass_and_are_pending(self):
        text = manifest(UNCONSULTED, UNCONSULTED)
        self.assertEqual(run(text), [])
        with tempfile.TemporaryDirectory() as d:
            p = os.path.join(d, "sources.toml")
            write(p, text)
            items = survey.pending_sources(p, "s.toml")
        self.assertEqual(len([t for t in items if "not consulted" in t]), 2)
        self.assertEqual(len([t for t in items if "licence unverified" in t]), 2)

    def test_strict_fails_every_uncleared_source(self):
        self.assertEqual(len(run(manifest(UNCONSULTED, CLEARED), strict=True)), 1)
        self.assertEqual(run(manifest(CLEARED), strict=True), [])

    def test_cleared_source_with_evidence_passes(self):
        self.assertEqual(run(manifest(CLEARED)), [])

    def test_unconsulted_source_cannot_claim_anything(self):
        for extra in ('licence_status = "cleared"\n', 'cleared_by = "x"\n', 'data_sha256 = "%s"\n' % H, 'retrieved = "2026-01-01"\n',
                      'vertical_rmse_mm = 5\n'):
            base = UNCONSULTED.replace('licence_status = "unverified"\n', "") if extra.startswith("licence_status") else UNCONSULTED
            self.assertTrue(run(manifest(base + extra)), extra)
        self.assertTrue(run(manifest(UNCONSULTED.replace("unchecked", "passed"))))

    def test_cleared_needs_each_piece_of_evidence(self):
        for key in ("data_sha256", "index_query_sha256", "licence_basis", "licence_evidence_sha256", "cleared_by", "retrieved", "acquired"):
            lines = [ln for ln in CLEARED.splitlines(True) if not ln.startswith(key + " ")]
            errs = run(manifest("".join(lines)))
            self.assertTrue(any(key in e for e in errs), (key, errs))
        self.assertTrue(run(manifest(CLEARED.replace("passed", "unchecked"))))
        self.assertTrue(run(manifest(CLEARED.replace("consulted = true", "consulted = false"))))

    def test_structure_rules(self):
        self.assertTrue(any("duplicate" in e for e in run(HEAD + "\n[[source]]\n" + UNCONSULTED.format(id=1) + "\n[[source]]\n" + UNCONSULTED.format(id=1))))
        self.assertTrue(run(HEAD + "\n[[source]]\n" + UNCONSULTED.format(id=0)))
        self.assertTrue(run(HEAD + "\n[[source]]\n" + UNCONSULTED.format(id=256)))
        self.assertTrue(run(manifest(UNCONSULTED + "extra = 1\n")))
        self.assertTrue(run(manifest(UNCONSULTED.replace("unverified", "maybe"))))
        self.assertTrue(run(manifest(UNCONSULTED + "foo = 1.5\n")))
        self.assertTrue(run(manifest(CLEARED.replace(H, "xyz", 1))))
        self.assertTrue(run("schema = [\n"))
        self.assertTrue(run(HEAD))

    def test_map_sources_need_control_points(self):
        m = 'id = 7\nkind = "map"\nprovider = "p"\nproduct = "q"\nconsulted = true\ncoverage_status = "unchecked"\nlicence_status = "pending"\n'
        pt = '{ label = "a", map_x_mm = 1, map_y_mm = 2, easting_mm = 3, northing_mm = 4 }'
        self.assertTrue(run(HEAD + "\n[[source]]\n" + m + "georef_rms_mm = 5\n"))
        self.assertEqual(run(HEAD + "\n[[source]]\n" + m + "georef_rms_mm = 5\ncontrol = [%s]\n" % ", ".join([pt] * 3)), [])
        self.assertEqual(run(HEAD + "\n[[source]]\n" + m.replace("id = 7", "id = 300")), [])


class Obstacles(unittest.TestCase):
    def test_ids_ascending_and_names_unique(self):
        with tempfile.TemporaryDirectory() as d:
            p = os.path.join(d, "obstacles.toml")
            for body, bad in (("[[kind]]\nid = 1\nname = \"a\"\n[[kind]]\nid = 2\nname = \"b\"\n", False),
                              ("[[kind]]\nid = 2\nname = \"a\"\n[[kind]]\nid = 2\nname = \"b\"\n", True),
                              ("[[kind]]\nid = 1\nname = \"a\"\n[[kind]]\nid = 2\nname = \"a\"\n", True)):
                write(p, body)
                self.assertEqual(bool(survey.check_obstacles(p)), bad)


class RealFiles(unittest.TestCase):
    def test_shipped_manifest_has_no_failures_and_nothing_claimed(self):
        self.assertEqual(survey.validate(REPO), [])
        path = os.path.join(REPO, "data", "survey", "gettysburg", "sources.toml")
        with open(path, "rb") as f:
            sources = tomllib.load(f)["source"]
        self.assertTrue(sources)
        for s in sources:
            self.assertIs(s["consulted"], False)
            self.assertEqual(s["licence_status"], "unverified")
            self.assertEqual(s["coverage_status"], "unchecked")
            for k in survey.EVIDENCE:
                self.assertNotIn(k, s)
        # Open items are reported as pending, and strict mode (the rule of hd-terrain check) refuses the manifest.
        self.assertGreaterEqual(len(survey.pending(REPO)), 4 * len(sources))
        self.assertEqual(len(survey.check_sources(path, SCHEMA, strict=True)), len(sources))

    def test_command_line_prints_pending_and_exits_zero(self):
        r = subprocess.run([sys.executable, "-I", os.path.join(TOOLS, "validators", "survey.py"), REPO], capture_output=True, text=True)
        self.assertEqual(r.returncode, 0)
        self.assertIn("pending:", r.stdout)

    def test_shipped_obstacles(self):
        self.assertEqual(survey.check_obstacles(os.path.join(REPO, "data", "rules", "obstacles.toml")), [])


if __name__ == "__main__":
    unittest.main()
