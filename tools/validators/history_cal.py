"""History-calibration validator plugin (SPEC-011): scripts and the policy table.

Checks, as plugin rule R8 expects (`validate(root) -> list[str]`):
  - every data/scenarios/*/script.toml against data/schemas/replay-script.schema.json, with no float,
    provenance rules (placeholder needs a calibration target, other rows sources, verified a second source),
    unique row ids, times inside clock_start_s..end_s, early_s < at_s < late_s, observe windows non-empty,
    and every observe `after` naming a trigger row
  - qc/calibration/policy.toml: schema calibration-policy/1, integer values, every row a placeholder whose
    calibration_target is CT-011-1101 to CT-011-1105, unique ids, the ids the Rust loader requires
  - qc/calibration/*-bands.toml against data/schemas/bands.schema.json; the Gettysburg file must hold exactly
    HB-01 to HB-24, no verified row, and every low and high recomputed from its phrase and source_value by
    the CT-011-1101 widening rules in policy.toml; ABT components are re-summed from qc/fixtures/gb-known-conflicts.toml
  - qc/fixtures/history-conflicts.toml: rows CF-01 to CF-10, a known rule, references that name a band or trigger,
    provenance, and, when data/scenarios/gb_timeline exists, the timeline rows tl_01 to tl_15
Nothing is read outside the repository and nothing is written.
"""
import glob
import json
import os
import re
import tomllib

from jsonschema import Draft202012Validator
from referencing import Registry, Resource

POLICY_IDS = {
    "widen_exact_pm": 1101, "widen_approx_pm": 1101, "approx_time_s": 1101, "weak_width_pm": 1101,
    "spread_limit_pm": 1102, "seeds_smoke_lo": 1102, "seeds_smoke_hi": 1102, "seeds_full_lo": 1102,
    "seeds_full_hi": 1102, "seeds_fit_lo": 1102, "seeds_fit_hi": 1102, "seeds_holdout_lo": 1102,
    "seeds_holdout_hi": 1102, "trend_alert_pm": 1103, "holdout_pass_pm": 1104, "holdout_loss_factor": 1104,
    "holdout_looks_per_cycle": 1104, "fit_rounds": 1105, "fit_grid_max_points": 1105, "fit_excess_cap_pm": 1105,
    "fit_accept_factor_pm": 1105, "fit_accept_slack_per_band": 1105, "fit_free_param_divisor": 1105,
}
ROW_TABLES = ("order", "trigger", "observe", "override")
HB_IDS = ["HB-%02d" % n for n in range(1, 25)]
CF_IDS = ["CF-%02d" % n for n in range(1, 11)]
TL_IDS = ["tl_%02d" % n for n in range(1, 16)]
CF_RULES = ("envelope", "time_window", "no_subtract", "bad_value", "record_range", "window_only")
CF_KEYS = {"id", "subject", "rule", "ref", "rate_ref", "rate_base", "value_a", "value_b", "value_c", "value_d",
           "confidence", "placeholder", "calibration_target", "sources", "second_source", "note"}


def _floats(node, path=""):
    if isinstance(node, float):
        yield path or "(root)"
    elif isinstance(node, dict):
        for k, v in node.items():
            yield from _floats(v, path + "/" + k)
    elif isinstance(node, list):
        for i, v in enumerate(node):
            yield from _floats(v, "%s[%d]" % (path, i))


def _registry(schema_dir):
    reg = Registry()
    for name in sorted(os.listdir(schema_dir)):
        if name.endswith(".schema.json"):
            with open(os.path.join(schema_dir, name), encoding="utf-8") as f:
                reg = reg.with_resource(name, Resource.from_contents(json.load(f)))
    return reg


def _provenance(rel, where, row):
    out = []
    if row.get("placeholder") is True:
        if not re.fullmatch(r"CT-[0-9]{3}-[0-9]+", str(row.get("calibration_target", ""))):
            out.append("%s: %s placeholder without a CT-<spec>-<n> calibration_target" % (rel, where))
    elif not row.get("sources"):
        out.append("%s: %s needs sources (or placeholder with a calibration_target)" % (rel, where))
    if row.get("confidence") == "verified" and not str(row.get("second_source", "")).strip():
        out.append("%s: %s is verified without second_source" % (rel, where))
    return out


def _check_script(rel, rec, validator):
    out = ["%s: schema: %s at /%s" % (rel, e.message[:120], "/".join(map(str, e.absolute_path)))
           for e in validator.iter_errors(rec)]
    out += ["%s: float at %s (scripts are integers only)" % (rel, p) for p in _floats(rec)]
    meta = rec.get("meta", {})
    start, end = meta.get("clock_start_s", 0), meta.get("end_s", 0)
    seen, triggers = set(), {r.get("id") for r in rec.get("trigger", []) if isinstance(r, dict)}
    for table in ROW_TABLES:
        for i, row in enumerate(rec.get(table, [])):
            if not isinstance(row, dict):
                continue
            where = "%s[%d]" % (table, i)
            out += _provenance(rel, where, row)
            if row.get("id") in seen:
                out.append("%s: %s duplicate id %r" % (rel, where, row.get("id")))
            seen.add(row.get("id"))
            if table != "observe" and isinstance(row.get("at_s"), int) and not start <= row["at_s"] <= end:
                out.append("%s: %s at_s %d outside %d..%d" % (rel, where, row["at_s"], start, end))
            if table == "trigger":
                at = row.get("at_s")
                if isinstance(at, int) and isinstance(row.get("early_s"), int) and not row["early_s"] < at:
                    out.append("%s: %s early_s must be before at_s" % (rel, where))
                if isinstance(at, int) and isinstance(row.get("late_s"), int) and not row["late_s"] > at:
                    out.append("%s: %s late_s must be after at_s" % (rel, where))
            if table == "observe":
                lo, hi = row.get("lo_s"), row.get("hi_s")
                if isinstance(lo, int) and isinstance(hi, int) and lo >= hi:
                    out.append("%s: %s window %d..%d is empty" % (rel, where, lo, hi))
                if "after" in row and row["after"] not in triggers:
                    out.append("%s: %s after %r names no trigger row" % (rel, where, row["after"]))
    return out


def _check_policy(rel, rec):
    out = []
    if rec.get("schema") != "calibration-policy/1":
        out.append("%s: schema must be calibration-policy/1" % rel)
    out += ["%s: float at %s (policy values are integers)" % (rel, p) for p in _floats(rec)]
    seen = set()
    for i, row in enumerate(rec.get("threshold", [])):
        where = "threshold[%d]" % i
        rid = row.get("id")
        if rid not in POLICY_IDS:
            out.append("%s: %s unknown id %r" % (rel, where, rid))
            continue
        if rid in seen:
            out.append("%s: %s duplicate id %r" % (rel, where, rid))
        seen.add(rid)
        if not isinstance(row.get("value"), int) or isinstance(row.get("value"), bool) or row["value"] < 0:
            out.append("%s: %s value must be a non-negative integer" % (rel, where))
        if row.get("placeholder") is not True or row.get("calibration_target") != "CT-011-%d" % POLICY_IDS[rid]:
            out.append("%s: %s must be a placeholder of CT-011-%d" % (rel, rid, POLICY_IDS[rid]))
        out += _provenance(rel, where, row)
    out += ["%s: missing threshold %s" % (rel, m) for m in sorted(set(POLICY_IDS) - seen)]
    return out


def widen(phrase, value, policy):
    """The (low, high) of section 7 for a phrase, high None being open; None when the phrase is malformed."""
    words = str(phrase).split()
    if not words or any(w.lstrip("-").isdigit() is False for w in words[1:]):
        return None
    kind, nums = words[0], [int(w) for w in words[1:]]
    exact, approx = policy["widen_exact_pm"], policy["widen_approx_pm"]
    down = lambda v, pm: v * (1000 - pm) // 1000
    up = lambda v, pm: -((-v * (1000 + pm)) // 1000)
    if kind in ("exact", "about", "about_open", "nearly", "over"):
        if nums or not isinstance(value, int):
            return None
        return {"exact": (down(value, exact), up(value, exact)), "about": (down(value, approx), up(value, approx)),
                "about_open": (down(value, approx), None), "nearly": (down(value, exact), value),
                "over": (value, up(value, exact))}[kind]
    if value is not None:
        return None
    if kind == "range" and len(nums) == 2 and nums[0] <= nums[1]:
        return nums[0], nums[1]
    if kind == "range_open" and len(nums) == 1:
        return nums[0], None
    return None


def _abt_links(rec):
    """Band id -> (field, figure) the ABT components imply, from the audited conflict rows C02 and C03."""
    rows = {r.get("id"): r for r in rec.get("conflict", []) if isinstance(r, dict)}
    out, problems = {}, []
    try:
        sums = [rows["C02"]["sums"][0], rows["C02"]["sums"][1], rows["C03"]["sums"][0]]
    except (KeyError, IndexError, TypeError):
        return out, ["gb-known-conflicts.toml: rows C02 and C03 with sums are required"]
    for i, row in enumerate(sums[:2]):
        if sum(row["parts"]) != row["total"]:
            problems.append("gb-known-conflicts.toml: C02 sums[%d] parts add to %d, not %d" % (i, sum(row["parts"]), row["total"]))
    union, conf, charge = sums
    out["HB-07"] = ("source_value", sum(union["parts"]))
    out["HB-08"] = ("high", sum(conf["parts"]))
    out["HB-09"] = ("source_value", sum(union["parts"][:2]))
    out["HB-10"] = ("source_value", sum(conf["parts"][:2]))
    out["HB-06"] = ("high", sum(charge["parts"]))
    return out, problems


def _check_bands(rel, rec, validator, policy, abt):
    out = ["%s: schema: %s at /%s" % (rel, e.message[:120], "/".join(map(str, e.absolute_path)))
           for e in validator.iter_errors(rec)]
    out += ["%s: float at %s (bands are integers only)" % (rel, p) for p in _floats(rec)]
    rows = [r for r in rec.get("band", []) if isinstance(r, dict)]
    seen = [r.get("id") for r in rows]
    gettysburg = os.path.basename(rel) == "gettysburg-bands.toml"
    if len(set(seen)) != len(seen):
        out.append("%s: duplicate band ids" % rel)
    if gettysburg and sorted(seen) != HB_IDS:
        out.append("%s: ids must be exactly HB-01 to HB-24" % rel)
    for row in rows:
        where = str(row.get("id"))
        out += _provenance(rel, where, row)
        if row.get("confidence") == "verified":
            out.append("%s: %s is verified; no band is verified until a second source is consulted" % (rel, where))
        if isinstance(row.get("from_s"), int) and isinstance(row.get("to_s"), int) and row["from_s"] >= row["to_s"]:
            out.append("%s: %s window is empty" % (rel, where))
        if isinstance(row.get("high"), int) and isinstance(row.get("low"), int) and row["high"] < row["low"]:
            out.append("%s: %s high is below low" % (rel, where))
        if policy is not None:
            edges = widen(row.get("phrase"), row.get("source_value"), policy)
            if edges is None:
                out.append("%s: %s phrase %r with source_value %r is not a widening rule" % (
                    rel, where, row.get("phrase"), row.get("source_value")))
            elif edges != (row.get("low"), row.get("high")):
                out.append("%s: %s edges %r..%r but its phrase gives %r..%r" % (
                    rel, where, row.get("low"), row.get("high"), edges[0], edges[1]))
        if gettysburg and where in abt and row.get(abt[where][0]) != abt[where][1]:
            out.append("%s: %s %s is %r but the ABT components give %d" % (
                rel, where, abt[where][0], row.get(abt[where][0]), abt[where][1]))
    return out


def _check_conflicts(rel, rec, band_ids, trigger_ids):
    out = []
    if rec.get("schema") != "history-conflicts/1":
        out.append("%s: schema must be history-conflicts/1" % rel)
    out += ["%s: float at %s (fixtures are integers only)" % (rel, p) for p in _floats(rec)]
    rows = [r for r in rec.get("conflict", []) if isinstance(r, dict)]
    ids = [r.get("id") for r in rows]
    if sorted(ids) != CF_IDS:
        out.append("%s: ids must be exactly CF-01 to CF-10" % rel)
    for row in rows:
        where = str(row.get("id"))
        out += _provenance(rel, where, row)
        out += ["%s: %s unknown key %r" % (rel, where, k) for k in sorted(set(row) - CF_KEYS)]
        rule, ref = row.get("rule"), row.get("ref")
        if rule not in CF_RULES:
            out.append("%s: %s unknown rule %r" % (rel, where, rule))
        elif rule in ("envelope", "no_subtract") and ref not in band_ids:
            out.append("%s: %s ref %r names no band" % (rel, where, ref))
        elif rule == "time_window" and trigger_ids is not None and ref not in trigger_ids:
            out.append("%s: %s ref %r names no trigger" % (rel, where, ref))
        elif rule == "record_range" and not ref:
            out.append("%s: %s needs a ref" % (rel, where))
        if "rate_ref" in row and row["rate_ref"] not in band_ids:
            out.append("%s: %s rate_ref %r names no band" % (rel, where, row["rate_ref"]))
        for key in ("value_a", "value_b"):
            if not isinstance(row.get(key), int) or isinstance(row.get(key), bool):
                out.append("%s: %s %s must be an integer" % (rel, where, key))
    return out


def _load(path):
    with open(path, "rb") as f:
        return tomllib.load(f)


def _validate_tables(root, schema_dir):
    problems = []
    cal = os.path.join(root, "qc", "calibration")
    band_files = sorted(glob.glob(os.path.join(cal, "*-bands.toml")))
    fixtures = os.path.join(root, "qc", "fixtures", "history-conflicts.toml")
    policy = None
    policy_path = os.path.join(cal, "policy.toml")
    if os.path.exists(policy_path):
        try:
            policy = {r["id"]: r["value"] for r in _load(policy_path).get("threshold", [])}
            if not all(isinstance(policy.get(k), int) for k in ("widen_exact_pm", "widen_approx_pm")):
                policy = None
        except (tomllib.TOMLDecodeError, KeyError, TypeError):
            policy = None
    abt, band_ids = {}, set()
    known = os.path.join(root, "qc", "fixtures", "gb-known-conflicts.toml")
    if os.path.exists(known):
        try:
            abt, more = _abt_links(_load(known))
            problems += more
        except tomllib.TOMLDecodeError as e:
            problems.append("qc/fixtures/gb-known-conflicts.toml: not valid TOML: %s" % e)
    if band_files:
        schema_path = os.path.join(schema_dir, "bands.schema.json")
        if not os.path.exists(schema_path):
            return problems + ["data/schemas/bands.schema.json is missing"]
        with open(schema_path, encoding="utf-8") as f:
            validator = Draft202012Validator(json.load(f), registry=_registry(schema_dir))
        for path in band_files:
            rel = os.path.relpath(path, root)
            try:
                rec = _load(path)
            except tomllib.TOMLDecodeError as e:
                problems.append("%s: not valid TOML: %s" % (rel, e))
                continue
            band_ids |= {r.get("id") for r in rec.get("band", []) if isinstance(r, dict)}
            problems += _check_bands(rel, rec, validator, policy, abt)
    if os.path.exists(fixtures):
        rel = os.path.relpath(fixtures, root)
        timeline = os.path.join(root, "data", "scenarios", "gb_timeline", "script.toml")
        trigger_ids = None
        if os.path.exists(timeline):
            try:
                rec = _load(timeline)
                have = {r.get("id") for t in ROW_TABLES for r in rec.get(t, []) if isinstance(r, dict)}
                trigger_ids = {r.get("id") for r in rec.get("trigger", []) if isinstance(r, dict)}
                problems += ["data/scenarios/gb_timeline/script.toml: missing timeline row %s" % m
                             for m in sorted(set(TL_IDS) - have)]
            except tomllib.TOMLDecodeError:
                pass
        try:
            problems += _check_conflicts(rel, _load(fixtures), band_ids, trigger_ids)
        except tomllib.TOMLDecodeError as e:
            problems.append("%s: not valid TOML: %s" % (rel, e))
    return problems


def validate(root):
    problems = []
    scripts = sorted(glob.glob(os.path.join(root, "data", "scenarios", "*", "script.toml")))
    policy = os.path.join(root, "qc", "calibration", "policy.toml")
    schema_dir = os.path.join(root, "data", "schemas")
    validator = None
    if scripts:
        schema_path = os.path.join(schema_dir, "replay-script.schema.json")
        if not os.path.exists(schema_path):
            return ["data/schemas/replay-script.schema.json is missing"]
        with open(schema_path, encoding="utf-8") as f:
            validator = Draft202012Validator(json.load(f), registry=_registry(schema_dir))
    for path in scripts + ([policy] if os.path.exists(policy) else []):
        rel = os.path.relpath(path, root)
        try:
            with open(path, "rb") as f:
                rec = tomllib.load(f)
        except tomllib.TOMLDecodeError as e:
            problems.append("%s: not valid TOML: %s" % (rel, e))
            continue
        problems += _check_policy(rel, rec) if path == policy else _check_script(rel, rec, validator)
    return problems + _validate_tables(root, schema_dir)
