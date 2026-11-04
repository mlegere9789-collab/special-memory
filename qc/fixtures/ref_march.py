#!/usr/bin/env python3
"""Independent reference marcher for SPEC-004 acceptance tests T4-03 and T4-04.

Plain Python with `fractions.Fraction` and integers; it shares no code with the Rust movement
module and exists only to produce expected values that the Rust tests compare against. The Rust
code itself uses no floating point.

Model: the regiment anchor travels a straight line at the pace of its most tired soldier, which
is the front rank (it enters rough ground first and tires first). Each tick, with the soldier's
fatigue f in ppm (integer):
    speed (mm/s) = gait speed * terrain speed_pm / 1000 * fatigue_speed_pm(f) / 1000
    the anchor advances speed / 20 mm, or the rest of the distance when that is shorter
    f += drain_ppm_per_tick * effort_pm // 1000, capped at 1000000
`fatigue_speed_pm` interpolates the curve of data/rules/locomotion.toml with integer division
truncating toward zero. Terrain is open ground except, for the strip case, woods where the anchor
x lies in [80000, 120000) mm. All rows come from data/rules/locomotion.toml and terrain.toml.

Output (CSV, integers only): case, gait, distance in mm, ideal ticks, model ticks.
  march rows: ideal = ceil(distance * 20 / speed), model = ticks until the anchor arrives.
  strip row:  distance = strip length, ideal = ceil(length / (speed * 6/10) * 20), model = ticks
              the anchor spends inside the strip while marching at quick time.

The committed file qc/fixtures/ref_march.csv was produced with
    python3 qc/fixtures/ref_march.py > qc/fixtures/ref_march.csv
CI does not run this script; it runs only the Rust tests, which read the committed output.
"""
import math
import os
import tomllib
from fractions import Fraction

MARCH_MM = 200_000
STRIP_MM = (80_000, 120_000)


def load():
    root = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    with open(os.path.join(root, "data", "rules", "locomotion.toml"), "rb") as f:
        loco = tomllib.load(f)
    with open(os.path.join(root, "data", "rules", "terrain.toml"), "rb") as f:
        terrain = tomllib.load(f)
    return loco, {c["name"]: c for c in terrain["class"]}


def fatigue_speed_pm(f, curve):
    for (f0, p0), (f1, p1) in zip(curve, curve[1:]):
        if f <= f1:
            return p0 + int(Fraction((p1 - p0) * (f - f0), f1 - f0))
    return curve[-1][1]


def march(gait, loco, classes, strip):
    row = {g["id"]: g for g in loco["gait"]}[gait]
    curve = [(p["fatigue_ppm"], p["speed_pm"]) for p in loco["fatigue_curve"]]
    f, x, ticks, inside = 0, Fraction(0), 0, 0
    while x < MARCH_MM:
        woods = strip and STRIP_MM[0] <= x < STRIP_MM[1]
        cls = classes["woods" if woods else "open"]
        inside += 1 if woods else 0
        speed = Fraction(row["speed_mm_s"] * cls["speed_pm"] * fatigue_speed_pm(f, curve), 10**6)
        x += min(speed / 20, MARCH_MM - x)
        f = min(1_000_000, f + row["drain_ppm_per_tick"] * cls["effort_pm"] // 1000)
        ticks += 1
    return ticks, inside, row["speed_mm_s"]


def main():
    loco, classes = load()
    print("case,gait,distance_mm,ideal_ticks,model_ticks")
    for gait in ("walk", "quick", "double_quick", "run"):
        ticks, _, speed = march(gait, loco, classes, False)
        ideal = math.ceil(Fraction(MARCH_MM * 20, speed))
        print(f"march,{gait},{MARCH_MM},{ideal},{ticks}")
    _, inside, speed = march("quick", loco, classes, True)
    length = STRIP_MM[1] - STRIP_MM[0]
    wood = classes["woods"]["speed_pm"]
    ideal = math.ceil(Fraction(length * 20 * 1000, speed * wood))
    print(f"strip,quick,{length},{ideal},{inside}")


if __name__ == "__main__":
    main()
