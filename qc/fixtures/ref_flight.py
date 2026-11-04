#!/usr/bin/env python3
"""Independent reference integrator for SPEC-005 acceptance test T5-05.

Plain Python floats, classical fourth-order Runge-Kutta, step 0.0005 s. It shares no code with
the Rust flight module and exists only to produce expected values that the Rust tests compare
against (tolerance 0.5 percent); the Rust code itself uses no floating point.

Model: quadratic drag against the air, a = -k |v - w| (v - w) - (0, 0, g), with
k = rho * cd * (pi d^2 / 4) / (2 m) and rho, g from the `[constants]` table of data/rules/ammo.toml.
Every ammunition row is fired along +x from a height of 1.2 m at 1, 5 and 10 degrees with no wind,
a 5 m/s cross wind (blowing toward +y) and a 3 m/s head wind (blowing toward -x). The flight ends
at the first contact with the ground (z = 0), found by linear interpolation inside the last step.

Output (CSV, integers only): ammo, elevation in degrees, wind case, horizontal range from the
muzzle in mm, sideways drift in mm, time of flight in microseconds, speed at impact in mm/s.

The committed file qc/fixtures/ref_flight.csv was produced with
    python3 qc/fixtures/ref_flight.py > qc/fixtures/ref_flight.csv
CI does not run this script; it runs only the Rust tests, which read the committed output.
"""
import math
import os
import sys
import tomllib

DT = 0.0005
LAUNCH_HEIGHT_M = 1.2
ELEVATIONS_DEG = (1, 5, 10)
WINDS = (("none", (0.0, 0.0)), ("cross", (0.0, 5.0)), ("head", (-3.0, 0.0)))


def load_ammo():
    root = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    with open(os.path.join(root, "data", "rules", "ammo.toml"), "rb") as f:
        data = tomllib.load(f)
    c = data["constants"]
    rho = c["air_density_g_m3"] / 1000.0
    g = c["gravity_um_s2"] / 1.0e6
    rows = []
    for r in data["ammo"]:
        d = r["diameter_mm"] / 1000.0
        m = r["mass_g"] / 1000.0
        cd = r["cd_pm"] / 1000.0
        k = rho * cd * (math.pi * d * d / 4.0) / (2.0 * m)
        rows.append((r["id"], k, r["muzzle_mm_s"] / 1000.0))
    return g, rows


def deriv(s, k, g, w):
    x, y, z, vx, vy, vz = s
    rx, ry, rz = vx - w[0], vy - w[1], vz
    q = k * math.sqrt(rx * rx + ry * ry + rz * rz)
    return (vx, vy, vz, -q * rx, -q * ry, -q * rz - g)


def axpy(s, d, h):
    return tuple(a + h * b for a, b in zip(s, d))


def rk4(s, k, g, w):
    k1 = deriv(s, k, g, w)
    k2 = deriv(axpy(s, k1, DT / 2), k, g, w)
    k3 = deriv(axpy(s, k2, DT / 2), k, g, w)
    k4 = deriv(axpy(s, k3, DT), k, g, w)
    return tuple(
        a + DT / 6.0 * (b + 2.0 * c + 2.0 * d + e)
        for a, b, c, d, e in zip(s, k1, k2, k3, k4)
    )


def fly(k, g, w, speed, elev_deg):
    e = math.radians(elev_deg)
    s = (0.0, 0.0, LAUNCH_HEIGHT_M, speed * math.cos(e), 0.0, speed * math.sin(e))
    t = 0.0
    while True:
        n = rk4(s, k, g, w)
        if n[2] <= 0.0:
            f = s[2] / (s[2] - n[2])
            c = tuple(a + f * (b - a) for a, b in zip(s, n))
            t += f * DT
            return c, t
        s = n
        t += DT


def main():
    g, rows = load_ammo()
    print("ammo,elev_deg,wind,range_mm,drift_mm,tof_us,impact_mm_s")
    for ammo_id, k, speed in rows:
        for elev in ELEVATIONS_DEG:
            for name, w in WINDS:
                c, t = fly(k, g, w, speed, elev)
                rng = math.hypot(c[0], c[1])
                v = math.sqrt(c[3] ** 2 + c[4] ** 2 + c[5] ** 2)
                print(
                    f"{ammo_id},{elev},{name},{round(rng * 1000)},{round(c[1] * 1000)},"
                    f"{round(t * 1e6)},{round(v * 1000)}"
                )


if __name__ == "__main__":
    sys.exit(main())
