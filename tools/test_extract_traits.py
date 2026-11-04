#!/usr/bin/env python3
"""Tests for extract_traits.py (SPEC-010 section 4, acceptance T10-01).
Run: python3 tools/test_extract_traits.py"""
import os
import sys
import tempfile
import tomllib
import unittest

TOOLS = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(TOOLS)
sys.path.insert(0, TOOLS)

import extract_traits  # noqa: E402


def record(rid, name, traits="", relationships=""):
    return ('id = "%s"\nname = "%s"\nside = "union"\n\n[[career]]\nrole = "Commander"\n\n[career.fact]\n'
            'value = "x"\nconfidence = "partial"\nsources = [{document = "CWV", page = 1}]\n%s%s'
            % (rid, name, traits, relationships))


def trait(name, conf="partial", page=7, tag="[V]"):
    return ('\n[[traits]]\ntrait = "%s"\nevidence = "An act, with \\"quotes\\"."\nconfidence = "%s"\n'
            'sources = [{document = "CWV", page = %d, tag = "%s"}]\n' % (name, conf, page, tag))


def relation(who, kind, conf="partial"):
    return ('\n[[relationships]]\nwith = "%s"\nkind = "%s"\nevidence = "A quarrel."\nconfidence = "%s"\n'
            'sources = [{document = "CWV", page = 9, tag = "untagged"}]\n' % (who, kind, conf))


def build(files):
    root = tempfile.mkdtemp()
    d = os.path.join(root, "data", "history", "commanders")
    os.makedirs(d)
    for name, text in files.items():
        with open(os.path.join(d, name + ".toml"), "w", encoding="utf-8") as f:
            f.write(text)
    return root


class Extraction(unittest.TestCase):
    def test_committed_file_equals_fresh_extraction(self):
        with open(os.path.join(REPO, "data", "rules", "ai_traits.toml"), encoding="utf-8") as f:
            self.assertEqual(f.read(), extract_traits.extract(REPO))

    def test_committed_rows_trace_to_records(self):
        rows = tomllib.loads(extract_traits.extract(REPO))
        self.assertGreater(len(rows["trait"]), 40)
        for row in rows["trait"]:
            self.assertIn(row["confidence"], ("verified", "partial", "unverified"))
            self.assertEqual(row["source_page"], row["sources"][0]["page"])
            self.assertTrue(row["note"])

    def test_sign_basis_matches_record(self):
        """Each ai.toml trait row with a non-zero delta cites the record act that fixes its sign."""
        with open(os.path.join(REPO, "data", "rules", "ai.toml"), "rb") as f:
            ai = tomllib.load(f)
        keys = ("risk", "reserve", "assault", "withdraw", "entrench", "discretion", "reaction")
        checked = 0
        for row in ai["trait"]:
            if not any(row[k] for k in keys):
                continue
            path = os.path.join(REPO, "data", "history", "commanders", row["act_from"] + ".toml")
            with open(path, "rb") as f:
                rec = tomllib.load(f)
            acts = [t for t in rec["traits"] if t["trait"] == row["id"]]
            self.assertTrue(acts, row["id"])
            self.assertEqual(row["sources"], acts[0]["sources"], row["id"])
            checked += 1
        self.assertEqual(checked, 8)

    def test_deterministic_and_sorted(self):
        root = build({"b_two": record("b_two", "B Two", trait("cautious")),
                      "a_one": record("a_one", "A One", trait("aggressive") + trait("engineer"))})
        text = extract_traits.extract(root)
        self.assertEqual(text, extract_traits.extract(root))
        rows = tomllib.loads(text)["trait"]
        self.assertEqual([(r["commander"], r["trait"]) for r in rows],
                         [("a_one", "aggressive"), ("a_one", "engineer"), ("b_two", "cautious")])

    def test_strings_survive_quoting(self):
        root = build({"a_one": record("a_one", "A One", trait("aggressive"))})
        row = tomllib.loads(extract_traits.extract(root))["trait"][0]
        self.assertEqual(row["note"], 'An act, with "quotes".')
        self.assertEqual(row["source_tag"], "[V]")

    def test_commander_without_traits_has_no_rows(self):
        root = build({"a_one": record("a_one", "A One")})
        self.assertNotIn("trait", tomllib.loads(extract_traits.extract(root)))

    def test_only_feud_and_rivalry_become_relationships(self):
        root = build({"a_one": record("a_one", "A One", relationships=(
            relation("B Two", "feud") + relation("C Three", "rivalry") + relation("D Four", "patronage")
            + relation("E Five", "subordinate_of"))),
            "b_two": record("b_two", "B Two")})
        rel = tomllib.loads(extract_traits.extract(root))["relationship"]
        self.assertEqual([(r["kind"], r["partner"], r["partner_name"]) for r in rel],
                         [("feud", "b_two", "B Two"), ("rivalry", "c_three", "C Three")])

    def test_partner_given_as_id_is_kept(self):
        root = build({"a_one": record("a_one", "A One", relationships=relation("b_two", "feud")),
                      "b_two": record("b_two", "B Two")})
        rel = tomllib.loads(extract_traits.extract(root))["relationship"][0]
        self.assertEqual(rel["partner"], "b_two")

    def test_verified_row_without_second_source_is_refused(self):
        root = build({"a_one": record("a_one", "A One", trait("aggressive", conf="verified"))})
        with self.assertRaises(SystemExit):
            extract_traits.extract(root)

    def test_check_mode(self):
        root = build({"a_one": record("a_one", "A One", trait("aggressive"))})
        out = os.path.join(root, "data", "rules")
        os.makedirs(out)
        self.assertEqual(extract_traits.main(["--root", root, "--check"]), 1)
        self.assertEqual(extract_traits.main(["--root", root]), 0)
        self.assertEqual(extract_traits.main(["--root", root, "--check"]), 0)


if __name__ == "__main__":
    unittest.main()
