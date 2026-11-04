#!/usr/bin/env python3
"""Independent reference values for SPEC-006 acceptance tests T6-02, T6-04 and the `lerp` helper.

Plain Python with `fractions.Fraction` and integers; it shares no code with the Rust morale
module and exists only to produce expected values that the Rust tests compare against. The Rust
code itself uses no floating point.

Output is CSV; the first column names the case and the remaining columns depend on it:
  shock,<training>,<s0>,<cap_pm>,<shock_unit>,<killed>,<wounded>,<disabled>,<officer>,<colonel>
      shock_unit = ceil(10^9 / (s0 * cap_pm)) as an exact fraction; each kind is
      shock_unit * weight_pm / 1000 truncated, with the weights of data/rules/morale.toml.
  exposure,<bearing_deg>,<f>,<r>,<class>
      An enemy at the bearing, measured from the facing direction, at distance 10^6 units:
      f = round(10^6 cos), r = round(10^6 |sin|). The class comes from the bearing alone, with
      the boundaries of SPEC-006 section 5.2 written as angles: front below 60, flank from 60 to
      70, enfilade from 70 to 110, flank from 110 to 135, rear from 135 (135 itself is flank).
  lerp,<c>,<c_hi>,<c_lo>,<v_hi>,<v_lo>,<value>
      v_hi + trunc((v_lo - v_hi) * (c_hi - c) / (c_hi - c_lo)) with c clamped to [c_lo, c_hi].

The committed file qc/fixtures/ref_morale.csv was produced with
    python3 qc/fixtures/ref_morale.py > qc/fixtures/ref_morale.csv
CI does not run this script; it runs only the Rust tests, which read the committed output.
"""
import math
import os
import tomllib
from fractions import Fraction

S0_LIST = (1, 40, 400, 1000, 2000)
BEARINGS = (0, 30, 59, 60, 61, 69, 70, 71, 90, 100, 105, 109, 110, 111, 115, 134, 135, 136, 150, 180)


def load():
    root = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    with open(os.path.join(root, "data", "rules", "morale.toml"), "rb") as f:
        return tomllib.load(f)


def ceil_fraction(x):
    return -((-x.numerator) // x.denominator)


def shock_rows(data):
    g = data["globals"]
    for row in data["training"]:
        for s0 in S0_LIST:
            cap = row["cap_pm"]
            unit = ceil_fraction(Fraction(10**9, s0 * cap))
            kinds = [unit * g[k] // 1000 for k in ("w_killed_pm", "w_wounded_pm", "w_disabled_pm", "w_officer_pm", "w_colonel_pm")]
            yield ["shock", row["id"], s0, cap, unit] + kinds


def exposure_class(bearing):
    if bearing < 60:
        return "front"
    if bearing < 70:
        return "flank"
    if bearing <= 110:
        return "enfilade"
    if bearing <= 135:
        return "flank"
    return "rear"


def exposure_rows():
    for b in BEARINGS:
        a = math.radians(b)
        yield ["exposure", b, round(10**6 * math.cos(a)), round(10**6 * abs(math.sin(a))), exposure_class(b)]


def lerp(c, c_hi, c_lo, v_hi, v_lo):
    c = max(c_lo, min(c_hi, c))
    q = Fraction((v_lo - v_hi) * (c_hi - c), c_hi - c_lo)
    return v_hi + int(q)  # int() truncates toward zero


def lerp_rows(data):
    g = data["globals"]
    steady, fall = g["steady"], g["fall"]
    for c in (fall - 5000, fall, 400000, 500000, 512345, 600000, 679999, steady, steady + 5000):
        yield ["lerp", c, steady, fall, 1000, 550, lerp(c, steady, fall, 1000, 550)]
        yield ["lerp", c, steady, fall, 1000, 1500, lerp(c, steady, fall, 1000, 1500)]
    for c, hi, lo, vh, vl in ((7, 10, 0, 0, 3), (7, 10, 0, 3, 0), (-3, 10, 0, 100, 0), (5, 10, 0, -1, 1)):
        yield ["lerp", c, hi, lo, vh, vl, lerp(c, hi, lo, vh, vl)]


def main():
    data = load()
    print("case,a,b,c,d,e,f,g,h,i")
    for rows in (shock_rows(data), exposure_rows(), lerp_rows(data)):
        for r in rows:
            print(",".join(str(x) for x in r))


if __name__ == "__main__":
    main()
