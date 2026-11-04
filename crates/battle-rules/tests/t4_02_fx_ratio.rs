//! T4-02: `fx_ratio` raw values (SPEC-004 section 1).

use battle_rules::fx_ratio;
use core_math::Fx;

#[test]
fn truncates_toward_zero_positive() {
    // 1067 * 2^32 / 1000 = 4_582_730_104.8, truncated.
    assert_eq!(fx_ratio(1067, 1000).raw(), 4_582_730_104);
}

#[test]
fn truncates_toward_zero_negative() {
    // -(2^32 / 3) = -1_431_655_765.33, truncated toward zero (not floored).
    assert_eq!(fx_ratio(-1, 3).raw(), -1_431_655_765);
}

#[test]
fn zero_denominator_follows_div_rules() {
    assert_eq!(fx_ratio(1, 0), Fx::MAX);
    assert_eq!(fx_ratio(-1, 0), Fx::MIN);
    assert_eq!(fx_ratio(0, 0), Fx::ZERO);
}

#[test]
fn exact_and_saturating_cases() {
    assert_eq!(fx_ratio(1, 1), Fx::ONE);
    assert_eq!(fx_ratio(1, 2), Fx::HALF);
    assert_eq!(fx_ratio(3, -2).raw(), -(3i64 << 31));
    assert_eq!(fx_ratio(i64::MAX, 1), Fx::MAX);
    assert_eq!(fx_ratio(i64::MIN, 1), Fx::MIN);
    assert_eq!(fx_ratio(i64::MIN, -1), Fx::MAX);
}

#[test]
fn matches_core_math_division_on_whole_numbers() {
    for (n, d) in [(7, 3), (-7, 3), (1000, 7), (0, 5), (123_456, 1000)] {
        assert_eq!(
            fx_ratio(n, d),
            Fx::from_int(n as i32) / Fx::from_int(d as i32)
        );
    }
}
