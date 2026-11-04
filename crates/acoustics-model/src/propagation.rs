//! Delay, gradient, horizon and flat-ground loss (SPEC-012 section 6 items 1 to 5, no terrain).
//!
//! Distances are millimetres, speeds mm/s, gradients mm/s per metre, losses centibels. All
//! arithmetic is integer; divisions truncate unless a function says it rounds up.

use crate::intmath::{isqrt_u128, log2_q16};
use crate::tables::AcousticTables;
use crate::weather::Weather;
use battle_rules::fx_ratio;
use core_math::Angle;

/// Shortest distance used by the model, mm.
const MIN_DISTANCE_MM: i64 = 1000;
/// Largest along-wind speed considered, mm/s.
const MAX_WIND_MM_S: i64 = 100_000;

/// Result of one source-to-listener path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PathResult {
    /// Horizontal distance, mm (at least 1000).
    pub distance_mm: i64,
    /// Travel time, ms.
    pub delay_ms: u64,
    /// Loss in the low, mid and high band, cdB (never negative).
    pub loss_cdb: [i32; 3],
    /// Direction from the listener to the source.
    pub bearing: Angle,
}

/// Speed of sound at `temp_dc` (tenths of a degree C), mm/s.
pub fn sound_speed_mm_s(t: &AcousticTables, temp_dc: i32) -> i64 {
    let g = t.globals();
    g.c0_mm_s + g.c_per_k_mm_s * i64::from(temp_dc) / 10
}

/// Wind component from the source toward the listener, mm/s. `(dx, dy)` points from the source to
/// the listener and `d` is its length.
pub fn along_wind_mm_s(wind: [i32; 2], dx: i64, dy: i64, d: i64) -> i64 {
    if d <= 0 {
        return 0;
    }
    let dot = i128::from(wind[0]) * i128::from(dx) + i128::from(wind[1]) * i128::from(dy);
    let w = (dot / i128::from(d)).clamp(-i128::from(MAX_WIND_MM_S), i128::from(MAX_WIND_MM_S));
    w as i64
}

/// Travel time of sound over `d_mm`, ms, rounded up. The effective speed is `c + w`, never below
/// `c / 2`.
pub fn delay_ms(d_mm: i64, c_mm_s: i64, w_mm_s: i64) -> u64 {
    let c_eff = (c_mm_s + w_mm_s).max(c_mm_s / 2).max(1);
    let d = d_mm.max(0);
    ((i128::from(d) * 1000 + i128::from(c_eff) - 1) / i128::from(c_eff)) as u64
}

/// Sound-speed gradient with height, mm/s per metre: the temperature part plus the wind shear.
pub fn gradient(t: &AcousticTables, lapse_mk_per_m: i32, w_mm_s: i64) -> i64 {
    let g = t.globals();
    i64::from(lapse_mk_per_m) * g.c_per_k_mm_s / 1000 + w_mm_s * g.shear_ppm / 1_000_000
}

/// Distance to the acoustic horizon, mm: the limit of a circular ray over source height `hs_mm`
/// and listener height `hr_mm`, the same for both signs of `g`. `None` when `|g|` is below
/// `g_min` and the path is not refracted.
pub fn horizon_mm(t: &AcousticTables, c_mm_s: i64, g: i64, hs_mm: i64, hr_mm: i64) -> Option<i64> {
    if g.abs() < t.globals().g_min || g == 0 {
        return None;
    }
    let r = i128::from(c_mm_s) * 1000 / i128::from(g.abs());
    let reach = |h: i64| isqrt_u128((2 * r * i128::from(h.max(0))) as u128) as i64;
    Some(reach(hs_mm) + reach(hr_mm))
}

/// `10 log10` of a Q16 power in cdB, rounded to nearest (the floor of `cdb_of_power` would lose
/// up to 1 cdB per term, and a square doubles it).
fn rounded_cdb(p: u128) -> i32 {
    const DEN: i64 = 65_536 * 1_000_000;
    let octaves_q16 = i64::from(log2_q16(p)) - (1 << 20);
    ((octaves_q16 * 301_029_996 + DEN / 2).div_euclid(DEN)) as i32
}

/// Spherical spreading loss over `x_mm`, cdB, relative to 1 m and never negative. It is
/// `20 log10(x / 1 m)`, evaluated as one rounded logarithm of the square, so the error is below
/// 0.6 cdB and exact powers of ten land on their value.
pub fn spherical_cdb(x_mm: i64) -> i32 {
    let x = u128::try_from(x_mm.max(1)).unwrap_or(1);
    (rounded_cdb((x * x) << 16) - 6000).max(0)
}

/// Spreading loss over `d_mm`: spherical, or, when `g > 0` and `d_mm` exceeds the horizon `d_h`,
/// spherical to the horizon, cylindrical beyond it, plus the ducting loss per kilometre.
pub fn spreading_cdb(t: &AcousticTables, d_mm: i64, g: i64, d_h: Option<i64>) -> i32 {
    match d_h {
        Some(h) if g > 0 && d_mm > h => {
            // Distances below 1 m are never used, so the horizon is floored the same way.
            let hh = h.max(MIN_DISTANCE_MM);
            let ratio = (i128::from(d_mm) << 16) / i128::from(hh);
            let duct = t.globals().duct_loss_cdb_per_km * (d_mm - h) / 1_000_000;
            spherical_cdb(hh) + rounded_cdb(ratio as u128) + duct as i32
        }
        _ => spherical_cdb(d_mm),
    }
}

/// Air absorption of band `band` (0 to 2) over `d_mm`, cdB.
pub fn absorption_cdb(t: &AcousticTables, band: usize, d_mm: i64) -> i32 {
    (t.bands()[band].abs_cdb_per_km * d_mm / 1_000_000) as i32
}

/// Shadow loss of band `band` beyond the horizon `d_h` when `g < 0` (sound bends up), cdB: the
/// smaller of the band cap scaled by `min(|g|, shadow_sat_g) / shadow_sat_g` and the slope times
/// the distance past the horizon. Zero otherwise.
pub fn shadow_cdb(t: &AcousticTables, band: usize, g: i64, d_mm: i64, d_h: Option<i64>) -> i32 {
    let h = match d_h {
        Some(h) if g < 0 && d_mm > h => h,
        _ => return 0,
    };
    let sat = t.globals().shadow_sat_g;
    let b = &t.bands()[band];
    let cap = b.shadow_cap_cdb * g.abs().min(sat) / sat;
    let slope = b.shadow_slope_cdb_per_km * (d_mm - h) / 1_000_000;
    cap.min(slope) as i32
}

/// The path from a source to a listener over flat ground; `None` beyond `max_range_mm`.
/// Positions are millimetres, heights are above the ground in mm. The loss is spreading, plus
/// absorption, plus shadow, per band.
pub fn path_result(
    t: &AcousticTables,
    weather: &Weather,
    src_mm: [i64; 2],
    hs_mm: u16,
    lis_mm: [i64; 2],
    hr_mm: u16,
) -> Option<PathResult> {
    let (dx, dy) = (lis_mm[0] - src_mm[0], lis_mm[1] - src_mm[1]);
    let raw =
        isqrt_u128((i128::from(dx) * i128::from(dx) + i128::from(dy) * i128::from(dy)) as u128);
    let d = i64::try_from(raw).ok()?.max(MIN_DISTANCE_MM);
    if d > t.globals().max_range_mm {
        return None;
    }
    let c = sound_speed_mm_s(t, i32::from(weather.temp_dc));
    let w = along_wind_mm_s(weather.wind_mm_s, dx, dy, d);
    let g = gradient(t, i32::from(weather.lapse_mk_per_m), w);
    let d_h = horizon_mm(t, c, g, i64::from(hs_mm), i64::from(hr_mm));
    let spread = spreading_cdb(t, d, g, d_h);
    let mut loss_cdb = [0i32; 3];
    for (b, out) in loss_cdb.iter_mut().enumerate() {
        *out = spread + absorption_cdb(t, b, d) + shadow_cdb(t, b, g, d, d_h);
    }
    // Direction listener to source, in metres.
    let bearing = if raw == 0 {
        Angle::ZERO
    } else {
        Angle::atan2(fx_ratio(-dy, 1000), fx_ratio(-dx, 1000))
    };
    Some(PathResult {
        distance_mm: d,
        delay_ms: delay_ms(d, c, w),
        loss_cdb,
        bearing,
    })
}
