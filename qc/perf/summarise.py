#!/usr/bin/env python3
"""Percentile summariser and threshold evaluator for the M1 performance runs (SPEC-016 sections 3.4 and 10).

Everything is integer arithmetic. A run file is a CSV with the header `metric,index,value` and one row per sample
(frame number, tick number or callback number in `index`; an integer in `value`). Run files live at
`<runs>/<class>/<scene>/run<k>.csv` for class RM-R or RM-M, scene S0 to S6 and k = 1, 2, 3.

Usage:
  summarise.py percentiles FILE [--metric NAME]    print `metric stat value` for p50, p95, p99, p999, max
  summarise.py evaluate --thresholds FILE --runs DIR [--machine RM-R] [--warmup 600]
                                                   one PASS or FAIL line per (row, class, scene); exit 1 on any FAIL

Percentile: for N sorted samples and q in permille, stat = sorted[ceil(q * N / 1000) - 1]. A verdict uses the median of
the three runs and also reports the worst run. A missing file, run or metric fails the scene. A threshold row without a
valid calibration target fails. Samples of per-frame metrics with index below the warm-up (default 600) are discarded.
"""
import argparse
import csv
import os
import re
import sys
import tomllib

CLASSES = ("RM-R", "RM-M")
SCENES = tuple("S%d" % n for n in range(7))
RUNS = (1, 2, 3)
WARMUP_FRAMES = 600
PERCENTILES = (("p50", 500), ("p95", 950), ("p99", 990), ("p999", 999))
FRAME_METRICS = frozenset(
    ["frame_us", "render_us", "submit_us", "wait_sim_us", "gpu_visibility_us", "gpu_shadows_us", "gpu_gbuffer_us",
     "gpu_lighting_us", "gpu_smoke_us", "gpu_post_us", "gpu_ui_us", "draw_calls", "triangles", "vram_mb", "ram_mb"])
OPS = ("le", "ge", "eq")
UNITS = ("us", "mb", "pm", "count", "samples")
MACHINES = CLASSES + ("both",)
ROW_KEYS = ("id", "scene", "machine", "metric", "stat", "op", "value", "unit", "locked")
PROVENANCE = ("confidence", "placeholder", "calibration_target", "sources", "second_source", "note")
STAT_RX = re.compile(r"^(p50|p95|p99|p999|max|min|mean|sum|count|over_[0-9]+)$")
SCENE_RX = re.compile(r"^(all|S[0-6](-S[0-6])?)$")
CT_RX = re.compile(r"^CT-[0-9]{3}-[0-9]+$")
ID_RX = re.compile(r"^H16-([1-9]|1[01])(-[a-z0-9_]+)*$")


def percentile(samples, q):
    """Nearest-rank percentile, q in permille."""
    if not samples:
        raise ValueError("no samples")
    s = sorted(samples)
    rank = max(1, (q * len(s) + 999) // 1000)
    return s[rank - 1]


def stat_value(samples, stat):
    """One named statistic of a non-empty integer sample list."""
    if not samples:
        raise ValueError("no samples")
    for name, q in PERCENTILES:
        if stat == name:
            return percentile(samples, q)
    if stat == "max":
        return max(samples)
    if stat == "min":
        return min(samples)
    if stat == "mean":
        return sum(samples) // len(samples)
    if stat == "sum":
        return sum(samples)
    if stat == "count":
        return len(samples)
    if stat.startswith("over_"):
        limit = int(stat[5:])
        return sum(1 for v in samples if v > limit)
    raise ValueError("unknown stat %r" % stat)


def load_run(path):
    """Read a run file into {metric: [(index, value)]}. Every index and value must be an integer."""
    out = {}
    with open(path, newline="", encoding="utf-8") as f:
        reader = csv.reader(f)
        if next(reader, None) != ["metric", "index", "value"]:
            raise ValueError("%s: header must be metric,index,value" % path)
        for n, rec in enumerate(reader, 2):
            if len(rec) != 3:
                raise ValueError("%s:%d: expected 3 fields" % (path, n))
            try:
                idx, val = int(rec[1]), int(rec[2])
            except ValueError:
                raise ValueError("%s:%d: index and value must be integers" % (path, n)) from None
            out.setdefault(rec[0], []).append((idx, val))
    return out


def row_problems(row):
    """Return the reasons a threshold row cannot be evaluated (empty when sound)."""
    bad = []
    for k in ROW_KEYS:
        if k not in row:
            bad.append("missing %s" % k)
    ct = row.get("calibration_target")
    if not isinstance(ct, str) or not CT_RX.match(ct):
        bad.append("no calibration target")
    if bad:
        return bad
    if not isinstance(row["id"], str) or not ID_RX.match(row["id"]):
        bad.append("bad id")
    if not isinstance(row["scene"], str) or not SCENE_RX.match(row["scene"]):
        bad.append("bad scene")
    elif row["scene"] != "all" and "-" in row["scene"] and row["scene"][1] > row["scene"][4]:
        bad.append("scene range reversed")
    if row["machine"] not in MACHINES:
        bad.append("bad machine")
    if not isinstance(row["stat"], str) or not STAT_RX.match(row["stat"]):
        bad.append("bad stat")
    if row["op"] not in OPS:
        bad.append("bad op")
    if type(row["value"]) is not int:
        bad.append("value is not an integer")
    if row["unit"] not in UNITS:
        bad.append("bad unit")
    if type(row["locked"]) is not bool:
        bad.append("locked is not a boolean")
    if not isinstance(row["metric"], str) or not row["metric"]:
        bad.append("bad metric")
    if row.get("placeholder") is True and row.get("locked") is True:
        bad.append("a placeholder row cannot be locked")
    return bad


def scenes_of(pattern):
    if pattern == "all":
        return list(SCENES)
    if "-" in pattern:
        return list(SCENES[int(pattern[1]):int(pattern[4]) + 1])
    return [pattern]


def holds(op, got, want):
    return got <= want if op == "le" else got >= want if op == "ge" else got == want


def _distance(op, got, want):
    """How badly `got` misses `want`; larger is worse (negative when it passes with room)."""
    return got - want if op == "le" else want - got if op == "ge" else abs(got - want)


def _run_stat(path, metric, stat, warmup):
    samples = load_run(path).get(metric)
    if samples is None:
        raise ValueError("metric %s absent" % metric)
    if metric in FRAME_METRICS:
        samples = [s for s in samples if s[0] >= warmup]
    return stat_value([v for _, v in samples], stat)


def evaluate(rows, runs_dir, classes, warmup=WARMUP_FRAMES):
    """Return one result dict per (row, class, scene). Keys: id, machine, scene, metric, stat, op, threshold, value,
    worst, verdict, reason. `value` is the median over the three runs and `worst` the run furthest from the threshold."""
    results = []
    for r in rows:
        problems = row_problems(r)
        wanted = [c for c in classes if r.get("machine") in (c, "both")] if not problems else list(classes)
        scenes = scenes_of(r["scene"]) if not problems else [r.get("scene", "?")]
        for cls in wanted:
            for scene in scenes:
                res = {"id": r.get("id", "?"), "machine": cls, "scene": scene, "metric": r.get("metric", "?"),
                       "stat": r.get("stat", "?"), "op": r.get("op", "?"), "threshold": r.get("value"),
                       "value": None, "worst": None, "verdict": "FAIL", "reason": ""}
                results.append(res)
                if problems:
                    res["reason"] = "; ".join(problems)
                    continue
                vals = []
                try:
                    for k in RUNS:
                        path = os.path.join(runs_dir, cls, scene, "run%d.csv" % k)
                        if not os.path.isfile(path):
                            raise ValueError("missing %s" % os.path.relpath(path, runs_dir))
                        vals.append(_run_stat(path, r["metric"], r["stat"], warmup))
                except (ValueError, OSError) as e:
                    res["reason"] = str(e)
                    continue
                res["value"] = sorted(vals)[1]
                res["worst"] = max(vals, key=lambda v: _distance(r["op"], v, r["value"]))
                res["verdict"] = "PASS" if holds(r["op"], res["value"], r["value"]) else "FAIL"
    return results


def load_rows(path):
    with open(path, "rb") as f:
        return tomllib.load(f).get("threshold", [])


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    p = sub.add_parser("percentiles")
    p.add_argument("file")
    p.add_argument("--metric")
    e = sub.add_parser("evaluate")
    e.add_argument("--thresholds", required=True)
    e.add_argument("--runs", required=True)
    e.add_argument("--machine", action="append", choices=CLASSES)
    e.add_argument("--warmup", type=int, default=WARMUP_FRAMES)
    a = ap.parse_args(argv)
    if a.cmd == "percentiles":
        run = load_run(a.file)
        for metric in sorted(run) if not a.metric else [a.metric]:
            vals = [v for _, v in run[metric]]
            for name in [n for n, _ in PERCENTILES] + ["max"]:
                print(metric, name, stat_value(vals, name))
        return 0
    classes = a.machine or [c for c in CLASSES if os.path.isdir(os.path.join(a.runs, c))]
    if not classes:
        print("summarise: no run directories under %s" % a.runs)
        return 1
    results = evaluate(load_rows(a.thresholds), a.runs, classes, a.warmup)
    for r in results:
        print("%s %s %s %s %s %s %s %s %s%s" % (
            r["verdict"], r["id"], r["machine"], r["scene"], r["metric"], r["stat"], r["op"], r["threshold"],
            "median=%s worst=%s" % (r["value"], r["worst"]) if r["value"] is not None else "",
            " (%s)" % r["reason"] if r["reason"] else ""))
    failed = sum(1 for r in results if r["verdict"] == "FAIL")
    print("summarise: %d rows, %d PASS, %d FAIL" % (len(results), len(results) - failed, failed))
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
