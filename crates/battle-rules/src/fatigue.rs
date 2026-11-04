//! Integer fatigue model (SPEC-004 section 4).

use crate::{Gait, LocomotionTable, FATIGUE_MAX};
use core_math::Fx;

/// Speed multiplier in permille for a fatigue level in ppm: linear interpolation between the two
/// bracketing curve points, `p0 + (p1 - p0) * (f - f0) / (f1 - f0)` with `i64` division that
/// truncates toward zero. Fatigue above [`FATIGUE_MAX`] is treated as [`FATIGUE_MAX`].
pub fn fatigue_speed_pm(fatigue_ppm: u32, table: &LocomotionTable) -> u32 {
    let f = fatigue_ppm.min(FATIGUE_MAX);
    let curve = table.curve();
    let upper = curve.partition_point(|p| p.fatigue_ppm < f);
    let Some(p1) = curve.get(upper) else {
        return curve.last().map_or(1000, |p| p.speed_pm);
    };
    if p1.fatigue_ppm == f || upper == 0 {
        return p1.speed_pm;
    }
    let p0 = &curve[upper - 1];
    let run = i64::from(p1.fatigue_ppm) - i64::from(p0.fatigue_ppm);
    let rise = i64::from(p1.speed_pm) - i64::from(p0.speed_pm);
    let v = i64::from(p0.speed_pm) + rise * (i64::from(f) - i64::from(p0.fatigue_ppm)) / run;
    u32::try_from(v).unwrap_or(0)
}

/// One tick of the fatigue update, returning the new fatigue in `0..=1_000_000`.
///
/// `applied_speed` is the speed (m/s) actually achieved this tick, zero when the displacement was
/// not applied. A soldier moving at or above the rest speed gains
/// `drain_ppm_per_tick(gait) * effort_pm / 1000` (integer floor); any other soldier recovers
/// `rest_recover_ppm_per_tick`. The result is clamped to the valid range.
pub fn fatigue_step(
    fatigue_ppm: u32,
    gait: Gait,
    effort_pm: u32,
    applied_speed: Fx,
    table: &LocomotionTable,
) -> u32 {
    fatigue_step_scaled(
        fatigue_ppm,
        gait,
        effort_pm,
        applied_speed,
        table,
        1000,
        1000,
    )
}

/// [`fatigue_step`] with the heat multipliers of SPEC-009 section 7 (SPEC-004 amendment A5):
/// moving soldiers gain `drain_ppm_per_tick * effort_pm * drain_pm / 10^6` (one floor over the
/// whole product), resting soldiers recover `rest_recover_ppm_per_tick * rest_pm / 1000` (floor).
/// With both multipliers at 1000 the result equals [`fatigue_step`].
pub fn fatigue_step_scaled(
    fatigue_ppm: u32,
    gait: Gait,
    effort_pm: u32,
    applied_speed: Fx,
    table: &LocomotionTable,
    drain_pm: u32,
    rest_pm: u32,
) -> u32 {
    let f = fatigue_ppm.min(FATIGUE_MAX);
    let moving = applied_speed > Fx::ZERO && applied_speed >= table.rest_speed();
    if moving {
        let drain = u64::from(table.gait(gait).drain_ppm_per_tick)
            * u64::from(effort_pm)
            * u64::from(drain_pm)
            / 1_000_000;
        let sum = u64::from(f) + drain;
        u32::try_from(sum.min(u64::from(FATIGUE_MAX))).unwrap_or(FATIGUE_MAX)
    } else {
        let recover =
            u64::from(table.globals().rest_recover_ppm_per_tick) * u64::from(rest_pm) / 1000;
        f.saturating_sub(u32::try_from(recover).unwrap_or(u32::MAX))
    }
}
