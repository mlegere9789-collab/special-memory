//! T9-15: with `env = None` the T4-12 hashes are unchanged (SPEC-009 section 1). The full step
//! and the movement-only step agree on a battle with no fire, and neither reads the weather
//! timeline. The T5-20 fire golden `cal_line` is unchanged too.

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

const F0_ROWS: &str = include_str!("../../../qc/golden-replays/expected-f0.json");

#[test]
fn the_fire_golden_is_unchanged_with_the_environment_off() {
    // T5-20: `cal_line` (rifle_58, trained, 91440 mm, dry) at tick 2400, seed 1.
    let line = F0_ROWS.lines().find(|l| l.contains("cal_line")).unwrap();
    let hex = line.split("\"final_hash\": \"").nth(1).unwrap();
    let want = u64::from_str_radix(&hex[..16], 16).unwrap();
    let mut b = scenarios::fire::cal_line(
        &tables(),
        1,
        "rifle_58",
        battle_rules::Training::Trained,
        91_440,
        false,
    );
    assert!(b.env.is_none());
    for _ in 0..2400 {
        b.step();
    }
    assert_eq!(b.hash(), want);
}
