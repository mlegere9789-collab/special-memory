//! Pure morale helpers (SPEC-006 sections 2, 5.2, 5.8 and 7).

/// Cohesion removed by one casualty of weight 1000 permille: `ceil(10^9 / (s0 * cap_pm))` ppm.
/// A regiment of zero soldiers or a zero capacity is treated as one, so the result is always
/// defined and at most `10^9`.
///
/// ```
/// assert_eq!(battle_rules::morale::shock_unit(400, 455), 5495);
/// ```
pub fn shock_unit(s0: u32, cap_pm: u32) -> u32 {
    let den = u64::from(s0.max(1)) * u64::from(cap_pm.max(1));
    u32::try_from(1_000_000_000u64.div_ceil(den)).unwrap_or(u32::MAX)
}

/// Linear interpolation of a value on a cohesion span: `v_hi + (v_lo - v_hi) * (c_hi - c) /
/// (c_hi - c_lo)` with `c` clamped to `[c_lo, c_hi]` and the division truncating toward zero.
/// With `c_hi <= c_lo` the span is empty and the result is `v_hi`.
///
/// ```
/// use battle_rules::morale::lerp;
/// assert_eq!(lerp(680_000, 680_000, 350_000, 1000, 550), 1000);
/// assert_eq!(lerp(350_000, 680_000, 350_000, 1000, 550), 550);
/// ```
pub fn lerp(c: i64, c_hi: i64, c_lo: i64, v_hi: i64, v_lo: i64) -> i64 {
    if c_hi <= c_lo {
        return v_hi;
    }
    let c = c.clamp(c_lo, c_hi);
    let step = i128::from(v_lo - v_hi) * i128::from(c_hi - c) / i128::from(c_hi - c_lo);
    // The step lies between zero and `v_lo - v_hi`, so the sum lies between the two values.
    i64::try_from(i128::from(v_hi) + step).unwrap_or(v_hi)
}

/// Whether a loading soldier makes progress on `tick`: with `a = (tick + slot_index) * fire_pm`
/// the answer is `(a + fire_pm) / 1000 > a / 1000` in `u64`. It is always true at 1000 permille,
/// never at 0, and over any 1000 ticks true `fire_pm` times, evenly spread.
///
/// ```
/// use battle_rules::morale::load_gate;
/// assert!((0..100).all(|t| load_gate(t, 7, 1000)));
/// assert!(!(0..100).any(|t| load_gate(t, 7, 0)));
/// ```
pub fn load_gate(tick: u64, slot_index: u32, fire_pm: u16) -> bool {
    let p = u64::from(fire_pm);
    let a = tick.saturating_add(u64::from(slot_index)).saturating_mul(p);
    a.saturating_add(p) / 1000 > a / 1000
}

/// How an enemy regiment bears on a regiment's facing (SPEC-006 section 5.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Exposure {
    /// Ahead, within 60 degrees of the facing.
    Front,
    /// Oblique: between front and enfilade, or between enfilade and rear.
    Flank,
    /// Along the line: within about 20 degrees of the line's own axis.
    Enfilade,
    /// Behind, within 45 degrees of straight back.
    Rear,
}

/// Classifies an enemy by `f`, its distance along the facing, and `r`, its distance across it
/// (the absolute value is taken), both raw `i128` dot products. Front: `f > 0` and
/// `r * 1000 < 1732 * f`. Rear: `f < 0` and `r < -f`. Of the rest, enfilade when
/// `|f| * 1000 <= 364 * r`, else flank.
///
/// ```
/// use battle_rules::morale::{classify_exposure, Exposure};
/// assert_eq!(classify_exposure(1000, 0), Exposure::Front);
/// assert_eq!(classify_exposure(0, 1000), Exposure::Enfilade);
/// assert_eq!(classify_exposure(-1000, 0), Exposure::Rear);
/// ```
pub fn classify_exposure(f: i128, r: i128) -> Exposure {
    let r = r.saturating_abs();
    if f > 0 && r.saturating_mul(1000) < f.saturating_mul(1732) {
        Exposure::Front
    } else if f < 0 && r < f.saturating_neg() {
        Exposure::Rear
    } else if f.saturating_abs().saturating_mul(1000) <= r.saturating_mul(364) {
        Exposure::Enfilade
    } else {
        Exposure::Flank
    }
}
