#!/usr/bin/env python3
"""Independent reference for SPEC-012 acceptance tests T12-03 and T12-05 (flat-ground propagation).

Plain Python with `math` and `float`; it shares no code with the Rust crate and exists only to
produce expected values that the Rust tests compare against. The Rust code itself uses no
floating point. Every constant is read from data/rules/acoustics.toml.

The source stands at the origin and the listener at (distance, 0), so the along-wind speed is the
x component of the wind. Linear terms that the spec defines as truncating integer divisions are
computed with the same truncation; only the logarithms and square roots are evaluated in float.
The sound-speed law, the along-wind speed and the gradient are the spec's truncating integer
formulas, so the comparison isolates the logarithm and square-root work.

Per row (distance d in mm, heights hs and hr in mm):
  c      = c0 + c_per_k * temp_dc / 10                    (truncating)
  w      = wind_x                                         (listener on the +x axis)
  delay  = ceil(d * 1000 / max(c + w, c / 2))
  g      = lapse_mk * c_per_k / 1000 + w * shear_ppm / 10^6   (each truncating)
  d_h    = sqrt(2 R hs) + sqrt(2 R hr), R = c * 1000 / |g|, only when |g| >= g_min
  loss_b = spreading + absorption + shadow, as in SPEC-012 section 6 item 4 (flat ground only)

Output (CSV, integers only): case, weather, distance_mm, hs_mm, hr_mm, delay_ms, then the loss in
cdB for the low, mid and high band.

The committed file qc/fixtures/ref_acoustics.csv was produced with
    python3 qc/fixtures/ref_acoustics.py > qc/fixtures/ref_acoustics.csv
CI does not run this script; it runs only the Rust tests, which read the committed output.
"""
import math
import os
import tomllib


def tdiv(a, b):
    """Integer division truncating toward zero."""
    q = abs(a) // abs(b)
    return q if (a < 0) == (b < 0) or q == 0 else -q


def load():
    root = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    with open(os.path.join(root, "data", "rules", "acoustics.toml"), "rb") as f:
        return tomllib.load(f)


DATA = load()
G = {}
for section in DATA["globals"].values():
    G.update({k: v for k, v in section.items() if isinstance(v, int) and not isinstance(v, bool)})
BANDS = DATA["band"]
WEATHER = {w["id"]: w for w in DATA["weather"]}


def sphere(x_mm):
    """Spherical spreading, cdB: 20 log10 of the distance in metres, never negative."""
    return max(0.0, 2000.0 * math.log10(x_mm / 1000.0))


def row(case, wid, d, hs, hr):
    w_ = WEATHER[wid]
    c = G["c0_mm_s"] + tdiv(G["c_per_k_mm_s"] * w_["temp_dc"], 10)
    w = max(-100_000, min(100_000, tdiv(w_["wind_x_mm_s"] * d, d)))
    c_eff = max(c + w, c // 2)
    delay = -((-d * 1000) // c_eff)
    g = tdiv(w_["lapse_mk_per_m"] * G["c_per_k_mm_s"], 1000) + tdiv(w * G["shear_ppm"], 1_000_000)
    d_h = None
    if abs(g) >= G["g_min"]:
        r = c * 1000 // abs(g)
        d_h = math.sqrt(2 * r * hs) + math.sqrt(2 * r * hr)
    losses = []
    for b in BANDS:
        if g > 0 and d_h is not None and d > d_h:
            dh = max(d_h, 1000.0)
            spread = sphere(dh) + 1000.0 * math.log10(d / dh)
            spread += math.floor(G["duct_loss_cdb_per_km"] * (d - d_h) / 1e6)
        else:
            spread = sphere(d)
        loss = spread + (b["abs_cdb_per_km"] * d) // 1_000_000
        if g < 0 and d_h is not None and d > d_h:
            cap = b["shadow_cap_cdb"] * min(abs(g), G["shadow_sat_g"]) // G["shadow_sat_g"]
            slope = b["shadow_slope_cdb_per_km"] * int(d - int(d_h)) // 1_000_000
            loss += min(cap, slope)
        losses.append(int(round(loss)))
    return [case, wid, d, hs, hr, delay] + losses


def distances(lo_m, hi_m, n):
    step = math.log10(hi_m / lo_m) / (n - 1)
    return [int(round(1000 * lo_m * 10 ** (i * step))) for i in range(n)]


def main():
    print("case,weather,distance_mm,hs_mm,hr_mm,delay_ms,loss_low_cdb,loss_mid_cdb,loss_high_cdb")
    rows = []
    for d in distances(1, 50_000, 100):
        rows.append(row("flat", "calm", d, 1500, 1500))
    for hs, hr in ((1500, 1500), (300, 1600)):
        for d in distances(100, 50_000, 25):
            rows.append(row("duct", "toward_plus_x_inversion", d, hs, hr))
            rows.append(row("shadow", "toward_minus_x", d, hs, hr))
        for d in distances(100, 50_000, 15):
            rows.append(row("shadow_weak", "calm_sunny", d, hs, hr))
    for r in rows:
        print(",".join(str(v) for v in r))


if __name__ == "__main__":
    main()
