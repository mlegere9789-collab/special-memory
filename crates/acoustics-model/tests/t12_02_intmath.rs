//! T12-02: integer math of SPEC-012 section 5 (square root, base-2 logarithm, centibel conversions).
//! Floating point appears only here, as the independent reference.

use acoustics_model::intmath::{cdb_of_power, isqrt_u128, log2_q16, power_of_cdb};
use proptest::prelude::*;

#[test]
fn isqrt_is_exact_up_to_2_pow_20() {
    for n in 0u128..=(1 << 20) {
        let r = isqrt_u128(n);
        assert!(r * r <= n && n < (r + 1) * (r + 1), "n = {n}");
    }
}

#[test]
fn isqrt_extremes() {
    assert_eq!(isqrt_u128(u128::MAX), u128::from(u64::MAX));
    assert_eq!(isqrt_u128(1 << 127), 13_043_817_825_332_782_212);
    assert_eq!(
        isqrt_u128(u128::from(u64::MAX).pow(2)),
        u128::from(u64::MAX)
    );
}

proptest! {
    #[test]
    fn isqrt_brackets_n(n in any::<u128>()) {
        let r = isqrt_u128(n);
        prop_assert!(r * r <= n);
        // (r + 1)^2 can exceed u128 only when r = u64::MAX, where n < 2^128 holds trivially.
        prop_assert!(r == u128::from(u64::MAX) || n < (r + 1) * (r + 1));
    }
}

#[test]
fn log2_q16_matches_f64_within_3_units() {
    let mut s: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut next = || {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        s
    };
    for i in 0..100_000u32 {
        // Spread values over every magnitude from 1 to 2^127.
        let bits = 1 + (i % 127);
        let x = ((u128::from(next()) << 64 | u128::from(next())) >> (128 - bits)).max(1);
        let want = (x as f64).log2() * 65536.0;
        let got = f64::from(log2_q16(x));
        assert!((got - want).abs() <= 3.0, "x = {x}: {got} vs {want}");
    }
}

#[test]
fn log2_q16_powers_of_two_are_exact() {
    for n in 0..128u32 {
        assert_eq!(log2_q16(1u128 << n), n << 16);
    }
}

#[test]
fn cdb_of_power_anchors() {
    assert_eq!(cdb_of_power(2 << 16), 301);
    assert_eq!(cdb_of_power(65536), 0);
    assert!((cdb_of_power(10 << 16) - 1000).abs() <= 1);
    assert_eq!(cdb_of_power(0), -20000);
}

#[test]
fn power_of_cdb_anchors_and_clamp() {
    let p = power_of_cdb(3000);
    let want: u128 = 1000 << 16;
    assert!(p.abs_diff(want) * 10_000 <= want, "{p} vs {want}");
    assert_eq!(power_of_cdb(0), 65536);
    assert_eq!(power_of_cdb(-7000), power_of_cdb(-6000));
    assert_eq!(power_of_cdb(25_000), power_of_cdb(19_000));
}

#[test]
fn power_of_cdb_is_monotone() {
    let mut prev = 0;
    for l in -6000..=19_000 {
        let p = power_of_cdb(l);
        assert!(p >= prev, "l = {l}");
        prev = p;
    }
}

#[test]
fn cdb_round_trip_within_1() {
    for l in 0..=19_000 {
        let back = cdb_of_power(power_of_cdb(l));
        assert!((back - l).abs() <= 1, "l = {l}: {back}");
    }
}
