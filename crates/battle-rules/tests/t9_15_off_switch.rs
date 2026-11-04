//! T9-15: with `env = None` the T4-12 hashes are unchanged (SPEC-009 section 1). The full step
//! and the movement-only step agree on a battle with no fire, and neither reads the weather
//! timeline.

mod t019_support;

use battle_rules::scenarios;
use t019_support::tables;

/// `(scenario, ticks, seed, hash)` as recorded by `t4_12_determinism.rs`.
const LITERALS: [(&str, u64, u64, u64); 2] = [
    ("march_single", 4000, 1, 0x49a6_1801_5d2d_9397),
    ("deploy_pair", 3000, 2, 0xbd8a_5938_d0a7_7d43),
];

#[test]
fn movement_goldens_are_unchanged_with_the_environment_off() {
    for (name, ticks, seed, want) in LITERALS {
        let mut b = scenarios::by_name(name, &tables(), seed).unwrap();
        assert!(b.env.is_none());
        for _ in 0..ticks {
            b.step_movement();
        }
        assert_eq!(b.hash(), want, "{name}");
    }
}

#[test]
fn the_full_step_with_the_environment_off_matches_the_movement_goldens() {
    let (name, ticks, seed, want) = LITERALS[0];
    let mut b = scenarios::by_name(name, &tables(), seed).unwrap();
    for _ in 0..ticks {
        b.step();
    }
    assert_eq!(b.hash(), want);
    assert!(b.env.is_none());
}
