//! Deterministic helpers for aiming errors (SPEC-005 sections 2 and 3).

use core_math::{Angle, Fx, Rng};

/// Micro-radians in one turn, `round(2 pi * 10^6)`.
const URAD_PER_TURN: i128 = 6_283_185;

/// A standard-normal draw: the sum of twelve [`Rng::unit_fx`] draws minus 6 (mean 0, variance 1,
/// confined to `[-6, 6]`). Integer only; consumes exactly twelve `u32` draws from `rng`.
pub fn gauss_unit(rng: &mut Rng) -> Fx {
    let mut sum: i64 = 0;
    for _ in 0..12 {
        sum += rng.unit_fx().raw();
    }
    Fx::from_raw(sum - 6 * Fx::ONE.raw())
}

/// Converts micro-radians to an [`Angle`]: raw `u * 2^32 / 6283185`, truncated toward zero.
/// A negative `u` gives the angle just below a full turn (wrapping, as `Angle` does); a
/// magnitude above one turn wraps.
pub fn angle_from_urad(u: i64) -> Angle {
    let raw = (i128::from(u) << 32) / URAD_PER_TURN;
    Angle::from_raw(raw.rem_euclid(1i128 << 32) as u32)
}
