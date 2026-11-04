#!/usr/bin/env python3
"""Independent reference for SPEC-007 acceptance test T7-03 (estimated arrival of a message).

Plain Python with `fractions.Fraction` and integers; it shares no code with the Rust command
module and exists only to produce expected values that the Rust tests compare against. The Rust
code itself uses no floating point.

Model (SPEC-007 section 5), all inputs from data/rules/command.toml and terrain.toml:
  D            = floor(length of the fixed-point difference vector * 1000) in millimetres, the length
                 being the floor square root of the sum of squares of the raw `Fx` components
  m            = max(min_terrain_pm, floor(sum of speed_pm at the eight points
                 from + (to - from) * (2i + 1) / 16, i = 0..7, divided by 8)); a point's cell is
                 floor((p - origin) / cell) per axis, clamped to the grid; the raw offset is
                 floor(delta * (2i + 1) / 16)
  courier      eta_lo = t0 + write + base + ceil(D * path_factor_pm * 20 / (speed_mm_s * m))
  flags        eta_lo = t0 + write + base + per_word * words
  telegraph    eta_lo = t0 + write + base + per_word * words
  eta_hi       = eta_lo + jitter_ticks (+ smoke_max_waits * smoke_wait_ticks for flags)

Output (CSV, integers only). First the terrain, one row per grid row from the south, then the
legend, then the cases:
  cell,<cell_mm>
  terrain,<row text>
  legend,<character>,<class id>
  case,<medium>,<t0>,<from_x_m>,<from_y_m>,<to_x_m>,<to_y_m>,<words>,<eta_lo>,<eta_hi>

The committed file qc/fixtures/ref_command.out was produced with
    python3 qc/fixtures/ref_command.py > qc/fixtures/ref_command.out
CI does not run this script; it runs only the Rust tests, which read the committed output.
"""
import math
import os
import tomllib
from fractions import Fraction

CELL_MM = 100_000
ROWS = 8
PATTERN = ".rw.mc..wr..c.mw"
LEGEND = {".": "open", "r": "road", "w": "woods", "m": "marsh", "c": "crop"}
ONE = 1 << 32

# (t0, from_x_m, from_y_m, to_x_m, to_y_m, words)
COURIER = [
    (0, 0, 0, 3000, 0, 4),
    (0, 100, 50, 100, 3050, 5),
    (120, 400, 300, 3400, 4300, 12),
    (0, 0, 0, 1000, 1000, 4),
    (50, 2000, 700, 500, 1100, 12),
    (0, 640, 0, 3200, 0, 5),
    (7, 10, 10, 5000, 500, 12),
    (0, 3200, 3200, 3200, 3300, 4),
    (1000, 1500, 2500, 4500, 2500, 5),
    (0, 6000, 100, 100, 6000, 12),
]
FLAGS = [
    (0, 0, 0, 3000, 0, 4),
    (0, 100, 50, 2100, 50, 5),
    (30, 400, 300, 1400, 1300, 12),
    (0, 0, 0, 100, 100, 4),
    (0, 2000, 2000, 6000, 2000, 12),
    (90, 500, 500, 500, 6500, 5),
    (0, 1000, 1000, 1000, 1000, 4),
    (5, 6400, 6400, 6400, 0, 12),
    (0, 3000, 100, 3000, 4100, 5),
    (200, 0, 0, 5000, 5000, 12),
]
TELEGRAPH = [
    (0, 0, 0, 1000, 0, 4),
    (0, 0, 0, 10000, 0, 4),
    (10, 100, 50, 2100, 50, 5),
    (0, 400, 300, 1400, 1300, 12),
    (0, 0, 0, 100, 100, 4),
    (0, 2000, 2000, 6000, 2000, 12),
    (90, 500, 500, 500, 6500, 5),
    (0, 1000, 1000, 1000, 1000, 5),
    (5, 6400, 6400, 6400, 0, 12),
    (200, 0, 0, 50000, 50000, 12),
]


def load():
    root = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    with open(os.path.join(root, "data", "rules", "command.toml"), "rb") as f:
        command = tomllib.load(f)
    with open(os.path.join(root, "data", "rules", "terrain.toml"), "rb") as f:
        terrain = tomllib.load(f)
    return command, {c["name"]: c for c in terrain["class"]}


def terrain_rows():
    n = len(PATTERN)
    return ["".join(PATTERN[(x + 3 * y) % n] for x in range(64)) for y in range(ROWS)]


def cell_class(rows, legend_class, x_raw, y_raw):
    cx = min(max(math.floor(Fraction(x_raw * 1000, ONE * CELL_MM)), 0), len(rows[0]) - 1)
    cy = min(max(math.floor(Fraction(y_raw * 1000, ONE * CELL_MM)), 0), len(rows) - 1)
    return legend_class[rows[cy][cx]]


def distance_mm(dx, dy):
    length = math.isqrt(dx * dx + dy * dy)
    return (length * 1000) >> 32


def main():
    command, classes = load()
    g = command["globals"]
    media = {m["id"]: m for m in command["medium"]}
    rows = terrain_rows()
    legend_class = {ch: classes[name]["id"] for ch, name in LEGEND.items()}
    speed = {c["id"]: c["speed_pm"] for c in classes.values()}

    print(f"cell,{CELL_MM}")
    for row in rows:
        print(f"terrain,{row}")
    for ch, cls in legend_class.items():
        print(f"legend,{ch},{cls}")

    def courier_lo(t0, fx, fy, tx, ty):
        m = media["courier"]
        x0, y0, x1, y1 = (v * ONE for v in (fx, fy, tx, ty))
        dx, dy = x1 - x0, y1 - y0
        total = 0
        for i in range(8):
            k = 2 * i + 1
            px, py = x0 + ((dx * k) >> 4), y0 + ((dy * k) >> 4)
            total += speed[cell_class(rows, legend_class, px, py)]
        mean = max(g["min_terrain_pm"], total // 8)
        d = distance_mm(dx, dy)
        leg = -((-Fraction(d * g["path_factor_pm"] * 20, m["speed_mm_s"] * mean)) // 1)
        return t0 + m["write_ticks"] + m["base_ticks"] + int(leg)

    def line_lo(medium, t0, words):
        m = media[medium]
        return t0 + m["write_ticks"] + m["base_ticks"] + m["per_word_ticks"] * words

    for t0, fx, fy, tx, ty, w in COURIER:
        lo = courier_lo(t0, fx, fy, tx, ty)
        print(f"case,courier,{t0},{fx},{fy},{tx},{ty},{w},{lo},{lo + g['jitter_ticks']}")
    smoke = g["smoke_max_waits"] * g["smoke_wait_ticks"]
    for t0, fx, fy, tx, ty, w in FLAGS:
        lo = line_lo("flags", t0, w)
        print(f"case,flags,{t0},{fx},{fy},{tx},{ty},{w},{lo},{lo + g['jitter_ticks'] + smoke}")
    for t0, fx, fy, tx, ty, w in TELEGRAPH:
        lo = line_lo("telegraph", t0, w)
        print(f"case,telegraph,{t0},{fx},{fy},{tx},{ty},{w},{lo},{lo + g['jitter_ticks']}")


if __name__ == "__main__":
    main()
