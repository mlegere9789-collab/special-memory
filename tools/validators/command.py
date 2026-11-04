"""Validator plugin for the SPEC-007 data (see example.py for how plugins work).

Checks, under the root it is given:
  - data/rules/command.toml: the cross-row rules the schema cannot express (each medium, level and trait exactly
    once, a courier with a speed, every calibration target of spec 007). The schema itself is checked by rule R7.
  - data/scenarios/<family>/officers.toml: the schema officers.schema.json, then the structure the Rust loader
    checks (ascending ids, parents and deputies exist, no cycle, level equals parent level + 1, same side, one army
    per side, one owner per regiment, links inside one side and not repeated).
"""
import json
import os
import re
import tomllib

from jsonschema import Draft202012Validator

MEDIA = {"courier", "flags", "telegraph"}
TRAITS = {"aggressive", "cautious", "high_discretion", "low_discretion"}


def _load(path):
    with open(path, "rb") as f:
        return tomllib.load(f)


def _exactly_once(rows, key, want, what, rel):
    seen = [r.get(key) for r in rows]
    problems = []
    for v in sorted(set(seen) - set(want), key=str):
        problems.append("%s: %s %r is not a known %s" % (rel, key, v, what))
    for v in sorted(set(want), key=str):
        if seen.count(v) != 1:
            problems.append("%s: %s %r appears %d times, expected once" % (rel, what, v, seen.count(v)))
    return problems


def check_table(root):
    path = os.path.join(root, "data", "rules", "command.toml")
    if not os.path.exists(path):
        return []
    rel = "data/rules/command.toml"
    rec = _load(path)
    problems = []
    problems += _exactly_once(rec.get("medium", []), "id", MEDIA, "medium", rel)
    problems += _exactly_once(rec.get("level", []), "level", range(5), "level", rel)
    problems += _exactly_once(rec.get("trait", []), "trait", TRAITS, "trait", rel)
    for r in rec.get("medium", []):
        if r.get("id") == "courier" and r.get("speed_mm_s", 0) <= 0:
            problems.append("%s: the courier row needs speed_mm_s above zero" % rel)
    rows = rec.get("medium", []) + rec.get("level", []) + rec.get("trait", []) + [rec.get("globals", {})]
    for r in rows:
        target = r.get("calibration_target")
        if r.get("placeholder") and target is not None and not re.match(r"^CT-007-[0-9]+$", target):
            problems.append("%s: calibration target %s does not belong to spec 007" % (rel, target))
    return problems


def check_officers(rec, rel):
    """Structure of one parsed officers file; returns problem texts."""
    problems = []
    officers = rec.get("officer", [])
    ids = [o.get("id") for o in officers]
    if any(b <= a for a, b in zip(ids, ids[1:])):
        problems.append("%s: officer ids must be unique and ascending" % rel)
    by_id = {o["id"]: o for o in officers if "id" in o}
    for o in officers:
        for key in ("parent", "deputy"):
            if key in o and o[key] not in by_id:
                problems.append("%s: officer %s: %s %s does not exist" % (rel, o["id"], key, o[key]))
        if o.get("deputy") == o["id"]:
            problems.append("%s: officer %s is his own deputy" % (rel, o["id"]))
    if problems:
        return problems
    for o in officers:
        cur, steps = o, 0
        while "parent" in cur:
            cur = by_id[cur["parent"]]
            steps += 1
            if steps > len(officers):
                problems.append("%s: cycle in the parent chain of officer %s" % (rel, o["id"]))
                return problems
    armies = {}
    for o in officers:
        if o["level"] == 0:
            if o["side"] in armies:
                problems.append("%s: side %s has two armies (officers %s and %s)" % (rel, o["side"], armies[o["side"]], o["id"]))
            armies[o["side"]] = o["id"]
    for o in officers:
        p = by_id.get(o.get("parent"))
        if p is not None:
            if o["level"] != p["level"] + 1:
                problems.append("%s: officer %s: level is not parent level + 1" % (rel, o["id"]))
            if o["side"] != p["side"]:
                problems.append("%s: officer %s: parent is on the other side" % (rel, o["id"]))
        d = by_id.get(o.get("deputy"))
        if d is not None and d["side"] != o["side"]:
            problems.append("%s: officer %s: deputy is on the other side" % (rel, o["id"]))
    owners = {}
    for o in officers:
        if "regiment" in o:
            if o["regiment"] in owners:
                problems.append("%s: regiment %s owned twice (officers %s and %s)" % (rel, o["regiment"], owners[o["regiment"]], o["id"]))
            owners[o["regiment"]] = o["id"]
    seen = set()
    for link in rec.get("link", []):
        a, b = link["a"], link["b"]
        if a not in by_id or b not in by_id:
            problems.append("%s: link %s-%s names an officer that does not exist" % (rel, a, b))
        elif a == b:
            problems.append("%s: link %s-%s joins one officer to himself" % (rel, a, b))
        elif by_id[a]["side"] != by_id[b]["side"]:
            problems.append("%s: link %s-%s crosses sides" % (rel, a, b))
        key = (link["medium"], min(a, b), max(a, b))
        if key in seen:
            problems.append("%s: link %s repeated" % (rel, key))
        seen.add(key)
    return problems


def validate(root):
    problems = check_table(root)
    base = os.path.join(root, "data", "scenarios")
    schema_path = os.path.join(root, "data", "schemas", "officers.schema.json")
    if not os.path.isdir(base):
        return problems
    with open(schema_path, encoding="utf-8") as f:
        validator = Draft202012Validator(json.load(f))
    for family in sorted(os.listdir(base)):
        path = os.path.join(base, family, "officers.toml")
        if not os.path.isfile(path):
            continue
        rel = "data/scenarios/%s/officers.toml" % family
        try:
            rec = _load(path)
        except tomllib.TOMLDecodeError as e:
            problems.append("%s: not valid TOML: %s" % (rel, e))
            continue
        schema_problems = ["%s: schema: %s at /%s" % (rel, err.message[:160], "/".join(map(str, err.absolute_path)))
                           for err in validator.iter_errors(rec)]
        problems += schema_problems
        if not schema_problems:
            problems += check_officers(rec, rel)
    return problems
