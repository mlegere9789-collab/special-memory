#!/usr/bin/env python3
"""Independent reference for SPEC-009 acceptance test T9-03 (the optical-depth table).

Plain Python with `decimal` at 50 digits; it shares no code with the Rust environment module and
exists only to produce expected values that the Rust tests compare against. The Rust code itself
uses no floating point.

Output is CSV, the first column names the case:
  table,<i>,<value>
      value = round(10^6 * exp(-i/8)) for i = 0 ..= 128, the contents of `EXP_TABLE`.
  sample,<tau_micro>,<interp_ppm>,<exact_ppm>
      1000 optical depths tau_micro = 16009 * k (k = 0 ..= 999). interp_ppm is the piecewise
      linear interpolation of the exact nodes at tau_micro / 125000, rounded; exact_ppm is
      round(10^6 * exp(-tau_micro / 10^6)).

The committed file qc/fixtures/ref_exp.csv was produced with
    python3 qc/fixtures/ref_exp.py > qc/fixtures/ref_exp.csv
CI does not run this script; it runs the Rust tests, which read the committed output, and
tools/test_ref_environment.py, which regenerates it and compares.
"""
from decimal import Decimal, ROUND_HALF_EVEN, getcontext

getcontext().prec = 50
STEP = 125000  # tau_micro per table entry (tau = 1/8)
SAMPLES = 1000


def node(i):
    return (Decimal(10) ** 6) * (-Decimal(i) / 8).exp()


def rnd(x):
    return int(x.quantize(Decimal(1), rounding=ROUND_HALF_EVEN))


def table():
    return [rnd(node(i)) for i in range(129)]


def samples():
    for k in range(SAMPLES):
        tau = 16009 * k
        i, frac = divmod(tau, STEP)
        lo, hi = node(i), node(i + 1)
        interp = lo + (hi - lo) * Decimal(frac) / STEP
        exact = (Decimal(10) ** 6) * (-Decimal(tau) / 10**6).exp()
        yield tau, rnd(interp), rnd(exact)


def main():
    for i, v in enumerate(table()):
        print("table,%d,%d" % (i, v))
    for tau, interp, exact in samples():
        print("sample,%d,%d,%d" % (tau, interp, exact))


if __name__ == "__main__":
    main()
