#!/usr/bin/env python3
"""Independent reference for SPEC-009 acceptance test T9-02 (the weather timeline).

Plain Python with `fractions.Fraction`; it shares no code with the Rust environment module and
exists only to produce expected values that the Rust tests compare against. The Rust code itself
uses no floating point.

Output is CSV: now,<tick>,<temp_f10>,<humidity_pm>,<wind_x_mm_s>,<wind_y_mm_s>,<rain_pm>,
<clear_vis_mm>,<light_pm>. The values at battle tick k are the linear interpolation of every field
of data/scenarios/gettysburg/weather.toml between the bracketing keys, weighted in ticks
(1200 per minute) at the absolute tick start_minute * 1200 + k and truncated toward zero
(`int(Fraction)`); before the first key and after the last the end key. Ticks sampled: every key,
one tick after it, a quarter, a half and three quarters of the way to the next, one tick before
the next, then the last key and two ticks beyond it.

The committed file qc/fixtures/ref_weather.csv was produced with
    python3 qc/fixtures/ref_weather.py > qc/fixtures/ref_weather.csv
CI does not run this script; it runs the Rust tests, which read the committed output, and
tools/test_ref_environment.py, which regenerates it and compares.
"""
import os
import tomllib
from fractions import Fraction

FIELDS = ("temp_f10", "humidity_pm", "wind_x_mm_s", "wind_y_mm_s", "rain_pm", "clear_vis_mm", "light_pm")
TICKS_PER_MINUTE = 1200


def load():
    root = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    with open(os.path.join(root, "data", "scenarios", "gettysburg", "weather.toml"), "rb") as f:
        return tomllib.load(f)


def at(data, k):
    keys = data["key"]
    t = data["start_minute"] * TICKS_PER_MINUTE + k
    if t <= keys[0]["t_min"] * TICKS_PER_MINUTE:
        return [keys[0][f] for f in FIELDS]
    if t >= keys[-1]["t_min"] * TICKS_PER_MINUTE:
        return [keys[-1][f] for f in FIELDS]
    b = next(i for i, key in enumerate(keys) if key["t_min"] * TICKS_PER_MINUTE > t)
    ka, kb = keys[b - 1], keys[b]
    ta, tb = ka["t_min"] * TICKS_PER_MINUTE, kb["t_min"] * TICKS_PER_MINUTE
    return [ka[f] + int(Fraction((kb[f] - ka[f]) * (t - ta), tb - ta)) for f in FIELDS]


def sample_ticks(data):
    base = data["start_minute"] * TICKS_PER_MINUTE
    keys = [k["t_min"] * TICKS_PER_MINUTE for k in data["key"]]
    ticks = set()
    for ta, tb in zip(keys, keys[1:]):
        d = tb - ta
        ticks.update((ta, ta + 1, ta + d // 4, ta + d // 2, ta + 3 * d // 4, tb - 1))
    ticks.update((keys[-1], keys[-1] + 1, keys[-1] + 2))
    return sorted(t - base for t in ticks if t >= base)


def main():
    data = load()
    for k in sample_ticks(data):
        print("now,%d,%s" % (k, ",".join(str(v) for v in at(data, k))))


if __name__ == "__main__":
    main()
