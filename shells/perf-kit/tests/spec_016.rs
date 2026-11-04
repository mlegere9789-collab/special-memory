//! SPEC-016 T16-08, T16-09, T16-10 and the supporting integer-geometry checks.

use perf_kit::{
    arrival_sample, assign_tiers, cluster_key, gain_q, range_mm, select_voices, Cluster, Tier,
    CLUSTER_VOICES, SAMPLES_PER_TICK,
};
use proptest::prelude::*;

// ---------------------------------------------------------------- T16-08 arrival

#[test]
fn t16_08_arrival_values() {
    assert_eq!(arrival_sample(0, 343_000), 48_000);
    assert_eq!(arrival_sample(0, 1_000_000), 139_941);
    assert_eq!(arrival_sample(0, 0), 0);
    // Tick 3 adds 7,200 samples.
    assert_eq!(arrival_sample(3, 0), 7_200);
    assert_eq!(arrival_sample(3, 343_000), 7_200 + 48_000);
    assert_eq!(SAMPLES_PER_TICK, 2_400);
}

#[test]
fn arrival_saturates_instead_of_wrapping() {
    assert_eq!(arrival_sample(u64::MAX, u64::MAX), u64::MAX);
    assert!(arrival_sample(0, u64::MAX) > 0);
}

proptest! {
    #[test]
    fn arrival_is_monotone(tick in 0u64..1_000_000, a in 0u64..10_000_000, b in 0u64..10_000_000) {
        let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
        prop_assert!(arrival_sample(tick, lo) <= arrival_sample(tick, hi));
        prop_assert!(arrival_sample(tick, lo) <= arrival_sample(tick + 1, lo));
    }
}

// ---------------------------------------------------------------- range_mm

#[test]
fn range_exact_values() {
    assert_eq!(range_mm(0, 0, 0), 0);
    assert_eq!(range_mm(3, 4, 0), 5);
    assert_eq!(range_mm(-3, -4, 12), 13);
    assert_eq!(range_mm(1, 1, 1), 1); // floor(sqrt(3))
    assert_eq!(range_mm(343_000, 0, 0), 343_000);
}

#[test]
fn range_extremes_do_not_overflow() {
    let r = range_mm(i64::MIN, i64::MIN, i64::MIN);
    // 3 * 2^126 has integer square root floor(sqrt(3) * 2^63).
    let r128 = u128::from(r);
    let n: u128 = 3 * (1u128 << 126);
    assert!(r128 * r128 <= n && (r128 + 1) * (r128 + 1) > n);
    assert_eq!(range_mm(i64::MAX, 0, 0), i64::MAX as u64);
}

proptest! {
    #[test]
    fn range_is_floor_of_euclidean_norm(dx in any::<i64>(), dy in any::<i64>(), dz in any::<i64>()) {
        let r = u128::from(range_mm(dx, dy, dz));
        let n = i128::from(dx).pow(2) as u128 + i128::from(dy).pow(2) as u128 + i128::from(dz).pow(2) as u128;
        prop_assert!(r * r <= n);
        prop_assert!((r + 1) * (r + 1) > n);
    }
}

// ---------------------------------------------------------------- T16-09 tiers

fn one(d: u32, prev: Tier) -> Tier {
    let mut out = Vec::new();
    assign_tiers(&[d], &[0], &[prev], (800, 6_000), &mut out);
    assert_eq!(out.len(), 1);
    out[0]
}

#[test]
fn t16_09_hysteresis_sequence() {
    let mut t = one(29_999, Tier::D);
    assert_eq!(t, Tier::A);
    t = one(31_000, t);
    assert_eq!(t, Tier::A);
    t = one(33_000, t);
    assert_eq!(t, Tier::B);
    t = one(31_000, t);
    assert_eq!(t, Tier::B);
    t = one(29_999, t);
    assert_eq!(t, Tier::A);
}

#[test]
fn hysteresis_at_every_boundary() {
    // (enter below, leave at or above) for A, B, C, with the nearer and farther tier.
    let bands = [
        (30_000, 33_000, Tier::A, Tier::B),
        (150_000, 165_000, Tier::B, Tier::C),
        (600_000, 660_000, Tier::C, Tier::D),
    ];
    for (enter, leave, near, far) in bands {
        assert_eq!(one(enter - 1, far), near);
        assert_eq!(one(enter, far), far);
        assert_eq!(one(leave - 1, near), near);
        assert_eq!(one(leave, near), far);
    }
    assert_eq!(one(0, Tier::D), Tier::A);
    assert_eq!(one(u32::MAX, Tier::A), Tier::D);
}

#[test]
fn t16_09_caps_800_and_200() {
    // 1,000 soldiers all inside 30 m; slots reversed so the slot tie-break matters.
    let n = 1_000u32;
    let d: Vec<u32> = (0..n).map(|i| 1_000 + (i % 7) * 100).collect();
    let slots: Vec<u32> = (0..n).map(|i| n - 1 - i).collect();
    let prev = vec![Tier::D; n as usize];
    let mut out = Vec::new();
    assign_tiers(&d, &slots, &prev, (800, 6_000), &mut out);
    assert_eq!(out.iter().filter(|t| **t == Tier::A).count(), 800);
    assert_eq!(out.iter().filter(|t| **t == Tier::B).count(), 200);
    // Every A is no farther (by (d, slot)) than every B.
    let key = |i: usize| (d[i], slots[i]);
    let worst_a = (0..n as usize)
        .filter(|&i| out[i] == Tier::A)
        .map(key)
        .max()
        .unwrap();
    let best_b = (0..n as usize)
        .filter(|&i| out[i] == Tier::B)
        .map(key)
        .min()
        .unwrap();
    assert!(worst_a < best_b);
}

#[test]
fn b_cap_overflow_falls_to_c_and_reduced_caps_apply() {
    let d: Vec<u32> = (0..2_000u32).map(|i| 100_000 + i).collect();
    let slots: Vec<u32> = (0..2_000u32).collect();
    let mut out = Vec::new();
    assign_tiers(&d, &slots, &[Tier::D; 2_000], (200, 1_500), &mut out);
    assert_eq!(out.iter().filter(|t| **t == Tier::A).count(), 0);
    assert_eq!(out.iter().filter(|t| **t == Tier::B).count(), 1_500);
    assert_eq!(out.iter().filter(|t| **t == Tier::C).count(), 500);
    // The nearest 1,500 keep B.
    assert!(out[..1_500].iter().all(|t| *t == Tier::B));
}

#[test]
fn a_overflow_cascades_through_b_cap() {
    // 10 in A range, caps (2, 3): 2 A, 3 B, 5 C.
    let d: Vec<u32> = (0..10u32).map(|i| 100 + i).collect();
    let slots: Vec<u32> = (0..10u32).collect();
    let mut out = Vec::new();
    assign_tiers(&d, &slots, &[Tier::A; 10], (2, 3), &mut out);
    let count = |t| out.iter().filter(|x| **x == t).count();
    assert_eq!(
        (
            count(Tier::A),
            count(Tier::B),
            count(Tier::C),
            count(Tier::D)
        ),
        (2, 3, 5, 0)
    );
}

#[test]
fn out_is_cleared_and_empty_input_is_empty() {
    let mut out = vec![Tier::A; 5];
    assign_tiers(&[], &[], &[], (800, 6_000), &mut out);
    assert!(out.is_empty());
}

proptest! {
    #[test]
    fn caps_hold_and_order_is_respected(
        raw in proptest::collection::vec((0u32..800_000, 0u8..4), 0..300),
        cap_a in 0u32..20,
        cap_b in 0u32..40,
    ) {
        let d: Vec<u32> = raw.iter().map(|r| r.0).collect();
        let prev: Vec<Tier> = raw.iter().map(|r| [Tier::A, Tier::B, Tier::C, Tier::D][r.1 as usize]).collect();
        let slots: Vec<u32> = (0..d.len() as u32).rev().collect();
        let mut out = Vec::new();
        assign_tiers(&d, &slots, &prev, (cap_a, cap_b), &mut out);
        prop_assert_eq!(out.len(), d.len());
        prop_assert!(out.iter().filter(|t| **t == Tier::A).count() <= cap_a as usize);
        prop_assert!(out.iter().filter(|t| **t == Tier::B).count() <= cap_b as usize);
        // A soldier never ends nearer than distance allows, and the result is deterministic.
        let mut again = Vec::new();
        assign_tiers(&d, &slots, &prev, (cap_a, cap_b), &mut again);
        prop_assert_eq!(&out, &again);
        for i in 0..d.len() {
            if d[i] >= 33_000 { prop_assert!(out[i] != Tier::A); }
            if d[i] >= 165_000 { prop_assert!(out[i] >= Tier::C); }
            if d[i] >= 660_000 { prop_assert!(out[i] == Tier::D); }
        }
    }
}

// ---------------------------------------------------------------- cluster_key

#[test]
fn cluster_key_uses_euclidean_division() {
    assert_eq!(cluster_key(7, 0, 0), (7, 0, 0));
    assert_eq!(cluster_key(7, 4_999, 5_000), (7, 0, 1));
    assert_eq!(cluster_key(7, -1, -5_000), (7, -1, -1));
    assert_eq!(cluster_key(7, -5_001, -4_999), (7, -2, -1));
    assert_eq!(
        cluster_key(65_535, i32::MIN, i32::MAX),
        (65_535, -429_497, 429_496)
    );
}

proptest! {
    #[test]
    fn cluster_key_is_floor(x in any::<i32>(), y in any::<i32>(), r in any::<u16>()) {
        let (kr, kx, ky) = cluster_key(r, x, y);
        prop_assert_eq!(kr, r);
        prop_assert!(i64::from(kx) * 5_000 <= i64::from(x) && i64::from(x) < (i64::from(kx) + 1) * 5_000);
        prop_assert!(i64::from(ky) * 5_000 <= i64::from(y) && i64::from(y) < (i64::from(ky) + 1) * 5_000);
    }
}

// ---------------------------------------------------------------- T16-10 voices

fn cluster(i: usize, shots: u32, range: u64) -> Cluster {
    Cluster {
        key: (0, i as i32, 0),
        shots,
        range_mm: range,
    }
}

#[test]
fn gain_matches_the_formula() {
    // n * 10^12 / (range + 1000)^2
    assert_eq!(gain_q(1, 0), 1_000_000);
    assert_eq!(gain_q(4, 1_000), 1_000_000);
    assert_eq!(gain_q(0, 5), 0);
    assert_eq!(gain_q(1, u64::MAX), 0);
}

#[test]
fn ties_go_to_the_lower_id() {
    let cs: Vec<Cluster> = (0..10).map(|i| cluster(i, 5, 20_000)).collect();
    assert_eq!(select_voices(&cs, 4), vec![0, 1, 2, 3]);
    let cs = vec![
        cluster(0, 1, 50_000),
        cluster(1, 9, 50_000),
        cluster(2, 9, 50_000),
    ];
    assert_eq!(select_voices(&cs, 2), vec![1, 2]);
    assert_eq!(select_voices(&cs, 10), vec![1, 2, 0]);
    assert!(select_voices(&cs, 0).is_empty());
    assert!(select_voices(&[], 48).is_empty());
}

/// Independent reference: rank each cluster by counting how many others beat it.
fn brute_force(cs: &[Cluster], max: usize) -> Vec<usize> {
    let g: Vec<u128> = cs
        .iter()
        .map(|c| {
            u128::from(c.shots) * 1_000_000_000_000 / ((u128::from(c.range_mm) + 1_000).pow(2))
        })
        .collect();
    let mut by_rank: Vec<Option<usize>> = vec![None; cs.len()];
    for i in 0..cs.len() {
        let rank = (0..cs.len())
            .filter(|&j| g[j] > g[i] || (g[j] == g[i] && j < i))
            .count();
        by_rank[rank] = Some(i);
    }
    by_rank.into_iter().flatten().take(max).collect()
}

proptest! {
    #[test]
    fn t16_10_forty_eight_of_200_match_brute_force(
        raw in proptest::collection::vec((0u32..400, 0u64..3_000_000), 200),
    ) {
        let cs: Vec<Cluster> = raw.iter().enumerate().map(|(i, r)| cluster(i, r.0, r.1)).collect();
        let got = select_voices(&cs, CLUSTER_VOICES);
        prop_assert_eq!(got.len(), 48);
        prop_assert_eq!(got, brute_force(&cs, 48));
    }

    #[test]
    fn heavy_ties_still_match_brute_force(
        raw in proptest::collection::vec((0u32..4, 0u64..3), 200),
        max in 0usize..60,
    ) {
        let cs: Vec<Cluster> = raw.iter().enumerate().map(|(i, r)| cluster(i, r.0, r.1 * 10_000)).collect();
        prop_assert_eq!(select_voices(&cs, max), brute_force(&cs, max));
    }
}
