//! T8-05 (hand-derived classifier cases) and T8-06 (input order independence), SPEC-008 section 4.1.
//!
//! Thresholds under test (`data/rules/fidelity.toml`): infantry and cavalry F0 450 m, F1 1800 m;
//! battery F0 1200 m, F1 2400 m; hysteresis 50 m; dwell 200 ticks.

use battle_rules::{
    classify, Arm, ClassInput, FidelityClass, FidelityTable, RegimentId, F0, F1, F2,
};
use core_math::{Fx, Vec2Fx};

const FIDELITY: &str = include_str!("../../../data/rules/fidelity.toml");
const TICK: u64 = 1000;

fn table() -> FidelityTable {
    FidelityTable::from_toml_str(FIDELITY).unwrap()
}

fn at(x_m: i32) -> Vec2Fx {
    Vec2Fx::new(Fx::from_int(x_m), Fx::ZERO)
}

#[allow(clippy::too_many_arguments)]
fn input(
    id: u16,
    team: u8,
    arm: Arm,
    x_m: i32,
    reach_mm: u32,
    fit: u32,
    class: FidelityClass,
    since: u64,
) -> ClassInput {
    ClassInput {
        id: RegimentId(id),
        team,
        arm,
        anchor: at(x_m),
        reach_mm,
        fit,
        class,
        since,
    }
}

/// An infantry regiment of team 0 at the origin facing one of team 1 `gap_m` metres away; the
/// result is the class of the team 0 regiment, which starts in `class` since `since`.
fn pair(gap_m: i32, class: FidelityClass, since: u64, fc: u8) -> FidelityClass {
    let v = [
        input(1, 0, Arm::Infantry, 0, 0, 400, class, since),
        input(2, 1, Arm::Infantry, gap_m, 0, 0, F2, 0),
    ];
    classify(&table(), &v, fc, 15_000, 35_000, TICK)[0]
}

#[test]
fn gap_selects_the_class() {
    assert_eq!(pair(300, F2, 0, 0), F0);
    assert_eq!(pair(450, F2, 0, 0), F0);
    assert_eq!(pair(451, F2, 0, 0), F1);
    assert_eq!(pair(1000, F2, 0, 0), F1);
    assert_eq!(pair(1800, F2, 0, 0), F1);
    assert_eq!(pair(1801, F2, 0, 0), F2);
    assert_eq!(pair(3000, F2, 0, 0), F2);
}

#[test]
fn hysteresis_keeps_a_finer_class() {
    // From F0, 480 m stays F0 (limit 450 + 50); 501 m leaves it once the dwell has passed.
    assert_eq!(pair(480, F0, 0, 0), F0);
    assert_eq!(pair(500, F0, 0, 0), F0);
    assert_eq!(pair(501, F0, TICK - 200, 0), F1);
    // From F1, 480 m stays F1: the F0 limit has no hysteresis for a regiment not in F0.
    assert_eq!(pair(480, F1, 0, 0), F1);
    // From F1, the F1 limit gains the margin: 1850 m stays, 1851 m leaves after the dwell.
    assert_eq!(pair(1850, F1, 0, 0), F1);
    assert_eq!(pair(1851, F1, TICK - 200, 0), F2);
}

#[test]
fn coarser_waits_for_the_dwell_and_finer_is_immediate() {
    assert_eq!(pair(501, F0, TICK - 199, 0), F0);
    assert_eq!(pair(501, F0, TICK - 200, 0), F1);
    assert_eq!(pair(3000, F0, TICK - 199, 0), F0);
    assert_eq!(pair(3000, F0, TICK - 200, 0), F2);
    assert_eq!(pair(3000, F1, TICK - 199, 0), F1);
    // Finer: no wait, even with the clock just reset.
    assert_eq!(pair(300, F1, TICK, 0), F0);
    assert_eq!(pair(300, F2, TICK, 0), F0);
    assert_eq!(pair(1000, F2, TICK, 0), F1);
}

#[test]
fn scenario_class_is_a_floor() {
    assert_eq!(pair(300, F2, 0, 1), F1);
    assert_eq!(pair(300, F0, 0, 1), F1);
    assert_eq!(pair(300, F0, 0, 2), F2);
    assert_eq!(pair(1000, F1, 0, 2), F2);
}

#[test]
fn reach_is_taken_off_the_gap() {
    let run = |dist_m: i32, reach_mm: u32| {
        let v = [
            input(1, 0, Arm::Infantry, 0, reach_mm, 400, F2, 0),
            input(2, 1, Arm::Infantry, dist_m, reach_mm, 0, F2, 0),
        ];
        classify(&table(), &v, 0, 15_000, 35_000, TICK)[0]
    };
    assert_eq!(run(480, 0), F1);
    assert_eq!(run(480, 20_000), F0);
    // A gap cannot go below zero.
    assert_eq!(run(10, 20_000), F0);
}

#[test]
fn arms_use_their_own_limits() {
    let run = |arm: Arm, gap_m: i32| {
        let v = [
            input(1, 0, arm, 0, 0, 12, F2, 0),
            input(2, 1, Arm::Infantry, gap_m, 0, 0, F2, 0),
        ];
        classify(&table(), &v, 0, 15_000, 35_000, TICK)[0]
    };
    assert_eq!(run(Arm::Battery, 1200), F0);
    assert_eq!(run(Arm::Battery, 1201), F1);
    assert_eq!(run(Arm::Battery, 2400), F1);
    assert_eq!(run(Arm::Battery, 2401), F2);
    assert_eq!(run(Arm::Cavalry, 450), F0);
    assert_eq!(run(Arm::Cavalry, 451), F1);
}

#[test]
fn no_enemy_is_class_two_and_friends_are_ignored() {
    let v = [
        input(1, 0, Arm::Infantry, 0, 0, 400, F0, 0),
        input(2, 0, Arm::Infantry, 10, 0, 400, F0, 0),
    ];
    assert_eq!(classify(&table(), &v, 0, 15_000, 35_000, TICK), [F2, F2]);
    assert!(classify(&table(), &[], 0, 15_000, 35_000, TICK).is_empty());
}

#[test]
fn the_nearest_enemy_decides() {
    let v = [
        input(1, 0, Arm::Infantry, 0, 0, 400, F2, 0),
        input(2, 1, Arm::Infantry, 5000, 0, 0, F2, 0),
        input(3, 1, Arm::Infantry, -1000, 0, 0, F2, 0),
    ];
    assert_eq!(classify(&table(), &v, 0, 15_000, 35_000, TICK)[0], F1);
}

/// Three team 0 candidates 100, 200 and 300 m from a team 0 enemy line of no strength; ids are
/// not in gap order (`3, 1, 2`).
fn three(fits: [u32; 3]) -> Vec<ClassInput> {
    vec![
        input(1, 0, Arm::Infantry, 200, 0, fits[0], F2, 0),
        input(2, 0, Arm::Infantry, 300, 0, fits[1], F2, 0),
        input(3, 0, Arm::Infantry, 100, 0, fits[2], F2, 0),
        input(9, 1, Arm::Infantry, 0, 0, 0, F2, 0),
    ]
}

#[test]
fn f0_cap_is_a_strict_prefix_in_gap_order() {
    let v = three([400, 400, 400]);
    let c = classify(&table(), &v, 0, 800, 35_000, TICK);
    assert_eq!((c[0], c[1], c[2]), (F0, F1, F0));
    // The first to exceed and everything after it, even a regiment that would still fit.
    let v = three([1000, 100, 400]);
    let c = classify(&table(), &v, 0, 800, 35_000, TICK);
    assert_eq!((c[2], c[0], c[1]), (F0, F1, F1));
    let v = three([100, 100, 700]);
    let c = classify(&table(), &v, 0, 800, 35_000, TICK);
    assert_eq!((c[2], c[0], c[1]), (F0, F0, F1));
}

#[test]
fn ties_break_by_id() {
    let v = vec![
        input(5, 0, Arm::Infantry, 200, 0, 400, F2, 0),
        input(4, 0, Arm::Infantry, -200, 0, 400, F2, 0),
        input(9, 1, Arm::Infantry, 0, 0, 0, F2, 0),
    ];
    let c = classify(&table(), &v, 0, 400, 35_000, TICK);
    assert_eq!((c[1], c[0]), (F0, F1));
}

#[test]
fn agent_cap_counts_f0_and_f1_and_demotes_the_rest_to_f2() {
    let v = three([400, 400, 400]);
    let c = classify(&table(), &v, 0, 800, 1000, TICK);
    assert_eq!((c[2], c[0], c[1]), (F0, F0, F2));
    let c = classify(&table(), &v, 0, 800, 10_000, TICK);
    assert_eq!((c[2], c[0], c[1]), (F0, F0, F1));
    // Regiments capped from F0 to F1 still count against the agent cap.
    let c = classify(&table(), &v, 0, 400, 800, TICK);
    assert_eq!((c[2], c[0], c[1]), (F0, F1, F2));
}

#[test]
fn caps_override_the_dwell() {
    // Class F0 held since this tick, but the cap still demotes at once.
    let mut v = three([400, 400, 400]);
    for r in &mut v {
        r.class = F0;
        r.since = TICK;
    }
    let c = classify(&table(), &v, 0, 800, 35_000, TICK);
    assert_eq!((c[2], c[0], c[1]), (F0, F0, F1));
}

#[test]
fn headquarters_inputs_are_class_two_and_play_no_part() {
    let v = [
        input(1, 0, Arm::Hq, 0, 0, 0, F0, 0),
        input(2, 1, Arm::Infantry, 100, 0, 400, F2, 0),
    ];
    assert_eq!(classify(&table(), &v, 0, 15_000, 35_000, TICK), [F2, F2]);
}

/// A deterministic shuffle (xorshift) of indices.
fn shuffled(n: usize, mut s: u64) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..n).collect();
    for i in (1..n).rev() {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        idx.swap(i, (s % (i as u64 + 1)) as usize);
    }
    idx
}

#[test]
fn t8_06_result_does_not_depend_on_slice_order() {
    let mut base = Vec::new();
    for i in 0..24i32 {
        let arm = [Arm::Infantry, Arm::Cavalry, Arm::Battery][(i % 3) as usize];
        let class = [F0, F1, F2][(i % 3) as usize];
        base.push(input(
            u16::try_from(i + 1).unwrap(),
            u8::try_from(i % 2).unwrap(),
            arm,
            (i * 137) % 2600 - 700,
            u32::try_from((i * 3_000) % 40_000).unwrap(),
            100 + u32::try_from(i * 53 % 400).unwrap(),
            class,
            u64::try_from(i * 90).unwrap(),
        ));
    }
    let t = table();
    for (fc, caps) in [
        (0u8, (900u32, 1500u32)),
        (0, (15_000, 35_000)),
        (1, (900, 1500)),
    ] {
        let by_id = |v: &[ClassInput]| {
            let mut m: Vec<(u16, FidelityClass)> = v
                .iter()
                .zip(classify(&t, v, fc, caps.0, caps.1, TICK))
                .map(|(r, c)| (r.id.0, c))
                .collect();
            m.sort();
            m
        };
        let want = by_id(&base);
        for seed in 1..=40u64 {
            let order = shuffled(base.len(), seed);
            let v: Vec<ClassInput> = order.iter().map(|&i| base[i].clone()).collect();
            assert_eq!(by_id(&v), want, "seed {seed}");
        }
    }
}
