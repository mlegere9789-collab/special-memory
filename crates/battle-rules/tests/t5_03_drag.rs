//! T5-03: the drag constant `k = rho * cd * A / (2 m)` computed at load in integers agrees with
//! an `f64` evaluation to 1e-6 relative (SPEC-005 sections 2 and 11).

use battle_rules::{drag_k, sphere_mass_g, AmmoTable};

const AMMO: &str = include_str!("../../../data/rules/ammo.toml");

/// Independent f64 evaluation, units: kg/m3, m, kg.
fn k_f64(rho_g_m3: u32, cd_pm: u32, d_mm: u32, mass_g: u32) -> f64 {
    let rho = f64::from(rho_g_m3) / 1000.0;
    let cd = f64::from(cd_pm) / 1000.0;
    let d = f64::from(d_mm) / 1000.0;
    let m = f64::from(mass_g) / 1000.0;
    let area = std::f64::consts::PI * d * d / 4.0;
    rho * cd * area / (2.0 * m)
}

fn assert_close(raw: i64, want: f64, what: &str) {
    let got = raw as f64 / 4_294_967_296.0;
    let rel = ((got - want) / want).abs();
    assert!(rel < 1e-6, "{what}: got {got}, want {want}, rel {rel}");
}

#[test]
fn every_ammo_row_and_pellet_matches_f64() {
    let t = AmmoTable::from_toml_str(AMMO).unwrap();
    let rho = t.constants().air_density_g_m3;
    for r in t.rows() {
        assert_close(
            r.drag_k.raw(),
            k_f64(rho, r.cd_pm, r.diameter_mm, r.mass_g),
            &r.id,
        );
        if let Some(b) = &r.burst {
            assert_close(
                b.pellet_drag_k.raw(),
                k_f64(rho, b.pellet_cd_pm, b.pellet_diameter_mm, b.pellet_mass_g),
                &format!("{} pellet", r.id),
            );
        }
    }
}

#[test]
fn free_function_matches_f64_over_a_grid() {
    for cd in [100, 300, 470, 1000] {
        for d in [11, 17, 38, 117] {
            for m in [9, 32, 208, 5443] {
                let want = k_f64(1225, cd, d, m);
                // Skip combinations whose k is too small to be held to 1e-6 by 2^-32 steps.
                if want * 4_294_967_296.0 < 1e7 {
                    continue;
                }
                assert_close(drag_k(1225, cd, d, m).raw(), want, "grid");
            }
        }
    }
}

#[test]
fn zero_drag_and_zero_mass() {
    assert_eq!(drag_k(1225, 0, 17, 27).raw(), 0);
    assert_eq!(drag_k(1225, 470, 17, 0), core_math::Fx::MAX);
}

#[test]
fn canister_pellet_mass_comes_from_iron_density_and_volume() {
    let t = AmmoTable::from_toml_str(AMMO).unwrap();
    let b = t.get("canister_12").unwrap().burst.as_ref().unwrap();
    // 7.2 g/cm3 * (pi/6) * (3.8 cm)^3 = 206.9 g, stored as the nearest whole gram.
    let want = (7.2_f64 * std::f64::consts::PI / 6.0 * 3.8_f64.powi(3)).round() as u32;
    assert_eq!(b.pellet_mass_g, want);
    assert_eq!(b.pellet_mass_g, 207);
    assert_eq!(sphere_mass_g(7200, 38), 207);
    // The fragments of a shell carry their stated mass.
    assert_eq!(
        t.get("shell_12")
            .unwrap()
            .burst
            .as_ref()
            .unwrap()
            .pellet_mass_g,
        60
    );
    // Case pellets are iron too: 7.2 * pi/6 * 1.7^3 = 18.5 g.
    assert_eq!(
        t.get("case_12")
            .unwrap()
            .burst
            .as_ref()
            .unwrap()
            .pellet_mass_g,
        19
    );
}

#[test]
fn changing_the_iron_density_changes_the_derived_mass() {
    let text = AMMO.replacen("iron_density_kg_m3 = 7200", "iron_density_kg_m3 = 7800", 1);
    let t = AmmoTable::from_toml_str(&text).unwrap();
    let b = t.get("canister_12").unwrap().burst.as_ref().unwrap();
    assert_eq!(b.pellet_mass_g, sphere_mass_g(7800, 38));
    assert!(b.pellet_mass_g > 207);
}
