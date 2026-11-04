#!/usr/bin/env python3
"""Extract battle-AI trait rows from the commander records (SPEC-010 section 4).

Reads data/history/commanders/*.toml (the only input) and writes data/rules/ai_traits.toml:
  [[trait]]         one row per commander trait: commander, trait, confidence, source_page, source_tag, sources, note
  [[relationship]]  one row per feud or rivalry: commander, kind, partner, partner_name, plus the same provenance
Rows are ordered by commander id, then record order. The output is deterministic.

Usage: python3 tools/extract_traits.py [--root DIR] [--check]
  --check   write nothing; exit 1 when the committed file differs from a fresh extraction
"""
import glob
import json
import os
import re
import sys
import tomllib

ROOT_DEFAULT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT_REL = os.path.join("data", "rules", "ai_traits.toml")
RELATION_KINDS = ("feud", "rivalry")
HEADER = ("# Battle-AI trait rows (SPEC-010 section 4), generated from data/history/commanders by\n"
          "# tools/extract_traits.py; regenerate rather than edit. Calibration of the trait deltas is in ai.toml.\n")


def q(text):
    """A TOML basic string (JSON string escapes are valid TOML)."""
    return json.dumps(text, ensure_ascii=False)


def slug(name):
    return re.sub(r"[^a-z0-9]+", "_", name.lower()).strip("_")


def provenance_lines(entry, who):
    if entry["confidence"] == "verified":
        sys.exit("%s: verified row needs second_source, which the commander record cannot carry" % who)
    first = entry["sources"][0]
    out = ["confidence = %s" % q(entry["confidence"]), "source_page = %d" % first["page"]]
    if "tag" in first:
        out.append("source_tag = %s" % q(first["tag"]))
    parts = []
    for s in entry["sources"]:
        item = "document = %s, page = %d" % (q(s["document"]), s["page"])
        if "tag" in s:
            item += ", tag = %s" % q(s["tag"])
        if "note" in s:
            item += ", note = %s" % q(s["note"])
        parts.append("{ %s }" % item)
    out.append("sources = [%s]" % ", ".join(parts))
    out.append("note = %s" % q(entry["evidence"]))
    return out


def extract(root):
    """Return the text of ai_traits.toml for the records under `root`."""
    records = []
    for path in sorted(glob.glob(os.path.join(root, "data", "history", "commanders", "*.toml"))):
        with open(path, "rb") as f:
            records.append(tomllib.load(f))
    records.sort(key=lambda r: r["id"])
    ids = {r["id"] for r in records}
    names = {r["name"]: r["id"] for r in records}

    def partner_id(text):
        return text if text in ids else names.get(text, slug(text))

    blocks = [HEADER]
    for rec in records:
        for t in rec.get("traits", []):
            lines = ["[[trait]]", "commander = %s" % q(rec["id"]), "trait = %s" % q(t["trait"])]
            lines += provenance_lines(t, "%s %s" % (rec["id"], t["trait"]))
            blocks.append("\n".join(lines) + "\n")
    for rec in records:
        for r in rec.get("relationships", []):
            if r["kind"] not in RELATION_KINDS:
                continue
            lines = ["[[relationship]]", "commander = %s" % q(rec["id"]), "kind = %s" % q(r["kind"]),
                     "partner = %s" % q(partner_id(r["with"])), "partner_name = %s" % q(r["with"])]
            lines += provenance_lines(r, "%s %s" % (rec["id"], r["kind"]))
            blocks.append("\n".join(lines) + "\n")
    return "\n".join(blocks)


def main(argv):
    root, check = ROOT_DEFAULT, False
    args = list(argv)
    while args:
        a = args.pop(0)
        if a == "--root":
            root = args.pop(0)
        elif a == "--check":
            check = True
        else:
            sys.exit("usage: extract_traits.py [--root DIR] [--check]")
    text = extract(root)
    out = os.path.join(root, OUT_REL)
    if check:
        current = ""
        if os.path.exists(out):
            with open(out, encoding="utf-8") as f:
                current = f.read()
        if current != text:
            print("%s differs from a fresh extraction; run tools/extract_traits.py" % OUT_REL)
            return 1
        return 0
    os.makedirs(os.path.dirname(out), exist_ok=True)
    with open(out, "w", encoding="utf-8", newline="\n") as f:
        f.write(text)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
