#!/usr/bin/env python3
"""Interface data checks (SPEC-015). Run: python3 tools/validate_ui.py [root]

Checks under the root:
  - data/ui/display-fields.toml against data/schemas/ui-display-fields.schema.json, plus: no name is
    listed twice and every field has a non-blank source (T15-02).
  - data/ui/{confidence,status,morale,symptoms}.toml against data/schemas/ui-params.schema.json (integers
    only, provenance on every row, calibration targets of SPEC-015), plus: the morale thresholds are
    ordered and the five levels differ in icon, word and symptom set.
  - crates/ui-model/tests/golden/snapshot.toml: the snapshot hash is re-derived here with an encoder
    written from the canonical encoding in crates/ui-model/src/hash.rs and must equal the recorded one.
tools/validators/ui.py registers `validate` as a plugin of tools/validate_data.py (rule R8).
"""
import json
import os
import struct
import sys
import tomllib

from jsonschema import Draft202012Validator
from referencing import Registry, Resource

FNV_OFFSET = 0xCBF29CE484222325
FNV_PRIME = 0x100000001B3


def fnv1a64(data):
    h = FNV_OFFSET
    for b in data:
        h = ((h ^ b) * FNV_PRIME) & 0xFFFFFFFFFFFFFFFF
    return h


def _load(root, rel):
    with open(os.path.join(root, rel), "rb") as f:
        return tomllib.load(f)


def check_display_fields(root):
    rel = "data/ui/display-fields.toml"
    if not os.path.exists(os.path.join(root, rel)):
        return []
    try:
        rec = _load(root, rel)
    except tomllib.TOMLDecodeError as e:
        return ["%s: not valid TOML: %s" % (rel, e)]
    with open(os.path.join(root, "data", "schemas", "ui-display-fields.schema.json"), encoding="utf-8") as f:
        schema = json.load(f)
    problems = []
    for err in Draft202012Validator(schema).iter_errors(rec):
        problems.append("%s: schema: %s at /%s" % (rel, err.message[:120], "/".join(map(str, err.absolute_path))))
    seen = set()
    for i, row in enumerate(rec.get("field", [])):
        name = row.get("name")
        if not str(row.get("source", "")).strip():
            problems.append("%s: field %s has no source" % (rel, name or "#%d" % i))
        if name in seen:
            problems.append("%s: field %s is listed twice" % (rel, name))
        seen.add(name)
    return problems


PARAM_FILES = ("confidence", "status", "morale", "symptoms")


def _registry(root):
    reg = Registry()
    sdir = os.path.join(root, "data", "schemas")
    for name in sorted(os.listdir(sdir)):
        if name.endswith(".schema.json"):
            with open(os.path.join(sdir, name), encoding="utf-8") as f:
                reg = reg.with_resource(name, Resource.from_contents(json.load(f)))
    return reg


def _level_problems(rel, levels):
    problems = []
    ids = sorted(row.get("id") for row in levels)
    if ids != [0, 1, 2, 3, 4]:
        problems.append("%s: levels 0 to 4 must each be listed once, found %s" % (rel, ids))
    for key, label in (("icon", "icon"), ("word", "word")):
        values = [row.get(key) for row in levels]
        if len(set(values)) != len(values):
            problems.append("%s: levels share a %s" % (rel, label))
    sets = [(row.get("fire_slowed"), row.get("looking_back"), row.get("ranks_thinning")) for row in levels]
    if len(set(sets)) != len(sets):
        problems.append("%s: levels share a symptom set" % rel)
    return problems


def check_params(root):
    """Schema, provenance and cross-field rules of the SPEC-015 parameter files that exist."""
    problems = []
    reg = None
    for name in PARAM_FILES:
        rel = "data/ui/%s.toml" % name
        if not os.path.exists(os.path.join(root, rel)):
            continue
        try:
            rec = _load(root, rel)
        except tomllib.TOMLDecodeError as e:
            problems.append("%s: not valid TOML: %s" % (rel, e))
            continue
        if reg is None:
            reg = _registry(root)
        branch = {"$ref": "ui-params.schema.json#/oneOf/%d" % PARAM_FILES.index(name)}
        errors = list(Draft202012Validator(branch, registry=reg).iter_errors(rec))
        for err in errors:
            problems.append("%s: schema: %s at /%s" % (rel, err.message[:120], "/".join(map(str, err.absolute_path))))
        if errors:
            continue
        params = rec.get("params", {})
        if name == "morale" and params.get("moderate_pm", 0) > params.get("large_pm", 0):
            problems.append("%s: moderate_pm must not exceed large_pm" % rel)
        if name == "symptoms":
            problems += _level_problems(rel, rec.get("level", []))
    return problems


def _opt(v):
    return b"\x00" if v < 0 else b"\x01" + struct.pack("<Q", v)


def _order(tag, arg, regiment):
    """The encode_order bytes of Hold (tag 1) or Form (tag 3)."""
    body = struct.pack("<BH", tag, regiment)
    return body + (bytes([arg]) if tag == 3 else b"")


def snapshot_bytes(g):
    out = struct.pack("<QIBB", g["tick"], g["clock_s"], g["side"], g["full_view"])
    regs = g.get("regiment", [])
    out += struct.pack("<I", len(regs))
    for r in regs:
        out += struct.pack("<HBqqIBIBBibI", r["id"], r["side"], r["pos_x"], r["pos_y"], r["heading"], r["formation"],
                           r["strength"], r["order"], r["level"], r["margin"], r["trend"], len(r["factors"]))
        for fid, c in r["factors"]:
            out += struct.pack("<Bi", fid, c)
    foes = g.get("enemy", [])
    out += struct.pack("<I", len(foes))
    for e in foes:
        out += struct.pack("<HQqqIIIB", e["unit"], e["last_seen"], e["pos_x"], e["pos_y"], e["err"], e["lo"], e["hi"],
                           e["formation_known"])
    orders = g.get("order", [])
    out += struct.pack("<I", len(orders))
    for o in orders:
        out += struct.pack("<I", o["id"]) + _order(o["tag"], o["arg"], o["regiment"]) + _opt(o["sent"])
        out += struct.pack("<QQQ", o["eta_lo"], o["eta_hi"], o["eta_hi0"]) + _opt(o["ack"])
        if o["interp_tag"]:
            out += b"\x01" + _order(o["interp_tag"], o["interp_arg"], o["regiment"])
        else:
            out += b"\x00"
        out += _opt(o["executed"]) + _opt(o["completed"]) + bytes([o["lost_known"]])
    return out


def check_snapshot_golden(root):
    rel = "crates/ui-model/tests/golden/snapshot.toml"
    if not os.path.exists(os.path.join(root, rel)):
        return []
    g = _load(root, rel)
    got = "%016x" % fnv1a64(snapshot_bytes(g))
    if got != g.get("hash"):
        return ["%s: recorded hash %s but the canonical encoding hashes to %s" % (rel, g.get("hash"), got)]
    return []


def validate(root):
    return check_display_fields(root) + check_params(root) + check_snapshot_golden(root)


def main():
    root = os.path.abspath(sys.argv[1]) if len(sys.argv) > 1 else os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    problems = validate(root)
    for p in problems:
        print(p)
    print("validate_ui: %d problems" % len(problems))
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
