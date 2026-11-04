//! T5-09: terrain cover absorbs a share of hits that depends on the posture (SPEC-005 section 5).

mod t023_support;

use battle_rules::F_PRONE;
use core_math::SplitMix64;
use t023_support::{bullet, field_of, fire_through, m, man};

/// Share of 20,000 hits on a lone target in `terrain` that cover absorbs, in permille.
fn absorbed_pm(terrain: char, flags: u16, z_mm: i64) -> i64 {
    let mut b = field_of(1, terrain);
    let t = man(&mut b, 1, m(100, 200), flags);
    let at = b.world.agents.pos(t).unwrap();
    let mut seeds = SplitMix64::new(0xC0FFEE);
    let mut absorbed = 0;
    let mut out = Vec::new();
    for _ in 0..20_000 {
        let mut p = bullet(&b, "minie_58", 999, 77, seeds.next_u64());
        let r = fire_through(&b, &mut p, at, z_mm, 50_000, 290, &mut out);
        assert_eq!(r.struck, 1);
        if p.hits == 0 {
            absorbed += 1;
        }
    }
    absorbed * 1000 / 20_000
}

#[test]
fn woods_absorb_15_percent_of_hits_on_a_standing_man() {
    let share = absorbed_pm('W', 0, 1200);
    assert!((135..=165).contains(&share), "{share} permille");
}

#[test]
fn woods_absorb_30_percent_of_hits_on_a_prone_man() {
    let share = absorbed_pm('W', F_PRONE, 300);
    assert!((285..=315).contains(&share), "{share} permille");
}

#[test]
fn open_ground_absorbs_nothing() {
    assert_eq!(absorbed_pm('.', 0, 1200), 0);
}
