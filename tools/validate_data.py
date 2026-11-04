#!/usr/bin/env python3
"""Validate every TOML record under data/history against its JSON Schema, plus provenance rules.

Rules beyond the schema:
  R1 a fact with confidence 'verified' must carry a non-empty 'second_source'
  R2 casualties: when killed, wounded and missing_captured are all present, they must sum to total
  R3 'low' <= value <= 'high' when a range is given
  R4 ids unique per kind and equal to the file name stem
  R5 start_date <= end_date and both are ISO dates
  R6 regression fixtures in qc/fixtures/*.toml: a listed (file, path) must NOT hold the listed bad value as verified
  R7 rules data: every data/rules/<name>.toml validates against data/schemas/<name>.schema.json, holds no float, and
     every table with a 'confidence' key passes the provenance definition in rules-common.schema.json
Exit 1 on any problem.
"""
import json
import os
import sys
import tomllib

import jsonschema
from jsonschema import Draft202012Validator
from referencing import Registry, Resource

ROOT = os.path.abspath(sys.argv[1]) if len(sys.argv) > 1 else os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SCHEMA_DIR = os.path.join(ROOT, "data", "schemas")
KINDS = {"battles": "battle", "commanders": "commander", "districts": "district", "politics": "politics"}


def load_registry():
    reg = Registry()
    for name in os.listdir(SCHEMA_DIR):
        if name.endswith(".schema.json"):
            with open(os.path.join(SCHEMA_DIR, name), encoding="utf-8") as f:
                doc = json.load(f)
            reg = reg.with_resource(name, Resource.from_contents(doc))
    return reg


PROVENANCE_KEYS = ("confidence", "placeholder", "calibration_target", "sources", "second_source", "note")


def walk_rows(node, path=""):
    """Yield (path, table) for every table that carries provenance fields (a 'confidence' key)."""
    if isinstance(node, dict):
        if "confidence" in node:
            yield path or "(root)", node
        for k, v in node.items():
            yield from walk_rows(v, path + "/" + k if path else k)
    elif isinstance(node, list):
        for i, v in enumerate(node):
            yield from walk_rows(v, "%s[%d]" % (path, i))


def find_floats(node, path=""):
    if isinstance(node, float):
        yield path or "(root)"
    elif isinstance(node, dict):
        for k, v in node.items():
            yield from find_floats(v, path + "/" + k if path else k)
    elif isinstance(node, list):
        for i, v in enumerate(node):
            yield from find_floats(v, "%s[%d]" % (path, i))


def check_rules(reg):
    """R7: validate data/rules/*.toml. Returns (files checked, problems found)."""
    d = os.path.join(ROOT, "data", "rules")
    if not os.path.isdir(d):
        return 0, 0
    prov = Draft202012Validator({"$ref": "rules-common.schema.json#/$defs/provenance"}, registry=reg)
    files = problems = 0
    for name in sorted(os.listdir(d)):
        if not name.endswith(".toml"):
            continue
        files += 1
        rel = os.path.relpath(os.path.join(d, name), ROOT)
        schema_path = os.path.join(SCHEMA_DIR, os.path.splitext(name)[0] + ".schema.json")
        try:
            with open(os.path.join(d, name), "rb") as f:
                rec = tomllib.load(f)
        except tomllib.TOMLDecodeError as e:
            print("%s: R7 not valid TOML: %s" % (rel, e))
            problems += 1
            continue
        if not os.path.exists(schema_path):
            print("%s: R7 no schema %s" % (rel, os.path.relpath(schema_path, ROOT)))
            problems += 1
        else:
            with open(schema_path, encoding="utf-8") as f:
                validator = Draft202012Validator(json.load(f), registry=reg)
            for err in validator.iter_errors(rec):
                print("%s: schema: %s at /%s" % (rel, err.message[:160], "/".join(map(str, err.absolute_path))))
                problems += 1
        for p in find_floats(rec):
            print("%s: R7 float at %s (rules data are integers only)" % (rel, p))
            problems += 1
        for p, row in walk_rows(rec):
            fields = {k: row[k] for k in PROVENANCE_KEYS if k in row}
            for err in prov.iter_errors(fields):
                print("%s: R7 provenance at %s: %s" % (rel, p, err.message[:160]))
                problems += 1
    return files, problems


def walk_facts(node, path=""):
    if isinstance(node, dict):
        if "confidence" in node and ("value" in node or "evidence" in node):
            yield path, node
        for k, v in node.items():
            yield from walk_facts(v, path + "/" + k)
    elif isinstance(node, list):
        for i, v in enumerate(node):
            yield from walk_facts(v, "%s/%d" % (path, i))


def main():
    reg = load_registry()
    problems = 0
    count = 0
    seen = {}
    for folder, kind in KINDS.items():
        d = os.path.join(ROOT, "data", "history", folder)
        if not os.path.isdir(d):
            continue
        with open(os.path.join(SCHEMA_DIR, kind + ".schema.json"), encoding="utf-8") as f:
            schema = json.load(f)
        validator = Draft202012Validator(schema, registry=reg)
        for name in sorted(os.listdir(d)):
            if not name.endswith(".toml"):
                continue
            path = os.path.join(d, name)
            rel = os.path.relpath(path, ROOT)
            count += 1
            with open(path, "rb") as f:
                rec = tomllib.load(f)
            for err in validator.iter_errors(rec):
                print("%s: schema: %s at /%s" % (rel, err.message[:160], "/".join(map(str, err.absolute_path))))
                problems += 1
            rid = rec.get("id")
            if rid != os.path.splitext(name)[0]:
                print("%s: R4 id %r does not match file name" % (rel, rid))
                problems += 1
            if (kind, rid) in seen:
                print("%s: R4 duplicate id %r" % (rel, rid))
                problems += 1
            seen[(kind, rid)] = rel
            for p, fact in walk_facts(rec):
                if fact.get("confidence") == "verified" and not str(fact.get("second_source", "")).strip():
                    print("%s: R1 %s is 'verified' without second_source" % (rel, p))
                    problems += 1
                if "value" in fact and isinstance(fact["value"], (int, float)):
                    lo, hi = fact.get("low"), fact.get("high")
                    if lo is not None and fact["value"] < lo or hi is not None and fact["value"] > hi:
                        print("%s: R3 %s value outside its range" % (rel, p))
                        problems += 1
            if kind == "battle":
                for side, sr in rec.get("sides", {}).items():
                    c = sr.get("casualties", {})
                    parts = [c.get(k, {}).get("value") for k in ("killed", "wounded", "missing_captured")]
                    tot = c.get("total", {}).get("value")
                    if None not in parts and tot is not None and sum(parts) != tot:
                        print("%s: R2 %s casualties %s sum to %s, total says %s" % (rel, side, parts, sum(parts), tot))
                        problems += 1
                s, e = rec.get("start_date", {}).get("value"), rec.get("end_date", {}).get("value")
                if s and e and not (len(s) == 10 and len(e) == 10 and s <= e):
                    print("%s: R5 dates %s..%s not ISO or out of order" % (rel, s, e))
                    problems += 1
    fx = os.path.join(ROOT, "qc", "fixtures")
    if os.path.isdir(fx):
        for name in sorted(os.listdir(fx)):
            if not name.endswith(".toml"):
                continue
            with open(os.path.join(fx, name), "rb") as f:
                fixture = tomllib.load(f)
            for item in fixture.get("bad_value", []):
                target = os.path.join(ROOT, item["file"])
                if not os.path.exists(target):
                    continue
                with open(target, "rb") as f:
                    rec = tomllib.load(f)
                node = rec
                try:
                    for part in item["path"].split("."):
                        node = node[int(part)] if isinstance(node, list) else node[part]
                except (KeyError, IndexError, ValueError):
                    continue
                if node.get("value") == item["bad_value"] and node.get("confidence") == "verified":
                    print("%s: R6 known-bad value %r held as verified at %s" % (item["file"], item["bad_value"], item["path"]))
                    problems += 1
    rule_files, rule_problems = check_rules(reg)
    count += rule_files
    problems += rule_problems
    print("validate_data: %d records, %d problems" % (count, problems))
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
