//! T12-05: flat-ground loss terms (SPEC-012 section 6 item 4) against the `f64` formula and the
//! committed output of the independent script `qc/fixtures/ref_acoustics.py`.

use acoustics_model::propagation::{absorption_cdb, path_result, spherical_cdb, spreading_cdb};
use acoustics_model::{AcousticTables, Weather};

fn tables() -> AcousticTables {
    AcousticTables::from_toml_str(include_str!("../../../data/rules/acoustics.toml")).unwrap()
}

fn preset(t: &AcousticTables, id: &str) -> Weather {
    Weather::from_preset(t.weather(id).unwrap())
}

/// 100 distances from 1 m to 50 km, evenly spaced on a logarithmic scale, in mm.
fn distances() -> Vec<i64> {
    let step = (50_000f64).log10() / 99.0;
    (0..100)
        .map(|i| (1000.0 * 10f64.powf(f64::from(i) * step)).round() as i64)
        .collect()
}

#[test]
fn spherical_term_at_100_m_and_absorption_at_1_km() {
    let t = tables();
    assert_eq!(spherical_cdb(100_000), 4000);
    assert_eq!(spherical_cdb(1000), 0);
    assert_eq!(spherical_cdb(500), 0, "inside 1 m is never negative");
    assert_eq!(spherical_cdb(10_000), 2000);
    assert_eq!(absorption_cdb(&t, 1, 1_000_000), 280);
    assert_eq!(absorption_cdb(&t, 0, 1_000_000), 40);
    assert_eq!(absorption_cdb(&t, 2, 1_000_000), 900);
}

#[test]
fn calm_flat_loss_follows_the_inverse_square_law() {
    let t = tables();
    let calm = preset(&t, "calm");
    for d in distances() {
        let p = path_result(&t, &calm, [0, 0], 1500, [d, 0], 1500, None).unwrap();
        assert_eq!(p.distance_mm, d);
        for (b, band) in t.bands().iter().enumerate() {
            let sphere = 2000.0 * (d as f64 / 1000.0).log10();
            let want = sphere.round() as i32 + (band.abs_cdb_per_km * d / 1_000_000) as i32;
            assert!(
                (p.loss_cdb[b] - want).abs() <= 1,
                "d {d} band {b}: {} vs {want}",
                p.loss_cdb[b]
            );
        }
    }
}

#[test]
fn ducting_beats_spherical_spreading_beyond_the_horizon() {
    let t = tables();
    let calm = preset(&t, "calm");
    let duct = preset(&t, "toward_plus_x_inversion");
    // The ducting loss of 1 dB per km makes the duct worse than open spreading past about 20 km.
    for d in [1_000_000, 8_000_000, 16_000_000] {
        let a = path_result(&t, &calm, [0, 0], 1500, [d, 0], 1500, None).unwrap();
        let b = path_result(&t, &duct, [0, 0], 1500, [d, 0], 1500, None).unwrap();
        for k in 0..3 {
            assert!(b.loss_cdb[k] < a.loss_cdb[k], "d {d} band {k}");
        }
    }
    // Inside the horizon the ducted path is the spherical one.
    let a = path_result(&t, &calm, [0, 0], 1500, [50_000, 0], 1500, None).unwrap();
    let b = path_result(&t, &duct, [0, 0], 1500, [50_000, 0], 1500, None).unwrap();
    assert_eq!(a.loss_cdb, b.loss_cdb);
}

#[test]
fn spreading_is_continuous_at_the_horizon() {
    let t = tables();
    let d_h = 117_000;
    let inside = spreading_cdb(&t, d_h, 298, Some(d_h));
    let outside = spreading_cdb(&t, d_h + 1, 298, Some(d_h));
    assert!((outside - inside).abs() <= 1, "{inside} then {outside}");
    assert_eq!(spreading_cdb(&t, 100_000, 0, None), 4000);
}

#[test]
fn shadow_adds_loss_downwind_of_nothing() {
    let t = tables();
    let calm = preset(&t, "calm");
    let into = preset(&t, "toward_minus_x");
    // The listener sits on the +x side, the wind blows toward -x: away from the listener.
    for d in [1_000_000, 10_000_000, 40_000_000] {
        let a = path_result(&t, &calm, [0, 0], 1500, [d, 0], 1500, None).unwrap();
        let b = path_result(&t, &into, [0, 0], 1500, [d, 0], 1500, None).unwrap();
        for k in 0..3 {
            assert!(b.loss_cdb[k] > a.loss_cdb[k], "d {d} band {k}");
        }
    }
}

#[test]
fn reference_file_agrees_within_2_cdb() {
    let t = tables();
    let text = include_str!("../../../qc/fixtures/ref_acoustics.csv");
    let mut rows = 0;
    let mut cases = std::collections::BTreeSet::new();
    for line in text.lines().skip(1) {
        let f: Vec<&str> = line.split(',').collect();
        assert_eq!(f.len(), 9, "{line}");
        let n = |i: usize| f[i].parse::<i64>().unwrap();
        let w = preset(&t, f[1]);
        let p = path_result(&t, &w, [0, 0], n(3) as u16, [n(2), 0], n(4) as u16, None).unwrap();
        assert!(
            (p.delay_ms as i64 - n(5)).abs() <= 1,
            "{line}: delay {}",
            p.delay_ms
        );
        for b in 0..3 {
            let got = i64::from(p.loss_cdb[b]);
            assert!((got - n(6 + b)).abs() <= 2, "{line}: band {b} gives {got}");
        }
        cases.insert(f[0].to_string());
        rows += 1;
    }
    assert!(rows >= 200, "only {rows} rows");
    assert_eq!(
        cases.into_iter().collect::<Vec<_>>(),
        ["duct", "flat", "shadow", "shadow_weak"]
    );
}
