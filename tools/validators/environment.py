"""Validator plugin for the SPEC-009 data (see example.py for how plugins work).

Checks, under the root it is given:
  - data/rules/environment.toml: the rules the schema cannot express (one retain row per terrain class of
    terrain.toml and no other, ascending; heat curve strictly increasing in apparent_f10, drain_pm non-decreasing,
    rest_pm non-increasing; the smoke flow sum; every calibration target of spec 009). The schema itself is checked by
    rule R7 of validate_data.py.
  - data/scenarios/<family>/relief.toml: the schema relief.schema.json, no float, provenance, equal row lengths,
    heights within 10000 decimetres.
  - data/scenarios/<family>/weather.toml: the schema weather.schema.json, no float, provenance, t_min strictly
    ascending, wind speed at most 20000 mm/s, calibration targets of spec 009, and every 880 key citing CWV page 79.
"""
import json
import math
import os
import re
import tomllib

from jsonschema import Draft202012Validator
from referencing import Registry, Resource

WIND_MAX_MM_S = 20000
TICK_HZ = 20
RELIEF_MAX_DM = 10000
PROVENANCE_KEYS = ("confidence", "placeholder", "calibration_target", "sources", "second_source", "note")


def _load(path):
    with open(path, "rb") as f:
        return tomllib.load(f)


def _registry(root):
    reg = Registry()
    d = os.path.join(root, "data", "schemas")
    for name in sorted(os.listdir(d)):
        if name.endswith(".schema.json"):
            with open(os.path.join(d, name), encoding="utf-8") as f:
                reg = reg.with_resource(name, Resource.from_contents(json.load(f)))
    return reg


def _floats(node, path=""):
    if isinstance(node, float):
        yield path or "(root)"
    elif isinstance(node, dict):
        for k, v in node.items():
            yield from _floats(v, path + "/" + k if path else k)
    elif isinstance(node, list):
        for i, v in enumerate(node):
            yield from _floats(v, "%s[%d]" % (path, i))


def _provenance(rows, prov, rel, what):
    problems = []
    for i, row in enumerate(rows):
        fields = {k: row[k] for k in PROVENANCE_KEYS if k in row}
        for err in prov.iter_errors(fields):
            problems.append("%s: %s %d provenance: %s" % (rel, what, i, err.message[:120]))
        target = row.get("calibration_target")
        if target is not None and not re.match(r"^CT-009-[0-9]+$", target):
            problems.append("%s: %s %d: calibration target %s does not belong to spec 009" % (rel, what, i, target))
    return problems


def check_environment(rec, terrain_ids, prov, rel="data/rules/environment.toml"):
    """Cross-row rules of one parsed environment file; returns problem texts."""
    problems = []
    classes = [r.get("class") for r in rec.get("retain", [])]
    if any(b <= a for a, b in zip(classes, classes[1:])):
        problems.append("%s: retain classes must be unique and ascending" % rel)
    if terrain_ids is not None:
        for c in sorted(set(classes) - set(terrain_ids)):
            problems.append("%s: retain class %s is not in terrain.toml" % (rel, c))
        for c in sorted(set(terrain_ids) - set(classes)):
            problems.append("%s: terrain class %s has no retain row" % (rel, c))
    curve = rec.get("heat_curve", [])
    for a, b in zip(curve, curve[1:]):
        if b["apparent_f10"] <= a["apparent_f10"]:
            problems.append("%s: heat_curve apparent_f10 must be strictly increasing" % rel)
        if b["drain_pm"] < a["drain_pm"]:
            problems.append("%s: heat_curve drain_pm must be non-decreasing" % rel)
        if b["rest_pm"] > a["rest_pm"]:
            problems.append("%s: heat_curve rest_pm must be non-increasing" % rel)
    smoke = rec.get("smoke", {})
    if "period_ticks" in smoke and "cell_mm" in smoke and "diffuse_ppm" in smoke:
        cells = WIND_MAX_MM_S * smoke["period_ticks"] * 10**6 // (TICK_HZ * smoke["cell_mm"])
        flow = 4 * smoke["diffuse_ppm"] + -(-cells * 1415 // 1000)
        if flow > 10**6:
            problems.append("%s: smoke flow sum %d ppm exceeds 1000000" % (rel, flow))
    tables = [rec.get(k, {}) for k in ("smoke", "sight", "heat", "rain")]
    problems += _provenance(tables + rec.get("retain", []) + curve, prov, rel, "row")
    return problems


def check_weather(rec, prov, rel):
    """Rules of one parsed weather file; returns problem texts."""
    problems = ["%s: float at %s (rules data are integers only)" % (rel, p) for p in _floats(rec)]
    keys = rec.get("key", [])
    times = [k.get("t_min") for k in keys]
    if any(b <= a for a, b in zip(times, times[1:])):
        problems.append("%s: key t_min must be strictly ascending" % rel)
    for k in keys:
        if math.hypot(k.get("wind_x_mm_s", 0), k.get("wind_y_mm_s", 0)) > WIND_MAX_MM_S:
            problems.append("%s: key t_min %s: wind speed above %d mm/s" % (rel, k.get("t_min"), WIND_MAX_MM_S))
        if k.get("temp_f10") == 880 and not any(s.get("document") == "CWV" and s.get("page") == 79 for s in k.get("sources", [])):
            problems.append("%s: key t_min %s: the 880 key must cite CWV page 79" % (rel, k.get("t_min")))
    return problems + _provenance(keys, prov, rel, "key")


def check_relief(rec, rel):
    """Rules of one parsed relief file the schema cannot express; returns problem texts."""
    problems = ["%s: float at %s (rules data are integers only)" % (rel, p) for p in _floats(rec)]
    widths = {len(r.get("dm", "").split()) for r in rec.get("row", [])}
    if len(widths) > 1:
        problems.append("%s: rows differ in length" % rel)
    for i, r in enumerate(rec.get("row", [])):
        if any(abs(int(v)) > RELIEF_MAX_DM for v in r.get("dm", "").split() if re.match(r"^-?[0-9]+$", v)):
            problems.append("%s: row %d: a height is beyond %d decimetres" % (rel, i, RELIEF_MAX_DM))
    return problems


def _validate_relief(root, files, reg, prov):
    if not files:
        return []
    problems = []
    with open(os.path.join(root, "data", "schemas", "relief.schema.json"), encoding="utf-8") as f:
        validator = Draft202012Validator(json.load(f), registry=reg)
    for path, rel in files:
        try:
            rec = _load(path)
        except tomllib.TOMLDecodeError as e:
            problems.append("%s: not valid TOML: %s" % (rel, e))
            continue
        for err in validator.iter_errors(rec):
            problems.append("%s: schema: %s at /%s" % (rel, err.message[:120], "/".join(map(str, err.absolute_path))))
        problems += check_relief(rec, rel)
        problems += _provenance([rec], prov, rel, "file")
    return problems


def validate(root):
    problems = []
    env_path = os.path.join(root, "data", "rules", "environment.toml")
    base = os.path.join(root, "data", "scenarios")
    weather = []
    relief = []
    if os.path.isdir(base):
        for family in sorted(os.listdir(base)):
            path = os.path.join(base, family, "weather.toml")
            if os.path.isfile(path):
                weather.append((path, "data/scenarios/%s/weather.toml" % family))
            path = os.path.join(base, family, "relief.toml")
            if os.path.isfile(path):
                relief.append((path, "data/scenarios/%s/relief.toml" % family))
    if not os.path.exists(env_path) and not weather and not relief:
        return problems
    reg = _registry(root)
    prov = Draft202012Validator({"$ref": "rules-common.schema.json#/$defs/provenance"}, registry=reg)
    if os.path.exists(env_path):
        terrain_path = os.path.join(root, "data", "rules", "terrain.toml")
        ids = [c["id"] for c in _load(terrain_path).get("class", [])] if os.path.exists(terrain_path) else None
        problems += check_environment(_load(env_path), ids, prov)
    problems += _validate_relief(root, relief, reg, prov)
    if not weather:
        return problems
    with open(os.path.join(root, "data", "schemas", "weather.schema.json"), encoding="utf-8") as f:
        validator = Draft202012Validator(json.load(f), registry=reg)
    for path, rel in weather:
        try:
            rec = _load(path)
        except tomllib.TOMLDecodeError as e:
            problems.append("%s: not valid TOML: %s" % (rel, e))
            continue
        for err in validator.iter_errors(rec):
            problems.append("%s: schema: %s at /%s" % (rel, err.message[:120], "/".join(map(str, err.absolute_path))))
        problems += check_weather(rec, prov, rel)
    return problems
