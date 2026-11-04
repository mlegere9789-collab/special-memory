#!/usr/bin/env python3
"""Independent reference for SPEC-014 acceptance test T14-02 (height queries).

Plain Python with `fractions.Fraction` and integers; it shares no code with the Rust `terrain`
crate and exists only to produce expected values that the Rust tests compare against.

Synthetic pack: 3 x 2 tiles of 128 x 128 cells (cell 4000 mm), `z_origin_cm = 0`. The height of
the vertex (vx, vy), in centimetres, is the integer function
    (vx*131 + vy*197 + ((vx*vy) % 97)*211 + ((vx ^ vy) % 64)*53) % 9000
of its global vertex coordinates, so tile seams agree by construction.

Query rule (SPEC-014 section 5): the position is clamped to the extent, the cell is
`(x // 4000, y // 4000)` clamped to the last cell, `fx, fy` are the offsets inside that cell, and
    h_mm = floor(10 * (h00*(C-fx)*(C-fy) + h10*fx*(C-fy) + h01*(C-fx)*fy + h11*fx*fy) / C^2).

Points (seed 14, SplitMix64): 9000 inside the extent, 500 up to 3 km outside it, 500 on or
within 1 mm of a tile seam or the extent edge.

Output (CSV, integers only): x_mm, y_mm, height_mm.

The committed file qc/fixtures/ref_height.csv was produced with
    python3 qc/fixtures/ref_height.py > qc/fixtures/ref_height.csv
CI does not run this script; it runs only the Rust tests, which read the committed output.
"""
from fractions import Fraction

C = 4000
TILE_MM = 128 * C
TILES_X, TILES_Y = 3, 2
EXT_X, EXT_Y = TILES_X * TILE_MM, TILES_Y * TILE_MM
CELLS_X, CELLS_Y = TILES_X * 128, TILES_Y * 128
MASK = (1 << 64) - 1


def vertex_cm(vx, vy):
    return (vx * 131 + vy * 197 + ((vx * vy) % 97) * 211 + ((vx ^ vy) % 64) * 53) % 9000


def height_mm(x, y):
    x = min(max(x, 0), EXT_X)
    y = min(max(y, 0), EXT_Y)
    cx = min(x // C, CELLS_X - 1)
    cy = min(y // C, CELLS_Y - 1)
    fx, fy = x - cx * C, y - cy * C
    h00, h10 = vertex_cm(cx, cy), vertex_cm(cx + 1, cy)
    h01, h11 = vertex_cm(cx, cy + 1), vertex_cm(cx + 1, cy + 1)
    num = h00 * (C - fx) * (C - fy) + h10 * fx * (C - fy) + h01 * (C - fx) * fy + h11 * fx * fy
    return (10 * Fraction(num, C * C)).__floor__()


class SplitMix64:
    def __init__(self, seed):
        self.s = seed & MASK

    def next(self):
        self.s = (self.s + 0x9E3779B97F4A7C15) & MASK
        z = self.s
        z = ((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9) & MASK
        z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) & MASK
        return z ^ (z >> 31)

    def below(self, n):
        return self.next() % n


def points():
    r = SplitMix64(14)
    out = []
    for _ in range(9000):
        out.append((r.below(EXT_X + 1), r.below(EXT_Y + 1)))
    for _ in range(500):
        out.append((r.below(EXT_X + 6_000_001) - 3_000_000, r.below(EXT_Y + 6_000_001) - 3_000_000))
    for _ in range(500):
        sx = r.below(TILES_X + 1) * TILE_MM + r.below(3) - 1
        sy = r.below(TILES_Y + 1) * TILE_MM + r.below(3) - 1
        if r.below(2) == 0:
            out.append((sx, r.below(EXT_Y + 1)))
        else:
            out.append((r.below(EXT_X + 1), sy))
    return out


if __name__ == "__main__":
    print("x_mm,y_mm,height_mm")
    for px, py in points():
        print(f"{px},{py},{height_mm(px, py)}")
