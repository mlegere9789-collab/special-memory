//! T5-02: metre values equal `yd * 914.4` and `ft * 304.8` exactly (SPEC-005 section 1).
//! All arithmetic here is integer: 914.4 mm = 9144 / 10 and 304.8 mm = 3048 / 10.

use battle_rules::{AmmoTable, WeaponTable};

const AMMO: &str = include_str!("../../../data/rules/ammo.toml");
const WEAPONS: &str = include_str!("../../../data/rules/weapons.toml");

/// `yd * 914.4` in whole millimetres, asserting the product is exact.
fn yd_mm(yd: u32) -> u32 {
    assert_eq!((yd * 9144) % 10, 0, "{yd} yd is not a whole number of mm");
    yd * 9144 / 10
}

/// `ft * 304.8` in whole millimetres, asserting the product is exact.
fn ft_mm(ft: u32) -> u32 {
    assert_eq!((ft * 3048) % 10, 0, "{ft} ft is not a whole number of mm");
    ft * 3048 / 10
}

#[test]
fn sourced_ranges_are_exact_yard_conversions() {
    let w = WeaponTable::from_toml_str(WEAPONS).unwrap();
    // (weapon, effective yd, extended yd) as the research page gives them.
    for (id, eff, ext) in [
        ("smoothbore_69", 50, 200),
        ("rifle_58", 300, 880), // half a mile
        ("napoleon_12", 350, 1680),
    ] {
        let r = w.get(id).unwrap();
        assert_eq!(r.effective_mm, yd_mm(eff), "{id} effective");
        assert_eq!(r.extended_mm, yd_mm(ext), "{id} extended");
    }
    assert_eq!(w.get("rifle_58").unwrap().effective_mm, 274_320);
    assert_eq!(w.get("rifle_58").unwrap().extended_mm, 804_672);
}

#[test]
fn shot_muzzle_velocity_is_an_exact_foot_conversion() {
    let a = AmmoTable::from_toml_str(AMMO).unwrap();
    assert_eq!(a.get("shot_12").unwrap().muzzle_mm_s, ft_mm(1485));
    assert_eq!(ft_mm(1485), 452_628);
}

#[test]
fn pound_masses_round_to_the_stored_grams() {
    // 1 lb = 453.592 g; masses are stored in whole grams.
    let a = AmmoTable::from_toml_str(AMMO).unwrap();
    let lb_g = |milli_lb: u64| ((milli_lb * 453_592 + 500_000) / 1_000_000) as u32;
    assert_eq!(a.get("shot_12").unwrap().mass_g, lb_g(12_000));
    assert_eq!(a.get("shell_12").unwrap().mass_g, lb_g(12_300));
}
