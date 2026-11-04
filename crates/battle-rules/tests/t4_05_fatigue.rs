//! T4-05: fatigue model, CT-004-2 targets within 20 percent, bounds, curve breakpoints,
//! monotonicity (SPEC-004 sections 3 and 4).

use battle_rules::{
    fatigue_speed_pm, fatigue_step, fx_ratio, Gait, LocomotionTable, FATIGUE_MAX, TICK_HZ,
};
use core_math::Fx;
use proptest::prelude::*;

const LOCO: &str = include_str!("../../../data/rules/locomotion.toml");

fn table() -> LocomotionTable {
    LocomotionTable::from_toml_str(LOCO).unwrap()
}

/// Ground speed of a gait in m/s as `Fx`.
fn speed(t: &LocomotionTable, g: Gait) -> Fx {
    fx_ratio(i64::from(t.gait(g).speed_mm_s), 1000)
}

/// Ticks a fresh soldier on open ground (effort 1000) needs to reach `target` ppm.
fn ticks_to_reach(t: &LocomotionTable, g: Gait, target: u32) -> u32 {
    let mut f = 0;
    let mut n = 0;
    while f < target {
        f = fatigue_step(f, g, 1000, speed(t, g), t);
        n += 1;
        assert!(n < 1_000_000, "never reaches {target}");
    }
    n
}

fn within_20_percent(ticks: u32, seconds: u32) -> bool {
    let want = u64::from(seconds) * u64::from(TICK_HZ);
    let got = u64::from(ticks);
    got * 5 >= want * 4 && got * 5 <= want * 6
}

#[test]
fn ct2_quick_reaches_half_in_900_s() {
    let t = table();
    let n = ticks_to_reach(&t, Gait::Quick, 500_000);
    assert!(within_20_percent(n, 900), "{n} ticks");
}

#[test]
fn ct2_double_quick_reaches_half_in_180_s() {
    let t = table();
    let n = ticks_to_reach(&t, Gait::DoubleQuick, 500_000);
    assert!(within_20_percent(n, 180), "{n} ticks");
}

#[test]
fn ct2_run_reaches_full_in_45_s() {
    let t = table();
    let n = ticks_to_reach(&t, Gait::Run, FATIGUE_MAX);
    assert!(within_20_percent(n, 45), "{n} ticks");
}

#[test]
fn ct2_rest_recovers_half_to_zero_in_300_s() {
    let t = table();
    let mut f = 500_000;
    let mut n = 0u32;
    while f > 0 {
        f = fatigue_step(f, Gait::Quick, 1000, Fx::ZERO, &t);
        n += 1;
    }
    assert!(within_20_percent(n, 300), "{n} ticks");
}

#[test]
fn drain_scales_with_effort_and_floors() {
    let t = table();
    // 139 * 1400 / 1000 = 194.6, floored to 194.
    let f = fatigue_step(0, Gait::DoubleQuick, 1400, speed(&t, Gait::DoubleQuick), &t);
    assert_eq!(f, 194);
    // 7 * 900 / 1000 = 6.3, floored to 6.
    let f = fatigue_step(0, Gait::Walk, 900, speed(&t, Gait::Walk), &t);
    assert_eq!(f, 6);
}

#[test]
fn slow_or_blocked_movement_counts_as_rest() {
    let t = table();
    let start = 10_000;
    // Applied displacement zero (blocked): rest recovery, not drain.
    assert_eq!(
        fatigue_step(start, Gait::Run, 1000, Fx::ZERO, &t),
        start - 83
    );
    // Below rest_speed_mm_s (100 mm/s): rest.
    let slow = fx_ratio(99, 1000);
    assert_eq!(fatigue_step(start, Gait::Run, 1000, slow, &t), start - 83);
    // At rest_speed_mm_s exactly: moving.
    let at = fx_ratio(100, 1000);
    assert_eq!(fatigue_step(start, Gait::Run, 1000, at, &t), start + 1111);
}

#[test]
fn step_clamps_at_both_ends() {
    let t = table();
    assert_eq!(fatigue_step(5, Gait::Quick, 1000, Fx::ZERO, &t), 0);
    assert_eq!(
        fatigue_step(FATIGUE_MAX - 1, Gait::Run, 1000, speed(&t, Gait::Run), &t),
        FATIGUE_MAX
    );
    // An out-of-range input is brought back into range.
    assert_eq!(
        fatigue_step(u32::MAX, Gait::Quick, 1000, Fx::ZERO, &t),
        FATIGUE_MAX - 83
    );
    assert_eq!(
        fatigue_step(u32::MAX, Gait::Run, 10_000, speed(&t, Gait::Run), &t),
        FATIGUE_MAX
    );
}

#[test]
fn curve_equals_data_at_every_breakpoint() {
    let t = table();
    for p in t.curve() {
        assert_eq!(fatigue_speed_pm(p.fatigue_ppm, &t), p.speed_pm);
    }
    assert_eq!(fatigue_speed_pm(0, &t), 1000);
    assert_eq!(fatigue_speed_pm(500_000, &t), 900);
    assert_eq!(fatigue_speed_pm(800_000, &t), 650);
    assert_eq!(fatigue_speed_pm(1_000_000, &t), 400);
}

#[test]
fn curve_interpolates_linearly_and_truncates_toward_zero() {
    let t = table();
    // Between (0,1000) and (500000,900): slope -100 per 500000.
    assert_eq!(fatigue_speed_pm(250_000, &t), 950);
    // 1000 + (-100 * 1) / 500000 = 1000 + 0 (truncated toward zero, not floored to 999).
    assert_eq!(fatigue_speed_pm(1, &t), 1000);
    assert_eq!(fatigue_speed_pm(4_999, &t), 1000);
    assert_eq!(fatigue_speed_pm(5_000, &t), 999);
    // Between (500000,900) and (800000,650): -250 per 300000; at 650000 -> 775.
    assert_eq!(fatigue_speed_pm(650_000, &t), 775);
    // Between (800000,650) and (1000000,400): -250 per 200000; at 900000 -> 525.
    assert_eq!(fatigue_speed_pm(900_000, &t), 525);
    // Above the range the last point holds.
    assert_eq!(fatigue_speed_pm(u32::MAX, &t), 400);
}

proptest! {
    #[test]
    fn curve_is_non_increasing(a in 0u32..=1_000_000, b in 0u32..=1_000_000) {
        let t = table();
        let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
        prop_assert!(fatigue_speed_pm(hi, &t) <= fatigue_speed_pm(lo, &t));
        prop_assert!(fatigue_speed_pm(lo, &t) <= 1000);
        prop_assert!(fatigue_speed_pm(hi, &t) >= 400);
    }

    #[test]
    fn fatigue_never_leaves_bounds(
        start in 0u32..=1_000_000,
        steps in proptest::collection::vec((0usize..4, 0u32..=3000, 0i64..=4000), 1..300),
    ) {
        let t = table();
        let gaits = [Gait::Walk, Gait::Quick, Gait::DoubleQuick, Gait::Run];
        let mut f = start;
        for (g, effort, mm_s) in steps {
            f = fatigue_step(f, gaits[g], effort, fx_ratio(mm_s, 1000), &t);
            prop_assert!(f <= FATIGUE_MAX);
        }
    }
}
