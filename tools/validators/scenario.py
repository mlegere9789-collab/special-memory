"""Validator plugin for data scenarios (SPEC-008 sections 2.3 and 3, acceptance T8-02).

Checks every `data/scenarios/*.toml` (the sidecar directories are not scenarios) against
`data/schemas/scenario.schema.json` and against rules V1 to V12, the Python twin of
`battle_rules::Scenario::from_toml_str`. Each problem is reported as `<file>: Vnn <text>`.

The fixtures in `qc/fixtures/scenario-bad/` are named `vNN_<what>.toml`. The plugin also checks
that each one is refused with exactly rule `VNN` here, so the Python and Rust loaders cannot
drift apart (a fixture the plugin accepts, or flags with another rule, is itself a problem).
"""
import json
import os
import re
import tomllib

import jsonschema
from jsonschema import Draft202012Validator
from referencing import Registry, Resource

ID = re.compile(r"^[a-z0-9_-]{1,32}$")
MAX_UNITS = 4096
MAX_MEN = 1000000
FNV_BASIS = 0xCBF29CE484222325
FNV_PRIME = 0x100000001B3


def fnv1a64(data):
    h = FNV_BASIS
    for b in data:
        h = ((h ^ b) * FNV_PRIME) & 0xFFFFFFFFFFFFFFFF
    return h


def rows_of(text):
    lines = [ln[:-1] if ln.endswith("\r") else ln for ln in text.split("\n")]
    if lines and lines[-1] == "":
        lines.pop()
    return lines


def rows_hash(rows):
    return fnv1a64("".join(r + "\n" for r in rows).encode("utf-8"))


def floats(node, path=""):
    if isinstance(node, float):
        yield path or "(root)"
    elif isinstance(node, dict):
        for k, v in node.items():
            yield from floats(v, path + "/" + k if path else k)
    elif isinstance(node, list):
        for i, v in enumerate(node):
            yield from floats(v, "%s[%d]" % (path, i))


def _schema_validator(root):
    sdir = os.path.join(root, "data", "schemas")
    reg = Registry()
    for name in sorted(os.listdir(sdir)):
        if name.endswith(".schema.json"):
            with open(os.path.join(sdir, name), encoding="utf-8") as f:
                reg = reg.with_resource(name, Resource.from_contents(json.load(f)))
    with open(os.path.join(sdir, "scenario.schema.json"), encoding="utf-8") as f:
        return Draft202012Validator(json.load(f), registry=reg)


def _terrain_blocking(root):
    path = os.path.join(root, "data", "rules", "terrain.toml")
    with open(path, "rb") as f:
        return {c["id"]: bool(c["blocks_movement"]) for c in tomllib.load(f)["class"]}


def _weapons(root):
    with open(os.path.join(root, "data", "rules", "weapons.toml"), "rb") as f:
        return {w["id"] for w in tomllib.load(f)["weapon"]}


class Ctx:
    def __init__(self, root):
        self.root = root
        self.validator = _schema_validator(root)
        self.blocking = _terrain_blocking(root)
        self.weapons = _weapons(root)
        self.commanders = os.path.join(root, "data", "history", "commanders")


def check_file(ctx, path):
    """Return a list of (rule, text) for one scenario file."""
    out = []

    def bad(rule, text):
        out.append((rule, text))

    try:
        with open(path, "rb") as f:
            doc = tomllib.load(f)
    except tomllib.TOMLDecodeError as e:
        return [("V01", "not valid TOML: %s" % e)]
    for p in floats(doc):
        bad("V01", "float at %s (scenario data are integers only)" % p)
    errors = list(ctx.validator.iter_errors(doc))
    # A row whose provenance fails also leaves its keys "unevaluated"; that echo is not a second fault.
    failed = {"/".join(map(str, e.absolute_path)) for e in errors if "second_source" in e.message}
    for err in errors:
        where = "/".join(map(str, err.absolute_path))
        if err.validator == "unevaluatedProperties" and where in failed:
            continue
        text = "%s at /%s" % (err.message[:140], where)
        if "second_source" in err.message and err.validator in ("required", "minLength"):
            bad("V12", "verified without second_source at /%s" % where)
        elif err.validator == "const" and where == "tick_hz":
            bad("V09", "tick_hz must be 20")
        elif err.validator == "maximum" and where.startswith("unit/") and where.endswith("/men"):
            bad("V11", text)
        elif err.validator == "maximum" and where in ("agent_cap", "f0_cap"):
            bad("V08", text)
        elif where == "fidelity_class":
            bad("V08", text)
        elif err.validator == "maxItems":
            bad("V11", text)
        else:
            bad("V01", text)
    if out:
        return out
    return out + check_semantics(ctx, doc, os.path.dirname(path))


def check_semantics(ctx, doc, directory):
    out = []

    def bad(rule, text):
        out.append((rule, text))

    dur = doc["duration_s"]
    if doc["f0_cap"] > doc["agent_cap"]:
        bad("V08", "f0_cap %d above agent_cap %d" % (doc["f0_cap"], doc["agent_cap"]))
    units = doc["unit"]
    by_id = {}
    for u in units:
        if u["id"] in by_id:
            bad("V02", "duplicate unit id %s" % u["id"])
        by_id[u["id"]] = u
    for kind in ("objective", "trigger"):
        ids = [r["id"] for r in doc.get(kind, [])]
        for i in sorted({x for x in ids if ids.count(x) > 1}):
            bad("V02", "duplicate %s id %s" % (kind, i))
    wx = [w["from_s"] for w in doc.get("weather", [])]
    if len(set(wx)) != len(wx):
        bad("V02", "two weather rows at one time")
    sc = [(c["at_s"], c["side"]) for c in doc.get("strength_check", [])]
    if len(set(sc)) != len(sc):
        bad("V02", "two strength checks at one time and side")
    for u in units:
        arm = u["arm"]
        if arm == "hq":
            ok = u["men"] == 0 and u["guns"] == 0 and u["weapon"] == "" and u["rounds_per_man"] == 0
        elif arm == "battery":
            ok = u["men"] >= 1 and u["guns"] >= 1 and u["weapon"] != ""
        else:
            ok = u["men"] >= 1 and u["guns"] == 0 and u["weapon"] != ""
        if not ok:
            bad("V11", "unit %s: men, guns, weapon and rounds do not fit arm %s" % (u["id"], arm))
        if arm != "hq" and u["weapon"] and u["weapon"] not in ctx.weapons:
            bad("V03", "unit %s names unknown weapon %s" % (u["id"], u["weapon"]))
        if u["commander"] and not os.path.exists(os.path.join(ctx.commanders, u["commander"] + ".toml")):
            bad("V03", "unit %s names unknown commander %s" % (u["id"], u["commander"]))
        if u["parent"] and u["parent"] not in by_id:
            bad("V03", "unit %s has unknown parent %s" % (u["id"], u["parent"]))
        elif u["parent"] and by_id[u["parent"]]["side"] != u["side"]:
            bad("V04", "unit %s and its parent %s are on different sides" % (u["id"], u["parent"]))
    for u in units:
        cur, steps = u, 0
        while cur["parent"] in by_id:
            cur = by_id[cur["parent"]]
            steps += 1
            if steps > len(units):
                bad("V04", "parent chain of unit %s loops" % u["id"])
                break

    def real(unit, what):
        v = by_id.get(unit)
        if v is None:
            bad("V03", "%s: unknown unit %s" % (what, unit))
        elif v["arm"] == "hq":
            bad("V03", "%s: %s is a headquarters" % (what, unit))

    # Terrain: the grid decides which positions are blocked (V10, V06).
    t = doc["terrain"]
    cell = t.get("cell_mm", 8000)
    ox, oy = t.get("origin_x_mm", doc["map"]["origin_x_mm"]), t.get("origin_y_mm", doc["map"]["origin_y_mm"])
    rows = None
    if "rows" in t:
        rows = rows_of(t["rows"])
    else:
        path = os.path.join(directory, t["source"])
        if not os.path.exists(path):
            bad("V10", "rows file %s is missing" % t["source"])
        else:
            with open(path, encoding="utf-8", newline="") as f:
                rows = rows_of(f.read())
            if rows_hash(rows) != int(t["source_hash"], 16):
                bad("V10", "rows file %s does not match source_hash" % t["source"])
    legend = {e["ch"]: e["class"] for e in t["legend"]}
    grid = None
    if rows is not None:
        if len(legend) != len(t["legend"]):
            bad("V10", "duplicate legend character")
        elif any(c not in ctx.blocking for c in legend.values()):
            bad("V10", "legend names an unknown terrain class")
        elif not rows or not rows[0] or any(len(r) != len(rows[0]) for r in rows):
            bad("V10", "terrain rows are empty or unequal")
        elif any(ch not in legend for r in rows for ch in r):
            bad("V10", "terrain row has a character the legend lacks")
        elif len(rows) * len(rows[0]) > 16777216:
            bad("V10", "too many terrain cells")
        else:
            grid = rows

    m = doc["map"]

    def where(x, y, what, blocking=True):
        if not (ox_ok(m, x, y)):
            bad("V06", "%s: (%d, %d) is outside the map" % (what, x, y))
        elif blocking and grid is not None:
            cx = min(max((x - ox) // cell, 0), len(grid[0]) - 1)
            cy = min(max((y - oy) // cell, 0), len(grid) - 1)
            if ctx.blocking[legend[grid[cy][cx]]]:
                bad("V06", "%s: (%d, %d) is on a blocking cell" % (what, x, y))

    arrivals, departures = {}, {}
    for a in doc["arrival"]:
        real(a["unit"], "arrival")
        if a["unit"] in arrivals:
            bad("V05", "two arrivals for unit %s" % a["unit"])
        arrivals[a["unit"]] = a
        if a["at_s"] > dur:
            bad("V05", "arrival of %s at %d s is after the end" % (a["unit"], a["at_s"]))
        if not a["low_s"] <= a["at_s"] <= a["high_s"]:
            bad("V05", "arrival of %s outside its evidence window" % a["unit"])
        where(a["x_mm"], a["y_mm"], "arrival of " + a["unit"])
    for u in units:
        if u["arm"] != "hq" and u["id"] not in arrivals:
            bad("V05", "unit %s never arrives" % u["id"])
    for d in doc.get("departure", []):
        real(d["unit"], "departure")
        if d["unit"] in departures:
            bad("V05", "two departures for unit %s" % d["unit"])
        departures[d["unit"]] = d
        if d["at_s"] > dur or d["unit"] not in arrivals or d["at_s"] <= arrivals[d["unit"]]["at_s"]:
            bad("V05", "unit %s departs outside the scenario or not after its arrival" % d["unit"])
    for c in doc.get("strength_check", []):
        if c["at_s"] > dur:
            bad("V05", "strength check at %d s is after the end" % c["at_s"])
        total = sum(
            u["men"] for u in units
            if u["side"] == c["side"] and u["id"] in arrivals and arrivals[u["id"]]["at_s"] <= c["at_s"]
            and not (u["id"] in departures and departures[u["id"]]["at_s"] <= c["at_s"]))
        if abs(total - c["men"]) > c["tol_men"]:
            bad("V07", "at %d s the %s units hold %d men, expected %d +/- %d" % (c["at_s"], c["side"], total, c["men"], c["tol_men"]))
    for o in doc.get("objective", []):
        where(o["x_mm"], o["y_mm"], "objective " + o["id"], blocking=False)
    for tr in doc.get("trigger", []):
        real(tr["unit"], "trigger " + tr["id"])
        if tr["at_s"] > dur:
            bad("V05", "trigger %s at %d s is after the end" % (tr["id"], tr["at_s"]))
        if tr["kind"] == "fire_mode":
            real(tr["target"], "trigger %s target" % tr["id"])
        if tr["kind"] == "order" and tr["order"] == "move":
            where(tr["x_mm"], tr["y_mm"], "trigger " + tr["id"])
    ws = sorted(wx)
    if ws and ws[0] != 0:
        bad("V05", "the first weather row must have from_s = 0")
    if any(w > dur for w in ws):
        bad("V05", "a weather row starts after the end")
    return out


def ox_ok(m, x, y):
    return (m["origin_x_mm"] <= x < m["origin_x_mm"] + m["width_mm"]
            and m["origin_y_mm"] <= y < m["origin_y_mm"] + m["height_mm"])


def validate(root):
    scen = os.path.join(root, "data", "scenarios")
    fixtures = os.path.join(root, "qc", "fixtures", "scenario-bad")
    problems = []
    names = []
    if os.path.isdir(scen):
        names = sorted(n for n in os.listdir(scen) if n.endswith(".toml") and os.path.isfile(os.path.join(scen, n)))
    if not names and not os.path.isdir(fixtures):
        return problems
    ctx = Ctx(root)
    for n in names:
        found = check_file(ctx, os.path.join(scen, n))
        for rule, text in found:
            problems.append("data/scenarios/%s: %s %s" % (n, rule, text))
        stem = os.path.splitext(n)[0]
        with open(os.path.join(scen, n), "rb") as f:
            sid = tomllib.load(f).get("id")
        if not found and sid != stem:
            problems.append("data/scenarios/%s: V02 id %r does not match the file name" % (n, sid))
    if os.path.isdir(fixtures):
        for n in sorted(os.listdir(fixtures)):
            m = re.match(r"^v(\d\d)_.*\.toml$", n)
            if not m:
                continue
            want = "V" + m.group(1)
            got = sorted({r for r, _ in check_file(ctx, os.path.join(fixtures, n))})
            if got != [want]:
                problems.append("qc/fixtures/scenario-bad/%s: expected exactly rule %s, found %s" % (n, want, got or "none"))
    return problems
