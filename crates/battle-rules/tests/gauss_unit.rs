//! `gauss_unit` mean and standard deviation over 1,000,000 draws, and `angle_from_urad`
//! (SPEC-005 sections 2 and 3).

use battle_rules::{angle_from_urad, gauss_unit};
use core_math::{Fx, RngStreams, StreamId};

#[test]
fn mean_and_deviation_over_a_million_draws() {
    let mut rng = RngStreams::new(1).stream(StreamId(0x0F01));
    let n = 1_000_000usize;
    let (mut sum, mut sum_sq) = (0f64, 0f64);
    for _ in 0..n {
        let g = gauss_unit(&mut rng).raw() as f64 / 4_294_967_296.0;
        sum += g;
        sum_sq += g * g;
    }
    let mean = sum / n as f64;
    let sd = (sum_sq / n as f64 - mean * mean).sqrt();
    assert!(mean.abs() < 0.01, "mean {mean}");
    assert!((sd - 1.0).abs() < 0.02, "sd {sd}");
}

#[test]
fn draws_are_bounded_and_consume_twelve_words() {
    let mut a = RngStreams::new(7).stream(StreamId(1));
    let mut b = a.clone();
    for _ in 0..10_000 {
        let g = gauss_unit(&mut a);
        assert!(g > Fx::from_int(-6) && g < Fx::from_int(6));
        for _ in 0..12 {
            b.next_u32();
        }
    }
    assert_eq!(a, b);
}

#[test]
fn same_seed_same_sequence() {
    let mut a = RngStreams::new(99).stream(StreamId(5));
    let mut b = RngStreams::new(99).stream(StreamId(5));
    for _ in 0..1000 {
        assert_eq!(gauss_unit(&mut a), gauss_unit(&mut b));
    }
}

#[test]
fn angle_from_urad_converts_and_wraps() {
    assert_eq!(angle_from_urad(0).raw(), 0);
    // One microradian is 2^32 / 6283185 = 683.57 raw units, truncated toward zero.
    assert_eq!(angle_from_urad(1).raw(), 683);
    assert_eq!(angle_from_urad(-1).raw(), u32::MAX - 682);
    // 60000 urad (the canister cone sigma): 60000 * 2^32 / 6283185.
    assert_eq!(
        i128::from(angle_from_urad(60_000).raw()),
        60_000i128 * (1 << 32) / 6_283_185
    );
    // A quarter turn is 1,570,796 urad, within one microradian of 2^30 raw.
    let q = i64::from(angle_from_urad(1_570_796).raw());
    assert!((q - (1 << 30)).abs() < 700, "{q}");
    // A full turn wraps to zero.
    assert_eq!(angle_from_urad(6_283_185).raw(), 0);
    // Negative of positive is the wrapped complement.
    for u in [5i64, 700, 12_000, 250_000] {
        let p = angle_from_urad(u).raw();
        let n = angle_from_urad(-u).raw();
        assert_eq!(p.wrapping_add(n), 0, "{u}");
    }
}
