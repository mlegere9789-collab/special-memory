//! T5-11: loading. Shot-to-shot ticks equal the `ceil` formula for every weapon and training;
//! measured rounds per minute meet bands B5 and B6; initial offsets differ (SPEC-005 section 4).

mod t022_support;

use battle_rules::Training;
use std::collections::BTreeSet;
use t022_support::{duel, no_misfire, tick};

fn load_pm(t: Training) -> u64 {
    match t {
        Training::Raw => 1200,
        Training::Trained => 1000,
        Training::Veteran => 840,
    }
}

fn ceil_div(a: u64, b: u64) -> u64 {
    a.div_ceil(b)
}

/// Ticks between shot `k` and shot `k + 1` (`k` shots already fired, fouling `min(1000, 12 k)`).
fn expected_interval(steps: &[(&str, u64)], training: Training, k: u64) -> u64 {
    let fouling = (12 * k).min(1000);
    steps
        .iter()
        .map(|&(id, ticks)| {
            let t = ceil_div(ticks * load_pm(training), 1000);
            if id == "ram" {
                ceil_div(t * (1000 + 500 * fouling / 1000), 1000)
            } else {
                t
            }
        })
        .sum()
}

/// Ticks at which soldier 0 of a one-man regiment fires, with misfires disabled.
fn shot_ticks(weapon: &str, training: Training, shots: usize, limit: u64) -> Vec<u64> {
    let (mut b, _, _) = duel(5, weapon, 0, training, 1, 20);
    no_misfire(&mut b);
    let mut out = Vec::new();
    while out.len() < shots && b.world.tick < limit {
        let before = b.pool.len();
        tick(&mut b);
        for _ in before..b.pool.len() {
            out.push(b.world.tick - 1);
        }
    }
    assert_eq!(out.len(), shots, "{weapon} fired too few shots");
    out
}

#[test]
fn shot_to_shot_ticks_follow_the_ceil_formula() {
    let weapons: [(&str, &[(&str, u64)]); 4] = [
        (
            "smoothbore_69",
            &[
                ("handle", 120),
                ("seat", 60),
                ("ram", 140),
                ("prime", 100),
                ("ready", 60),
            ],
        ),
        (
            "rifle_58",
            &[
                ("handle", 120),
                ("seat", 60),
                ("ram", 160),
                ("prime", 100),
                ("ready", 60),
            ],
        ),
        (
            "carbine_52",
            &[("open", 40), ("insert", 60), ("close", 30), ("prime", 20)],
        ),
        (
            "napoleon_12",
            &[
                ("sponge", 200),
                ("load", 300),
                ("ram", 200),
                ("prime", 200),
                ("lay", 300),
            ],
        ),
    ];
    for (name, steps) in weapons {
        let shots = if name == "napoleon_12" { 4 } else { 6 };
        for training in Training::ALL {
            let t = shot_ticks(name, training, shots, 40_000);
            for k in 0..shots - 1 {
                let got = t[k + 1] - t[k];
                let want = expected_interval(steps, training, k as u64 + 1);
                assert_eq!(got, want, "{name} {training:?} interval {k}");
            }
        }
    }
}

#[test]
fn revolver_fires_its_cylinder_then_reloads() {
    let t = shot_ticks("revolver_44", Training::Trained, 8, 10_000);
    for k in 0..5 {
        assert_eq!(t[k + 1] - t[k], 20, "cycle {k}");
    }
    assert_eq!(t[6] - t[5], 1200 + 20 - 20, "reload after the sixth round");
    assert_eq!(t[7] - t[6], 20);
}

/// Rounds per soldier per minute from the mean shot-to-shot interval of `men` soldiers over
/// `minutes` minutes (shots are counted after each soldier's first, so the random start of the
/// first load does not bias the rate).
fn rate_per_minute(weapon: &str, training: Training, men: usize, minutes: u64) -> f64 {
    let (mut b, _, _) = duel(11, weapon, 0, training, men, 40);
    let mut last = vec![None::<u64>; 1000];
    let (mut sum, mut count) = (0u64, 0u64);
    for _ in 0..minutes * 1200 {
        let n = b.pool.len();
        tick(&mut b);
        let t = b.world.tick - 1;
        for p in &b.pool[n..] {
            if let Some(prev) = last[p.owner as usize].replace(t) {
                sum += t - prev;
                count += 1;
            }
        }
    }
    1200.0 * count as f64 / sum as f64
}

#[test]
fn smoothbore_rate_meets_band_b5() {
    for (training, lo, hi) in [
        (Training::Trained, 2.3, 2.7),
        (Training::Raw, 1.9, 2.2),
        (Training::Veteran, 2.8, 3.2),
    ] {
        let r = rate_per_minute("smoothbore_69", training, 100, 3);
        assert!((lo..=hi).contains(&r), "{training:?}: {r}");
    }
}

#[test]
fn gun_rate_meets_band_b6() {
    let r = rate_per_minute("napoleon_12", Training::Trained, 1, 6);
    assert!((0.9..=1.1).contains(&r), "{r}");
}

#[test]
fn initial_offsets_differ() {
    let (b, _, _) = duel(3, "smoothbore_69", 0, Training::Trained, 200, 40);
    let lefts: BTreeSet<u16> = b.fire.iter().take(200).map(|f| f.left).collect();
    assert!(lefts.len() > 20, "only {} distinct offsets", lefts.len());
    assert!(b.fire.iter().take(200).all(|f| f.left < 120));
}
