#!/usr/bin/env python3
"""Independent reference for SPEC-010 acceptance test T10-08 (the lookahead budget rule).

Plain Python with `fractions`; it shares no code with the Rust crate and only produces the
expected values that the Rust test compares against. The rule (SPEC-010 section 7):

    horizon = clamp(floor(budget / (candidates * samples * soldiers)), 100, 1200)

and, when the quotient is below 100, samples are dropped one at a time down to 1 and then
candidates down to 1 (always from the end of the list) until it reaches 100 or nothing is left to
drop; the horizon is then clamped. A tactical pass uses a tenth of the budget (floor) and at most
2 candidates.

Output is CSV, one line per case:
  plan,<budget>,<candidates>,<samples>,<soldiers>,<horizon>,<candidates_used>,<samples_used>
  tactical,<budget>,<candidates>,<samples>,<soldiers>,<horizon>,<candidates_used>,<samples_used>
      for `tactical` the first column of numbers is the operational budget the tenth is taken of.

The committed file qc/fixtures/ref_budget.csv was produced with
    python3 qc/fixtures/ref_budget.py > qc/fixtures/ref_budget.csv
tools/test_ref_budget.py regenerates it and compares; the Rust test reads the committed output.
"""
from fractions import Fraction
import math

LO, HI = 100, 1200

PLANS = [
    (12_000_000, 4, 3, 3000),
    (6_000_000, 3, 2, 1500),
    (2_000_000, 2, 1, 3000),
    (12_000_000, 4, 3, 500),
    (2_000_000, 2, 1, 30_000),
    (6_000_000, 3, 2, 50_000),
    (1_000_000, 4, 3, 1),
    (0, 2, 1, 100),
    (12_000_000, 4, 3, 100),
    (999_999, 1, 1, 1000),
]
TACTICAL = [
    (12_000_000, 4, 3, 1000),
    (6_000_000, 3, 2, 300),
    (2_000_000, 2, 1, 2500),
    (12_000_000, 4, 3, 40_000),
]


def floor_div(a, b):
    return math.floor(Fraction(a, b))


def plan(budget, candidates, samples, soldiers):
    soldiers = max(soldiers, 1)
    c, s = candidates, samples
    while floor_div(budget, c * s * soldiers) < LO and (s > 1 or c > 1):
        if s > 1:
            s -= 1
        else:
            c -= 1
    h = min(max(floor_div(budget, c * s * soldiers), LO), HI)
    return h, c, s


def main():
    for b, c, s, n in PLANS:
        h, cu, su = plan(b, c, s, n)
        print("plan,%d,%d,%d,%d,%d,%d,%d" % (b, c, s, n, h, cu, su))
    for b, c, s, n in TACTICAL:
        h, cu, su = plan(floor_div(b, 10), min(c, 2), s, n)
        print("tactical,%d,%d,%d,%d,%d,%d,%d" % (b, c, s, n, h, cu, su))


if __name__ == "__main__":
    main()
