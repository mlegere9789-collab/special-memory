"""Validator plugin for the SPEC-016 provenance, animation and audio manifests (ticket T-161, mechanism of T-027).

Checks, for each file that exists under the root: it parses as TOML; it validates against its closed JSON Schema
(unknown keys refused); it holds no float; every row that carries `confidence` passes the shared provenance
definition; and the semantic rules of tools/check_rights.py and tools/check_clips.py hold. For the skeleton it
checks 64 unique joints, a parent index below each child index, and a 28-joint tier-B subset (T16-13).
A file that is absent is skipped, so a root without these data returns no problems.
"""
import importlib.util
import json
import os
import sys
import tomllib

from jsonschema import Draft202012Validator
from referencing import Registry, Resource

TOOLS = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
FILES = {
    "assets": "data/provenance/assets.toml",
    "consents": "data/provenance/consents.toml",
    "clips": "data/animation/clips.toml",
    "recordings": "data/audio/recordings.toml",
    "skeleton": "data/animation/skeleton_soldier_v1.toml",
}
PROVENANCE_KEYS = ("confidence", "placeholder", "calibration_target", "sources", "second_source", "note")
JOINTS, TIER_B = 64, 28


def _tool(name):
    if TOOLS not in sys.path:
        sys.path.insert(0, TOOLS)
    spec = importlib.util.spec_from_file_location("hd_tool_" + name, os.path.join(TOOLS, name + ".py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def _floats(node, path=""):
    if isinstance(node, float):
        yield path or "(root)"
    elif isinstance(node, dict):
        for k, v in node.items():
            yield from _floats(v, path + "/" + k)
    elif isinstance(node, list):
        for i, v in enumerate(node):
            yield from _floats(v, "%s[%d]" % (path, i))


def check_skeleton(rec):
    joints = rec.get("joint", [])
    out = []
    names = [j.get("name") for j in joints]
    if len(joints) != JOINTS:
        out.append("skeleton has %d joints, expected %d" % (len(joints), JOINTS))
    if len(set(names)) != len(names):
        out.append("skeleton joint names are not unique")
    for i, j in enumerate(joints):
        p = j.get("parent", -1)
        if not (p == -1 and i == 0 or 0 <= p < i):
            out.append("joint %r (index %d): parent %r is not -1 for the root or an earlier index" % (j.get("name"), i, p))
    n_b = sum(1 for j in joints if j.get("tier_b"))
    if n_b != TIER_B:
        out.append("tier-B subset has %d joints, expected %d" % (n_b, TIER_B))
    by = {i: j for i, j in enumerate(joints)}
    for i, j in by.items():
        if j.get("tier_b") and j.get("parent", -1) >= 0 and not by[j["parent"]].get("tier_b"):
            out.append("tier-B joint %r has a parent outside the subset" % j.get("name"))
    return out


def validate(root):
    schema_dir = os.path.join(root, "data", "schemas")
    present = {k: os.path.join(root, rel) for k, rel in FILES.items() if os.path.exists(os.path.join(root, rel))}
    if not present:
        return []
    reg = Registry()
    for name in sorted(os.listdir(schema_dir)):
        if name.endswith(".schema.json"):
            with open(os.path.join(schema_dir, name), encoding="utf-8") as f:
                reg = reg.with_resource(name, Resource.from_contents(json.load(f)))
    prov = Draft202012Validator({"$ref": "rules-common.schema.json#/$defs/provenance"}, registry=reg)
    out, loaded = [], {}
    for kind, path in present.items():
        rel = FILES[kind]
        try:
            with open(path, "rb") as f:
                rec = tomllib.load(f)
            with open(os.path.join(schema_dir, kind + ".schema.json"), encoding="utf-8") as f:
                schema = json.load(f)
        except (tomllib.TOMLDecodeError, OSError, ValueError) as e:
            out.append("%s: cannot load: %s" % (rel, str(e)[:160]))
            continue
        loaded[kind] = rec
        for err in Draft202012Validator(schema, registry=reg).iter_errors(rec):
            out.append("%s: schema: %s at /%s" % (rel, err.message[:160], "/".join(map(str, err.absolute_path))))
        out += ["%s: float at %s (integers only)" % (rel, p) for p in _floats(rec)]
        for row_kind, rows in rec.items():
            for i, row in enumerate(rows if isinstance(rows, list) else []):
                if isinstance(row, dict) and "confidence" in row:
                    fields = {k: row[k] for k in PROVENANCE_KEYS if k in row}
                    out += ["%s: provenance at %s[%d]: %s" % (rel, row_kind, i, e.message[:160]) for e in prov.iter_errors(fields)]
    if "skeleton" in loaded:
        out += ["%s: %s" % (FILES["skeleton"], t) for t in check_skeleton(loaded["skeleton"])]
    if out:
        return out  # semantic rules assume well-formed rows
    rights, clips = _tool("check_rights"), _tool("check_clips")
    data = rights.load(root)
    out += ["check_rights: " + t for t in rights.check(data)]
    out += ["check_clips: " + t for t in clips.check(data["clip"], data["consent"])]
    return out
