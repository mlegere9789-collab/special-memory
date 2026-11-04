//! T9-03: the optical-depth table equals its reference regeneration, `transmit(0)` is 10^6, the
//! function is non-increasing, tracks the reference interpolation within 2 ppm at 1,000 sampled
//! values and is 0 from tau = 16 (SPEC-009 section 5, `qc/fixtures/ref_exp.py`).

use battle_rules::{exp_table, EXP_TABLE};

const REF: &str = include_str!("../../../qc/fixtures/ref_exp.csv");

fn rows(kind: &str) -> Vec<Vec<i64>> {
    REF.lines()
        .filter_map(|l| l.strip_prefix(kind).and_then(|r| r.strip_prefix(',')))
        .map(|r| r.split(',').map(|x| x.parse().unwrap()).collect())
        .collect()
}

#[test]
fn const_table_equals_the_regenerated_reference() {
    let t = rows("table");
    assert_eq!(t.len(), 129);
    for (i, r) in t.iter().enumerate() {
        assert_eq!(r[0], i as i64);
        assert_eq!(i64::from(EXP_TABLE[i]), r[1], "entry {i}");
    }
}

#[test]
fn endpoints_and_monotonicity() {
    assert_eq!(exp_table(0), 1_000_000);
    assert_eq!(EXP_TABLE[8], 367_879);
    assert_eq!(exp_table(1_000_000), 367_879);
    assert_eq!(exp_table(16_000_000), 0);
    assert_eq!(exp_table(16_000_001), 0);
    assert_eq!(exp_table(u64::MAX), 0);
    let mut prev = exp_table(0);
    for tau in (0..16_100_000u64).step_by(997) {
        let v = exp_table(tau);
        assert!(v <= prev, "rose at {tau}");
        prev = v;
    }
}

#[test]
fn within_two_ppm_of_the_reference_at_1000_samples() {
    let s = rows("sample");
    assert_eq!(s.len(), 1000);
    for r in &s {
        let got = i64::from(exp_table(r[0] as u64));
        assert!(
            (got - r[1]).abs() <= 2,
            "tau_micro {}: {got} against {}",
            r[0],
            r[1]
        );
        // The table is the interpolant of exp, not exp itself: chord error is at most 1954 ppm.
        assert!(
            (got - r[2]).abs() <= 2000,
            "tau_micro {}: {got} against exp {}",
            r[0],
            r[2]
        );
    }
}
