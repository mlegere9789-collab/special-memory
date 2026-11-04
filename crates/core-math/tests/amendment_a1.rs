//! SPEC-001 Amendment A1 and mutation-hardening tests.

use core_math::{Angle, Rng};

/// Amendment A1: `try_from_state` is the exact round-trip API and rejects the all-zero state.
#[test]
fn try_from_state_rejects_zero_and_round_trips_exactly() {
    assert!(Rng::try_from_state([0; 4]).is_none());
    let mut a = Rng::from_seed(77);
    for _ in 0..5 {
        a.next_u64();
    }
    let mut b = Rng::try_from_state(a.state()).expect("non-zero state is valid");
    assert_eq!(b.state(), a.state());
    for _ in 0..16 {
        assert_eq!(a.next_u64(), b.next_u64());
    }
    for s in [[1, 0, 0, 0], [0, 0, 0, 1], [u64::MAX; 4]] {
        assert_eq!(Rng::try_from_state(s).map(|r| r.state()), Some(s));
    }
}

/// Targets the mutant that changes the zero-state fallback seed of `from_state`
/// (for example seeding from 1, or keeping the zero state).
#[test]
fn from_state_zero_substitutes_the_seed_zero_state() {
    let r = Rng::from_state([0; 4]);
    assert_eq!(r, Rng::from_seed(0));
    // The seed-0 state starts with the first SplitMix64 output for seed 0.
    assert_eq!(r.state()[0], 0xE220_A839_7B1D_CDAF);
    assert_ne!(r.state(), [0; 4]);
    assert_ne!(r, Rng::from_seed(1));
    let mut r = r;
    assert_ne!(r.next_u64(), 0);
    // Non-zero states are never substituted.
    assert_eq!(Rng::from_state([0, 0, 0, 9]).state(), [0, 0, 0, 9]);
}

/// Targets the mutant that removes the `num >= den` early return of `coin` (so that `num == den`
/// draws a random number) and variants that consume a draw in the degenerate cases.
#[test]
fn coin_degenerate_cases_consume_no_draw() {
    for (num, den, expect) in [
        (5u32, 5u32, true),
        (1, 1, true),
        (u32::MAX, u32::MAX, true),
        (7, 3, true),
        (u32::MAX, 1, true),
        (0, 5, false),
        (0, 0, false),
        (3, 0, false),
        (0, u32::MAX, false),
    ] {
        let mut r = Rng::from_seed(123);
        let before = r.state();
        assert_eq!(r.coin(num, den), expect, "coin({num}, {den})");
        assert_eq!(r.state(), before, "coin({num}, {den}) consumed a draw");
    }
    // A non-degenerate coin does consume exactly one draw (state advanced).
    let mut r = Rng::from_seed(123);
    let before = r.state();
    let _ = r.coin(1, 2);
    assert_ne!(r.state(), before);
}

/// Targets the mutant that drops or alters the rounding offset (`+ den / 2`) of
/// `Angle::from_turns_fraction`: floor instead of round-to-nearest.
#[test]
fn from_turns_fraction_rounds_to_nearest() {
    assert_eq!(Angle::from_turns_fraction(1, 3).raw(), 1_431_655_765);
    assert_eq!(Angle::from_turns_fraction(2, 3).raw(), 2_863_311_531);
    assert_eq!(Angle::from_turns_fraction(-1, 3).raw(), 2_863_311_531);
    assert_eq!(Angle::from_turns_fraction(1, 7).raw(), 613_566_757);
    assert_eq!(Angle::from_turns_fraction(1, 6).raw(), 715_827_883);
    assert_eq!(Angle::from_turns_fraction(5, 7).raw(), 3_067_833_783);
    // Nearest characterisation over many fractions: no neighbouring raw unit is closer.
    for den in [3u32, 5, 7, 9, 11, 13, 360, 1000, 65_537, 4_000_000_007] {
        for num in [1i32, 2, 3, 5, 7, 11, 12, 100, 299, -1, -7] {
            let raw = i128::from(Angle::from_turns_fraction(num, den).raw());
            let r = i128::from(num).rem_euclid(i128::from(den));
            let exact_times_den = r << 32;
            let err = |cand: i128| (cand * i128::from(den) - exact_times_den).abs();
            assert!(err(raw) <= err(raw + 1), "num {num} den {den}");
            assert!(err(raw) <= err(raw - 1), "num {num} den {den}");
        }
    }
}

/// Targets the mutant that drops or alters the rounding offset (`+ 180`) of
/// `Angle::from_degrees_int`: floor instead of round-to-nearest.
#[test]
fn from_degrees_int_rounds_to_nearest() {
    assert_eq!(Angle::from_degrees_int(1).raw(), 11_930_465);
    assert_eq!(Angle::from_degrees_int(7).raw(), 83_513_253);
    assert_eq!(Angle::from_degrees_int(13).raw(), 155_096_041);
    assert_eq!(Angle::from_degrees_int(359).raw(), 4_283_036_831);
    assert_eq!(Angle::from_degrees_int(-1).raw(), 4_283_036_831);
    for deg in 0..360i32 {
        let raw = i128::from(Angle::from_degrees_int(deg).raw());
        let exact_times_360 = i128::from(deg) << 32;
        let err = |cand: i128| (cand * 360 - exact_times_360).abs();
        assert!(err(raw) <= err(raw + 1), "deg {deg}");
        assert!(err(raw) <= err(raw - 1), "deg {deg}");
    }
}
