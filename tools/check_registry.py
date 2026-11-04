#!/usr/bin/env python3
"""Registry checker (acceptance TR-03). Stdlib only. Run: python3 tools/check_registry.py [--ct] [root]

Reads docs/specs/REGISTRY.md and reports, one line per problem:
  - a namespace table (first column) holding the same value twice, or hex values/ranges that overlap in the actor,
    tag and stream tables, or an ADR number listed twice (in REGISTRY section 17 or by two files in docs/adr/)
  - a docs/specs/SPEC-NNN-*.md that declares an amendment label REGISTRY section 9 does not list for that spec,
    declares one twice, or names a ticket id that differs from the one the registry gives for that label
  - a ticket id that appears twice in the first column of docs/specs/TICKETS-*.md
  - with --ct: a calibration target id in data/ or qc/ that is not `CT-<registered three-digit spec>-<n>`
    (off by default until the rules data have migrated to that form, ticket T-025)
Exit 1 on any problem.
"""
import collections
import glob
import os
import re
import sys

ROOT_DEFAULT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
HEX_TABLES = {"Actor", "Tag", "Stream id"}
CT_SCAN_DIRS = ("data", "qc")
CT_SCAN_EXT = {".toml", ".json", ".md", ".csv", ".txt"}
CT_SKIP = ("qc/fixtures/registry",)
Table = collections.namedtuple("Table", "heading header rows")

_SEP = re.compile(r"^\|[\s:|-]+\|\s*$")
_HEX = re.compile(r"`(0x[0-9A-Fa-f_]+)(?:\.\.(0x[0-9A-Fa-f_]+))?`(?:\s+to\s+`(0x[0-9A-Fa-f_]+)`)?")
_ADR = re.compile(r"\b(\d{4})\b(?:\s+(to)\s+(\d{4})|\s+(and above))?")
_AMEND = re.compile(r"^(?:#{1,6}\s*|\*\*)Amendment (A\d+)\b(.*)$")
_TICKET_ROW = re.compile(r"^\|\s*(T-\d{3}[a-z]?)\s*\|")
_TICKET_ID = re.compile(r"\bT-\d{3}[a-z]?\b")
_CT_ANY = re.compile(r"\bCT-[0-9A-Za-z][0-9A-Za-z-]*")
_CT_OK = re.compile(r"CT-([0-9]{3})-[0-9]+$")


def parse_tables(text):
    """Return the pipe tables of a markdown text, each with the heading above it."""
    lines = text.splitlines()
    tables, heading, i = [], "", 0
    while i < len(lines):
        line = lines[i]
        if line.startswith("#"):
            heading = line.lstrip("#").strip()
        elif line.startswith("|") and i + 1 < len(lines) and _SEP.match(lines[i + 1]):
            header = _cells(line)
            rows, i = [], i + 2
            while i < len(lines) and lines[i].startswith("|"):
                rows.append(_cells(lines[i]))
                i += 1
            tables.append(Table(heading, header, rows))
            continue
        i += 1
    return tables


def _cells(line):
    return [c.strip() for c in line.strip().strip("|").split("|")]


def tokens(cell):
    """The backticked values of a cell, or the whole cell text when it has none."""
    found = re.findall(r"`([^`]+)`", cell)
    return found or ([cell] if cell else [])


def hex_intervals(cell):
    """(low, high) for every plain hex value or range written in backticks; `0x.. + offset` forms are skipped."""
    out = []
    for m in _HEX.finditer(cell):
        lo = int(m.group(1).replace("_", ""), 16)
        end = m.group(2) or m.group(3)
        out.append((lo, int(end.replace("_", ""), 16) if end else lo))
    return out


def overlaps(intervals):
    """Pairs of (low, high) intervals that share a value; touching ends count, adjacent ranges do not."""
    ordered = sorted(intervals)
    return [(a, b) for i, a in enumerate(ordered) for b in ordered[i + 1:] if b[0] <= a[1]]


def adr_numbers(cell):
    """ADR numbers named by a cell; `0010 and above` names none (it is the free pool)."""
    out = []
    for m in _ADR.finditer(cell):
        if m.group(4):
            continue
        lo = int(m.group(1))
        out.extend(range(lo, int(m.group(3)) + 1) if m.group(2) else [lo])
    return out


def _read(path):
    with open(path, encoding="utf-8") as f:
        return f.read()


def check_namespaces(tables):
    problems = []
    for t in tables:
        if not t.header:
            continue
        where = "REGISTRY '%s'" % t.heading
        if t.header[0] == "ADR":
            seen = {}
            for row in t.rows:
                for n in adr_numbers(row[0]):
                    if n in seen:
                        problems.append("%s: ADR %04d is listed twice" % (where, n))
                    seen[n] = True
            continue
        seen = set()
        for row in t.rows:
            for tok in tokens(row[0]) if row else []:
                if tok in seen:
                    problems.append("%s: duplicate value %s in column '%s'" % (where, tok, t.header[0]))
                seen.add(tok)
        if "CT prefix" in t.header:
            col, seen = t.header.index("CT prefix"), set()
            for row in t.rows:
                v = row[col] if col < len(row) else ""
                if v and v != "none" and v in seen:
                    problems.append("%s: duplicate value %s in column 'CT prefix'" % (where, v))
                seen.add(v)
        if t.header[0] in HEX_TABLES:
            items = [iv for row in t.rows for iv in hex_intervals(row[0])]
            for a, b in overlaps(items):
                problems.append("%s: overlap between 0x%04X..0x%04X and 0x%04X..0x%04X" % (where, a[0], a[1], b[0], b[1]))
    return problems


def registered_amendments(text, tables):
    """{spec number: {label: ticket cell}} from REGISTRY section 9, including the 'Reserved:' labels."""
    out = collections.defaultdict(dict)
    for t in tables:
        m = re.match(r"9\.\d+\s+SPEC-(\d{3})", t.heading)
        if not m or "Label" not in t.header:
            continue
        tcol = t.header.index("Ticket") if "Ticket" in t.header else None
        for row in t.rows:
            out[m.group(1)][row[0]] = row[tcol] if tcol is not None and tcol < len(row) else ""
    for line in text.splitlines():
        if line.startswith("Reserved:"):
            for spec, label in re.findall(r"SPEC-(\d{3}) (A\d+)", line):
                out[spec].setdefault(label, "")
    return out


def check_amendments(root, registry):
    problems = []
    for path in sorted(glob.glob(os.path.join(root, "docs", "specs", "SPEC-[0-9][0-9][0-9]-*.md"))):
        name = os.path.basename(path)
        spec = name[5:8]
        declared = set()
        for line in _read(path).splitlines():
            m = _AMEND.match(line)
            if not m or re.match(r"\s+to SPEC-\d{3}", m.group(2)):
                continue
            label = m.group(1)
            if label in declared:
                problems.append("%s: amendment %s is declared twice" % (name, label))
            declared.add(label)
            listed = registry.get(spec, {})
            if label not in listed:
                problems.append("%s: SPEC-%s amendment %s is not listed in REGISTRY section 9" % (name, spec, label))
                continue
            ids, cell = _TICKET_ID.findall(m.group(2)), listed[label]
            for tid in ids:
                if cell and tid not in _TICKET_ID.findall(cell):
                    problems.append("%s: SPEC-%s %s names %s but REGISTRY section 9 lists ticket %s"
                                    % (name, spec, label, tid, cell))
    return problems


def check_tickets(root):
    problems, first = [], {}
    for path in sorted(glob.glob(os.path.join(root, "docs", "specs", "TICKETS-*.md"))):
        name = os.path.basename(path)
        for n, line in enumerate(_read(path).splitlines(), 1):
            m = _TICKET_ROW.match(line)
            if not m:
                continue
            if m.group(1) in first:
                problems.append("ticket id %s appears twice: %s and %s:%d" % (m.group(1), first[m.group(1)], name, n))
            else:
                first[m.group(1)] = "%s:%d" % (name, n)
    return problems


def check_adr_files(root):
    problems, seen = [], {}
    for path in sorted(glob.glob(os.path.join(root, "docs", "adr", "[0-9][0-9][0-9][0-9]-*.md"))):
        name = os.path.basename(path)
        if name[:4] in seen:
            problems.append("ADR %s is used by two files: %s and %s" % (name[:4], seen[name[:4]], name))
        seen[name[:4]] = name
    return problems


def ct_prefixes(tables):
    out = set()
    for t in tables:
        if "CT prefix" in t.header:
            col = t.header.index("CT prefix")
            for row in t.rows:
                out.update(re.findall(r"CT-([0-9]{3})-n", row[col] if col < len(row) else ""))
    return out


def check_ct_ids(root, prefixes):
    problems = []
    for top in CT_SCAN_DIRS:
        for dp, dn, fn in os.walk(os.path.join(root, top)):
            dn[:] = sorted(d for d in dn if d not in {"target", "__pycache__"})
            for name in sorted(fn):
                path = os.path.join(dp, name)
                rel = os.path.relpath(path, root).replace(os.sep, "/")
                if os.path.splitext(name)[1] not in CT_SCAN_EXT or rel.startswith(CT_SKIP):
                    continue
                for n, line in enumerate(_read(path).splitlines(), 1):
                    for tok in _CT_ANY.findall(line):
                        m = _CT_OK.match(tok)
                        if not m:
                            problems.append("%s:%d: %s is not of the form CT-<spec>-<n>" % (rel, n, tok))
                        elif prefixes and m.group(1) not in prefixes:
                            problems.append("%s:%d: %s has no registered spec prefix" % (rel, n, tok))
    return problems


def check(root, ct=False):
    """Return the list of problems found under `root` (empty when the registry and its users agree)."""
    path = os.path.join(root, "docs", "specs", "REGISTRY.md")
    if not os.path.isfile(path):
        return ["docs/specs/REGISTRY.md not found under %s" % root]
    text = _read(path)
    tables = parse_tables(text)
    problems = (check_namespaces(tables) + check_amendments(root, registered_amendments(text, tables))
                + check_tickets(root) + check_adr_files(root))
    return problems + (check_ct_ids(root, ct_prefixes(tables)) if ct else [])


def main(argv):
    args = [a for a in argv[1:] if a != "--ct"]
    root = os.path.abspath(args[0]) if args else ROOT_DEFAULT
    problems = check(root, ct="--ct" in argv[1:])
    for p in problems:
        print(p)
    print("check_registry: %d problems" % len(problems))
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
