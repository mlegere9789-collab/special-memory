//! Cumulative-permille lookup, bands, regions, disable time and `energy_joules` (SPEC-005
//! sections 1, 5 and T5-10's golden).

use battle_rules::{energy_joules, fx_ratio, DamageTable, DataError, HitOutcome, Region};
use core_math::Fx;

const DAMAGE: &str = include_str!("../../../data/rules/damage.toml");

fn table() -> DamageTable {
    DamageTable::from_toml_str(DAMAGE).unwrap()
}

const REGIONS: [Region; 3] = [Region::Legs, Region::Torso, Region::Head];

#[test]
fn boundaries_of_a_cell() {
    let d = table();
    // band 0 legs: 0 killed, 50 wounded, 200 disabled permille.
    let o = |roll| d.outcome(0, Region::Legs, roll);
    assert_eq!(o(0), HitOutcome::Wounded);
    assert_eq!(o(49_999), HitOutcome::Wounded);
    assert_eq!(o(50_000), HitOutcome::Disabled);
    assert_eq!(o(249_999), HitOutcome::Disabled);
    assert_eq!(o(250_000), HitOutcome::NoEffect);
    assert_eq!(o(999_999), HitOutcome::NoEffect);
    // The roll is reduced modulo 10^6.
    assert_eq!(o(1_000_000), o(0));
    assert_eq!(o(1_250_000), HitOutcome::NoEffect);
    assert_eq!(o(u64::MAX), o(u64::MAX % 1_000_000));
}

#[test]
fn a_cell_that_sums_to_1000_never_gives_no_effect() {
    let d = table();
    // band 3 head: 950 / 50 / 0.
    assert_eq!(d.outcome(3, Region::Head, 949_999), HitOutcome::Killed);
    assert_eq!(d.outcome(3, Region::Head, 950_000), HitOutcome::Wounded);
    assert_eq!(d.outcome(3, Region::Head, 999_999), HitOutcome::Wounded);
}

#[test]
fn every_cell_reproduces_its_permille_exactly_over_a_full_sweep() {
    let d = table();
    for band in 0..4u8 {
        for region in REGIONS {
            let c = d.cell(band, region);
            let mut n = [0u32; 4];
            for roll in 0..1_000_000u64 {
                n[d.outcome(band, region, roll) as usize] += 1;
            }
            assert_eq!(n[HitOutcome::Killed as usize], c.killed_pm * 1000);
            assert_eq!(n[HitOutcome::Wounded as usize], c.wounded_pm * 1000);
            assert_eq!(n[HitOutcome::Disabled as usize], c.disabled_pm * 1000);
            let rest = 1000 - c.killed_pm - c.wounded_pm - c.disabled_pm;
            assert_eq!(n[HitOutcome::NoEffect as usize], rest * 1000);
        }
    }
}

#[test]
fn severity_order() {
    assert!(HitOutcome::Killed > HitOutcome::Wounded);
    assert!(HitOutcome::Wounded > HitOutcome::Disabled);
    assert!(HitOutcome::Disabled > HitOutcome::NoEffect);
}

#[test]
fn band_above_three_uses_the_top_band() {
    let d = table();
    for roll in [0, 500_000, 999_999] {
        assert_eq!(
            d.outcome(9, Region::Torso, roll),
            d.outcome(3, Region::Torso, roll)
        );
    }
}

#[test]
fn lethality_rises_with_energy_in_the_data() {
    let d = table();
    for region in REGIONS {
        let mut prev = 0;
        for band in 0..4 {
            let k = d.cell(band, region).killed_pm;
            assert!(k >= prev, "{region:?} band {band}");
            prev = k;
        }
    }
}

#[test]
fn energy_bands() {
    let d = table();
    let at = |num, den| d.band_for_energy(fx_ratio(num, den));
    assert_eq!(at(0, 1), 0);
    assert_eq!(at(149_999, 1000), 0);
    assert_eq!(at(150, 1), 1);
    assert_eq!(at(499_999, 1000), 1);
    assert_eq!(at(500, 1), 2);
    assert_eq!(at(1499, 1), 2);
    assert_eq!(at(1500, 1), 3);
    assert_eq!(d.band_for_energy(Fx::MAX), 3);
    assert_eq!(d.band_for_energy(fx_ratio(-5, 1)), 0);
}

#[test]
fn hit_height_regions() {
    let d = table();
    assert_eq!(d.region_for_height(0), Region::Legs);
    assert_eq!(d.region_for_height(499), Region::Legs);
    assert_eq!(d.region_for_height(500), Region::Torso);
    assert_eq!(d.region_for_height(849), Region::Torso);
    assert_eq!(d.region_for_height(850), Region::Head);
    assert_eq!(d.region_for_height(1000), Region::Head);
}

#[test]
fn disabled_ticks_cover_600_to_2400() {
    let d = table();
    assert_eq!(d.disabled_ticks(0), 600);
    assert_eq!(d.disabled_ticks(1800), 2400);
    assert_eq!(d.disabled_ticks(1801), 600);
    let (mut lo, mut hi, mut sum) = (u32::MAX, 0, 0u64);
    for roll in 0..1801u64 {
        let t = d.disabled_ticks(roll);
        lo = lo.min(t);
        hi = hi.max(t);
        sum += u64::from(t);
    }
    assert_eq!((lo, hi), (600, 2400));
    assert_eq!(sum, 1801 * 1500);
}

#[test]
fn energy_joules_golden() {
    // 32 g at 290 m/s: 32 * 290^2 / 2000 = 1345.6 J.
    let e = energy_joules(32, Fx::from_int(290));
    assert_eq!(e, fx_ratio(32 * 290 * 290, 2000));
    assert_eq!(e.raw(), 5_779_307_993_497);
}

#[test]
fn energy_joules_edges() {
    assert_eq!(energy_joules(0, Fx::from_int(300)), Fx::ZERO);
    assert_eq!(energy_joules(32, Fx::ZERO), Fx::ZERO);
    assert_eq!(energy_joules(u32::MAX, Fx::MAX), Fx::MAX);
    // Direction does not matter.
    assert_eq!(
        energy_joules(32, Fx::from_int(-290)),
        energy_joules(32, Fx::from_int(290))
    );
    // A 12-pounder shot at 452.628 m/s carries about 557 kJ.
    let e = energy_joules(5443, fx_ratio(452_628, 1000));
    assert!((557_000..558_500).contains(&e.to_int_floor()));
}

#[test]
fn a_missing_cell_is_a_parse_or_range_error() {
    let text = DAMAGE.replacen(
        "band = 0\nregion = \"legs\"",
        "band = 0\nregion = \"torso\"",
        1,
    );
    let e = DamageTable::from_toml_str(&text).unwrap_err();
    assert!(matches!(e, DataError::Parse(_) | DataError::Range(_)));
}
