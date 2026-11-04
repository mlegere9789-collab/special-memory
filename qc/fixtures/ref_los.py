#!/usr/bin/env python3
"""Independent reference for SPEC-014 acceptance test T14-05 (line of sight on the synthetic pack).

Plain Python with integers and `fractions.Fraction`; it shares no code with the Rust `terrain`
crate and exists only to produce expected values that the Rust tests compare against.

Synthetic pack: 3 x 2 tiles of 128 x 128 cells (cell 4000 mm), `z_origin_cm = 0`, all functions of
global coordinates:
    tri(v, p)          = min(v % p, p - v % p)
    vertex_cm(vx, vy)  = 3000 + 30*tri(vx, 56) + 25*tri(vy, 44) + ((vx*7 + vy*13) % 11)*4
    occ_dm(cx, cy)     = 180 if (cx//12 + cy//9) % 6 == 0, else 12 if (cx*5 + cy*3) % 13 == 0, else 0
plus the obstacle segments of `ref_los_obstacles.csv` (this script prints it): columns seg_id, kind,
flags, height_mm, ax, ay, bx, by; flags bit0 = blocks fire, bit1 = blocks sight.

Rule (SPEC-014 section 5, "Line of sight"), restated:
  * D = isqrt(dx^2 + dy^2), n = max(1, ceil(D / 2000)); A0, A1 = ground + eye at both ends.
  * For k = 1..n-1 the sample p_k = from + floor(d*k/n) per axis and ray_k = A0 + floor((A1-A0)*k/n).
    Ground blocks if height(p_k) > ray_k; in sight mode a cover block also blocks if
    height(p_k) + 100*occ(cell) > ray_k, except in the cells of the two end points.
  * An obstacle of a blocking kind that meets the ray at rational t (closed segments; for a
    collinear overlap, the first shared point) blocks if floor(A0 + (A1-A0)*t) <= height(floor of
    the meeting point) + height_mm.
  * The nearest blocker wins: the smallest t among obstacles (ties to the lower seg_id), against
    the first blocking ground sample at t = k/n, the obstacle first on a tie.
  * blocked_at_mm is floor(k*D/n) for ground and cover, floor(D*t) for an obstacle; 0 when clear.
  * Positions are clamped to the extent only inside height and cover lookups.

Pairs: 1000 seeded (SplitMix64, seed 143), each in both modes, so 2000 rows.
Output (CSV, integers only): fx, fy, fz, tx, ty, tz, mode (0 fire, 1 sight), clear (0/1),
blocked_at_mm, blocker (0 none, 1 ground, 2 cover, 3 obstacle), seg_id (0 unless blocker is 3).

The committed files were produced with
    python3 qc/fixtures/ref_los.py obstacles > qc/fixtures/ref_los_obstacles.csv
    python3 qc/fixtures/ref_los.py > qc/fixtures/ref_los.csv
CI does not run this script; it runs only the Rust tests, which read the committed output.
"""
import sys
from fractions import Fraction
from math import isqrt

C = 4000
TILE_MM = 128 * C
EXT_X, EXT_Y = 3 * TILE_MM, 2 * TILE_MM
CELLS_X, CELLS_Y = 3 * 128, 2 * 128
MASK = (1 << 64) - 1


def tri(v, p):
    return min(v % p, p - v % p)


def vertex_cm(vx, vy):
    return 3000 + 30 * tri(vx, 56) + 25 * tri(vy, 44) + ((vx * 7 + vy * 13) % 11) * 4


def occ_dm(cx, cy):
    if (cx // 12 + cy // 9) % 6 == 0:
        return 180
    if (cx * 5 + cy * 3) % 13 == 0:
        return 12
    return 0


def cell_of(x, y):
    x = min(max(x, 0), EXT_X)
    y = min(max(y, 0), EXT_Y)
    return min(x // C, CELLS_X - 1), min(y // C, CELLS_Y - 1)


def ground_mm(x, y):
    x = min(max(x, 0), EXT_X)
    y = min(max(y, 0), EXT_Y)
    cx, cy = cell_of(x, y)
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


KINDS = [  # kind, height_mm (0 = per segment), flags
    (1, 1000, 1),  # stone wall: stops fire
    (2, 1300, 0),  # rail fence: neither
    (3, 1500, 0),  # board fence: neither
    (4, 1500, 2),  # hedge: stops sight
    (5, 1100, 1),  # earthwork: stops fire
    (6, 0, 3),  # building wall: both
]


def obstacles():
    r = SplitMix64(1430)
    out = []
    for i in range(1, 91):
        kind, height, flags = KINDS[r.below(len(KINDS))]
        if height == 0:
            height = 3000 + r.below(6001)
        ax, ay = r.below(EXT_X + 1), r.below(EXT_Y + 1)
        length = 5000 + r.below(300_001)
        shape = r.below(4)
        if shape == 0:
            dx, dy = length, 0
        elif shape == 1:
            dx, dy = 0, length
        else:
            dx, dy = r.below(2 * length + 1) - length, r.below(2 * length + 1) - length
        bx = min(max(ax + dx, 0), EXT_X)
        by = min(max(ay + dy, 0), EXT_Y)
        if (ax, ay) == (bx, by):
            bx = ax + 1 if ax < EXT_X else ax - 1
        out.append((i * 1024, kind, flags, height, ax, ay, bx, by))
    return out


def cross(ax, ay, bx, by):
    return ax * by - ay * bx


def meet(f, t, seg):
    """Smallest t in [0, 1] (a Fraction) where the ray f->t meets the segment, or None."""
    (fx, fy), (tx, ty) = f, t
    ax, ay, bx, by = seg
    rx, ry = tx - fx, ty - fy
    sx, sy = bx - ax, by - ay
    if rx == 0 and ry == 0:
        return None
    qx, qy = ax - fx, ay - fy
    den = cross(rx, ry, sx, sy)
    if den == 0:
        if cross(qx, qy, rx, ry) != 0:
            return None
        rr = rx * rx + ry * ry
        ta = Fraction(qx * rx + qy * ry, rr)
        tb = Fraction((bx - fx) * rx + (by - fy) * ry, rr)
        lo, hi = max(Fraction(0), min(ta, tb)), min(Fraction(1), max(ta, tb))
        return lo if lo <= hi else None
    tt = Fraction(cross(qx, qy, sx, sy), den)
    uu = Fraction(cross(qx, qy, rx, ry), den)
    if 0 <= tt <= 1 and 0 <= uu <= 1:
        return tt
    return None


def los(f, fz, t, tz, sight, segs):
    dx, dy = t[0] - f[0], t[1] - f[1]
    d = isqrt(dx * dx + dy * dy)
    n = max(1, -(-d // 2000))
    a0, a1 = ground_mm(*f) + fz, ground_mm(*t) + tz
    cf, ct = cell_of(*f), cell_of(*t)
    ground = None  # (k, blocker kind)
    for k in range(1, n):
        px, py = f[0] + (dx * k) // n, f[1] + (dy * k) // n
        ray = a0 + ((a1 - a0) * k) // n
        g = ground_mm(px, py)
        if g > ray:
            ground = (k, 1)
            break
        if sight:
            c = cell_of(px, py)
            if c != cf and c != ct and g + 100 * occ_dm(*c) > ray:
                ground = (k, 2)
                break
    best = None  # (t, seg_id, height)
    bit = 2 if sight else 1
    for sid, _kind, flags, height, ax, ay, bx, by in segs:
        if not flags & bit:
            continue
        tt = meet(f, t, (ax, ay, bx, by))
        if tt is None:
            continue
        ix = f[0] + (dx * tt).__floor__()
        iy = f[1] + (dy * tt).__floor__()
        if a0 + ((a1 - a0) * tt).__floor__() <= ground_mm(ix, iy) + height:
            if best is None or (tt, sid) < (best[0], best[1]):
                best = (tt, sid)
    if best is not None and (ground is None or best[0] <= Fraction(ground[0], n)):
        return 0, (d * best[0]).__floor__(), 3, best[1]
    if ground is not None:
        return 0, (ground[0] * d) // n, ground[1], 0
    return 1, 0, 0, 0


def pairs():
    r = SplitMix64(143)
    eyes = [0, 450, 1000, 1700, 2500]
    scales = [3_000, 30_000, 120_000, 400_000]
    out = []
    for _ in range(1000):
        fx = r.below(EXT_X + 100_001) - 50_000
        fy = r.below(EXT_Y + 100_001) - 50_000
        s = scales[r.below(len(scales))]
        tx = fx + r.below(2 * s + 1) - s
        ty = fy + r.below(2 * s + 1) - s
        out.append(((fx, fy), eyes[r.below(5)], (tx, ty), eyes[r.below(5)]))
    return out


if __name__ == "__main__":
    segs = obstacles()
    if sys.argv[1:] == ["obstacles"]:
        print("seg_id,kind,flags,height_mm,ax,ay,bx,by")
        for row in segs:
            print(",".join(str(v) for v in row))
    else:
        print("fx,fy,fz,tx,ty,tz,mode,clear,blocked_at_mm,blocker,seg_id")
        for f, fz, t, tz in pairs():
            for mode in (0, 1):
                res = los(f, fz, t, tz, mode == 1, segs)
                row = (f[0], f[1], fz, t[0], t[1], tz, mode) + res
                print(",".join(str(v) for v in row))
