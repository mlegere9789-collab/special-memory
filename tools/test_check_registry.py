#!/usr/bin/env python3
"""Tests for check_registry.py (acceptance TR-03). Run: python3 tools/test_check_registry.py

Fixtures live in qc/fixtures/registry/: `good/` is a complete mini repository root; each `bad/<case>/` holds only the
files that differ from it, so a test copies `good/` to a scratch root, lays one case over it and runs the checker.
"""
import os
import shutil
import subprocess
import sys
import tempfile
import unittest

TOOLS = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(TOOLS)
FIXTURES = os.path.join(REPO, "qc", "fixtures", "registry")
sys.path.insert(0, TOOLS)
import check_registry  # noqa: E402


def build_root(tmp, case=None):
    root = os.path.join(tmp, "root")
    shutil.copytree(os.path.join(FIXTURES, "good"), root)
    if case:
        shutil.copytree(os.path.join(FIXTURES, "bad", case), root, dirs_exist_ok=True)
    return root


def run_cli(root, *flags):
    p = subprocess.run([sys.executable, os.path.join(TOOLS, "check_registry.py"), *flags, root],
                       capture_output=True, text=True, check=False)
    return p.returncode, p.stdout


class Fixtures(unittest.TestCase):
    def check_case(self, case, *needles, ct=False):
        with tempfile.TemporaryDirectory() as tmp:
            root = build_root(tmp, case)
            problems = check_registry.check(root, ct=ct)
            self.assertTrue(problems, "case %s was accepted" % case)
            text = "\n".join(problems)
            for n in needles:
                self.assertIn(n, text)
            code, out = run_cli(root, *(["--ct"] if ct else []))
            self.assertEqual(code, 1, out)
            self.assertIn("%d problems" % len(problems), out)

    def test_good_fixture_passes(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = build_root(tmp)
            self.assertEqual(check_registry.check(root, ct=True), [])
            for flags in ((), ("--ct",)):
                code, out = run_cli(root, *flags)
                self.assertEqual(code, 0, out)
                self.assertIn("0 problems", out)

    def test_duplicate_namespace_value(self):
        self.check_case("dup-namespace-value", "duplicate", "0x02")

    def test_duplicate_ticket_id(self):
        self.check_case("dup-ticket-id", "ticket", "T-015", "TICKETS-004.md", "TICKETS-005.md")

    def test_unlisted_amendment_label(self):
        self.check_case("unlisted-amendment", "SPEC-004", "A7")

    def test_amendment_with_other_ticket_id(self):
        self.check_case("amendment-wrong-ticket", "SPEC-004", "A2", "T-019")

    def test_overlapping_stream_range(self):
        self.check_case("overlap-stream-range", "overlap", "0x0DF0")

    def test_reused_adr_number(self):
        self.check_case("reused-adr-number", "ADR", "0002")

    def test_bare_ct_identifier(self):
        self.check_case("bare-ct", "CT-1", "probe.toml", ct=True)

    def test_unregistered_ct_prefix(self):
        self.check_case("unregistered-ct-prefix", "CT-042-1", ct=True)

    def test_ct_scan_is_off_by_default(self):
        with tempfile.TemporaryDirectory() as tmp:
            self.assertEqual(check_registry.check(build_root(tmp, "bare-ct")), [])

    def test_missing_registry_is_a_problem(self):
        with tempfile.TemporaryDirectory() as tmp:
            problems = check_registry.check(tmp)
            self.assertEqual(len(problems), 1)
            self.assertIn("REGISTRY.md", problems[0])


class Parsing(unittest.TestCase):
    def test_tables_are_parsed_with_their_headings(self):
        text = "## 3. Streams\n\n| Id | Owner |\n|---|---|\n| `1` | a |\n| `2` | b |\n\ntext\n"
        tables = check_registry.parse_tables(text)
        self.assertEqual(len(tables), 1)
        self.assertEqual(tables[0].heading, "3. Streams")
        self.assertEqual(tables[0].header, ["Id", "Owner"])
        self.assertEqual(tables[0].rows, [["`1`", "a"], ["`2`", "b"]])

    def test_hex_intervals_in_a_cell(self):
        got = check_registry.hex_intervals("`0x01` Hold, `0x06` to `0x0F`, `0x0D00..0x0DFF`, `0x4000_0000 + slot`")
        self.assertEqual(got, [(1, 1), (6, 15), (0x0D00, 0x0DFF)])

    def test_adjacent_ranges_do_not_overlap(self):
        self.assertEqual(check_registry.overlaps([(0, 5), (6, 9)]), [])
        self.assertEqual(check_registry.overlaps([(0, 6), (6, 9)]), [((0, 6), (6, 9))])

    def test_adr_numbers_expand_ranges_and_skip_open_ends(self):
        self.assertEqual(check_registry.adr_numbers("0001 to 0003"), [1, 2, 3])
        self.assertEqual(check_registry.adr_numbers("0010 and above"), [])
        self.assertEqual(check_registry.adr_numbers("0005"), [5])


if __name__ == "__main__":
    unittest.main()
