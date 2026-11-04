//! T6-03: `load_gate` is always true at 1000 permille, never at 0, and over any 1000 ticks the
//! number of true ticks follows the floor formula (SPEC-006 section 5.8).

use battle_rules::morale::load_gate;
use proptest::prelude::*;

#[test]
fn extremes() {
    for tick in [0u64, 1, 19, 20, 1_000_003, u64::from(u32::MAX)] {
        for slot in [0u32, 1, 399, u32::MAX] {
            assert!(load_gate(tick, slot, 1000));
            assert!(!load_gate(tick, slot, 0));
        }
    }
}

#[test]
fn half_rate_alternates() {
    let hits: Vec<bool> = (0..6).map(|t| load_gate(t, 0, 500)).collect();
    assert_eq!(hits, [false, true, false, true, false, true]);
}

proptest! {
    #[test]
    fn count_over_a_thousand_ticks(t0 in 0u64..10_000_000, slot in 0u32..20_000, p in 0u16..=1000) {
        let count = (0..1000).filter(|k| load_gate(t0 + k, slot, p)).count() as u64;
        let i = u64::from(slot);
        let want = (t0 + 1000 + i) * u64::from(p) / 1000 - (t0 + i) * u64::from(p) / 1000;
        prop_assert_eq!(count, want);
    }

    #[test]
    fn gate_is_a_spread_not_a_burst(t0 in 0u64..1_000_000, slot in 0u32..20_000, p in 1u16..1000) {
        // Between two true ticks the gap never exceeds ceil(1000 / p).
        let limit = 1000u64.div_ceil(u64::from(p));
        let first = (t0..).find(|t| load_gate(*t, slot, p)).unwrap();
        prop_assert!(first - t0 < limit);
    }
}
