"""History-calibration validator plugin (SPEC-011): scripts and the policy table.

Checks, as plugin rule R8 expects (`validate(root) -> list[str]`):
  - every data/scenarios/*/script.toml against data/schemas/replay-script.schema.json, with no float,
    provenance rules (placeholder needs a calibration target, other rows sources, verified a second source),
    unique row ids, times inside clock_start_s..end_s, early_s < at_s < late_s, observe windows non-empty,
    and every observe `after` naming a trigger row
  - qc/calibration/policy.toml: schema calibration-policy/1, integer values, every row a placeholder whose
    calibration_target is CT-011-1101 to CT-011-1105, unique ids, the ids the Rust loader requires
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
    return problems
