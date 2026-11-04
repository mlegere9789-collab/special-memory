//! T5-10: damage rolls reproduce the table; disabled ticks; energy (SPEC-005 sections 1 and 5).

mod t023_support;

use battle_rules::{energy_joules, DamageTable, HitOutcome, Region};
use core_math::{Fx, SplitMix64};
use t023_support::{bullet, field_of, fire_through, m, man, tables};

#[test]
fn every_band_and_region_reproduces_the_table_within_half_a_point() {
    let t: DamageTable = tables().damage;
    let mut sm = SplitMix64::new(77);
    for band in 0..4u8 {
        for region in [Region::Legs, Region::Torso, Region::Head] {
            let mut n = [0i64; 4];
            for _ in 0..200_000 {
                n[t.outcome(band, region, sm.next_u64()) as usize] += 1;
            }
            let cell = t.cell(band, region);
            let want = [
                0,
                i64::from(cell.disabled_pm),
                i64::from(cell.wounded_pm),
                i64::from(cell.killed_pm),
            ];
            for k in 1..4 {
                // Permille of 200,000 rolls: 0.5 points is 5 permille = 1000 rolls.
                let got = n[k];
                let target = want[k] * 200;
                assert!(
                    (got - target).abs() <= 1000,
                    "band {band} {region:?} outcome {k}"
                );
            }
        }
    }
}

#[test]
fn energy_golden() {
    let e = energy_joules(32, Fx::from_int(290));
    assert_eq!(i128::from(e.raw()), ((32i128 * 290 * 290) << 32) / 2000);
}

#[test]
fn the_hit_path_gives_the_table_outcomes_and_disabled_ticks() {
    // 27 g ball at 290 m/s is 1135 J (band 2); a hit at 300 mm of a 1700 mm man is the legs.
    let mut b = field_of(1, '.');
    let t = man(&mut b, 1, m(100, 200), 0);
    let at = b.world.agents.pos(t).unwrap();
    let mut seeds = SplitMix64::new(5);
    let (mut killed, mut wounded, mut disabled) = (0i64, 0i64, 0i64);
    let (mut sum, mut lo, mut hi) = (0u64, u32::MAX, 0u32);
    for _ in 0..200_000 {
        let mut p = bullet(&b, "ball_69", 999, 77, seeds.next_u64());
        let mut out = Vec::new();
        fire_through(&b, &mut p, at, 300, 50_000, 290, &mut out);
        assert!(out.len() <= 1);
        for h in out {
            match h.outcome {
                HitOutcome::Killed => killed += 1,
                HitOutcome::Wounded => wounded += 1,
                HitOutcome::Disabled => {
                    disabled += 1;
                    sum += u64::from(h.disabled_ticks);
                    lo = lo.min(h.disabled_ticks);
                    hi = hi.max(h.disabled_ticks);
                }
                HitOutcome::NoEffect => panic!("no-effect hits are not queued"),
            }
        }
    }
    // Band 2, legs: 50 / 800 / 100 permille.
    assert!((killed - 10_000).abs() <= 1000, "killed {killed}");
    assert!((wounded - 160_000).abs() <= 1000, "wounded {wounded}");
    assert!((disabled - 20_000).abs() <= 1000, "disabled {disabled}");
    assert!(lo >= 600 && hi <= 2400, "{lo}..{hi}");
    let mean = sum / disabled as u64;
    assert!((1470..=1530).contains(&mean), "mean {mean}");
}
