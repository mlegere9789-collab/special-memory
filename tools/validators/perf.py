"""Validator plugin for the performance data under qc/perf (SPEC-016 section 2; see example.py for the plugin contract).

Checks, against data/schemas/perf-*.schema.json: machine.toml, flights/F*.toml and thresholds.toml.
Beyond the schemas: no TOML float anywhere, unique ids, flight ids equal to file names, waypoint frames strictly
increasing from 0 and ending on frame `frames - 1` or `frames`, the provenance rules of rules-common.schema.json for every
threshold row, the row rules of qc/perf/summarise.py (so the summariser and the validator never disagree), and at least
one row for each of H16-1 to H16-11.
"""
import importlib.util
import json
import os
import tomllib

import jsonschema
from jsonschema import Draft202012Validator
from referencing import Registry, Resource

PROVENANCE_KEYS = ("confidence", "placeholder", "calibration_target", "sources", "second_source", "note")


def _registry(schema_dir):
    reg = Registry()
    for name in sorted(os.listdir(schema_dir)):
        if name.endswith(".schema.json"):
            with open(os.path.join(schema_dir, name), encoding="utf-8") as f:
                reg = reg.with_resource(name, Resource.from_contents(json.load(f)))
    return reg


def _floats(node, path=""):
    if isinstance(node, float):
        yield path or "(root)"
    elif isinstance(node, dict):
        for k, v in node.items():
            yield from _floats(v, path + "/" + k)
    elif isinstance(node, list):
        for i, v in enumerate(node):
            yield from _floats(v, "%s[%d]" % (path, i))


def _load(path, rel, out):
    try:
        with open(path, "rb") as f:
            return tomllib.load(f)
    except (tomllib.TOMLDecodeError, OSError) as e:
        out.append("%s: not readable TOML: %s" % (rel, str(e)[:120]))
        return None


def _summariser(root):
    path = os.path.join(root, "qc", "perf", "summarise.py")
    spec = importlib.util.spec_from_file_location("hd_perf_summarise", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def _check_file(path, rel, schema_name, reg, schema_dir, out):
    """Parse, schema-check and float-check one file; return the record or None."""
    rec = _load(path, rel, out)
    if rec is None:
        return None
    with open(os.path.join(schema_dir, schema_name), encoding="utf-8") as f:
        validator = Draft202012Validator(json.load(f), registry=reg)
    for err in validator.iter_errors(rec):
        out.append("%s: schema: %s at /%s" % (rel, err.message[:140], "/".join(map(str, err.absolute_path))))
    for p in _floats(rec):
        out.append("%s: float at %s (integers only)" % (rel, p))
    return rec


def validate(root):
    perf = os.path.join(root, "qc", "perf")
    if not os.path.isdir(perf):
        return []
    schema_dir = os.path.join(root, "data", "schemas")
    reg = _registry(schema_dir)
    out = []

    path = os.path.join(perf, "machine.toml")
    if os.path.isfile(path):
        rec = _check_file(path, "qc/perf/machine.toml", "perf-machine.schema.json", reg, schema_dir, out)
        ids = [m.get("id") for m in (rec or {}).get("machine", [])]
        for i in sorted({i for i in ids if ids.count(i) > 1}, key=str):
            out.append("qc/perf/machine.toml: duplicate machine id %r" % (i,))
        for m in (rec or {}).get("machine", []):
            d = m.get("date")
            if isinstance(d, int) and not (1 <= d // 100 % 100 <= 12 and 1 <= d % 100 <= 31):
                out.append("qc/perf/machine.toml: machine %r date %d is not YYYYMMDD" % (m.get("id"), d))

    fdir = os.path.join(perf, "flights")
    for name in sorted(os.listdir(fdir)) if os.path.isdir(fdir) else []:
        if not name.endswith(".toml"):
            continue
        rel = "qc/perf/flights/" + name
        rec = _check_file(os.path.join(fdir, name), rel, "perf-flight.schema.json", reg, schema_dir, out)
        if rec is None:
            continue
        if rec.get("id") != name[:-5]:
            out.append("%s: id %r does not match the file name" % (rel, rec.get("id")))
        frames = [w.get("frame") for w in rec.get("waypoint", []) if isinstance(w.get("frame"), int)]
        if frames and (frames[0] != 0 or any(b <= a for a, b in zip(frames, frames[1:]))):
            out.append("%s: waypoint frames must start at 0 and strictly increase" % rel)
        n = rec.get("frames")
        if frames and isinstance(n, int) and frames[-1] not in (n - 1, n):
            out.append("%s: last waypoint frame %d must be frames - 1 or frames" % (rel, frames[-1]))

    path = os.path.join(perf, "thresholds.toml")
    if os.path.isfile(path):
        rel = "qc/perf/thresholds.toml"
        rec = _check_file(path, rel, "perf-thresholds.schema.json", reg, schema_dir, out)
        rows = (rec or {}).get("threshold", [])
        prov = Draft202012Validator({"$ref": "rules-common.schema.json#/$defs/provenance"}, registry=reg)
        summ = _summariser(root) if rec is not None else None
        seen = set()
        for r in rows:
            rid = r.get("id", "?")
            if rid in seen:
                out.append("%s: duplicate row id %s" % (rel, rid))
            seen.add(rid)
            for err in prov.iter_errors({k: r[k] for k in PROVENANCE_KEYS if k in r}):
                out.append("%s: %s provenance: %s" % (rel, rid, err.message[:140]))
            for problem in summ.row_problems(r):
                out.append("%s: %s: %s" % (rel, rid, problem))
        if rec is not None:
            for n in range(1, 12):
                if not any(str(i).split("-")[:2] == ["H16", str(n)] for i in seen):
                    out.append("%s: no row for H16-%d" % (rel, n))
    return out
