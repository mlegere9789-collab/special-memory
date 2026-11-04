//! T12-04: gradient, horizon and shadow (SPEC-012 section 6 items 2 to 4).

use acoustics_model::propagation::{gradient, horizon_mm, shadow_cdb};
use acoustics_model::AcousticTables;

fn tables() -> AcousticTables {
    AcousticTables::from_toml_str(include_str!("../../../data/rules/acoustics.toml")).unwrap()
}

#[test]
fn gradient_of_the_shipped_presets() {
    let t = tables();
    assert_eq!(gradient(&t, -20, -6000), -432);
    assert_eq!(gradient(&t, 30, 4000), 298);
    assert_eq!(gradient(&t, 0, 0), 0);
    assert_eq!(gradient(&t, -20, 0), -12);
}

#[test]
fn horizon_of_a_circular_ray() {
    let t = tables();
    assert_eq!(horizon_mm(&t, 343_420, -100, 1500, 1500), Some(203_002));
    // The limit of a circular ray is the same for both signs.
    assert_eq!(horizon_mm(&t, 343_420, 100, 1500, 1500), Some(203_002));
    // A source on the ground still reaches the listener's own horizon.
    let one_sided = horizon_mm(&t, 343_420, -100, 0, 1500).unwrap();
    assert!(one_sided > 0 && one_sided < 203_002);
}

#[test]
fn weak_gradient_has_no_horizon() {
    let t = tables();
    for g in [-4, -1, 0, 1, 4] {
        assert_eq!(horizon_mm(&t, 343_420, g, 1500, 1500), None, "g = {g}");
    }
    assert!(horizon_mm(&t, 343_420, 5, 1500, 1500).is_some());
    assert!(horizon_mm(&t, 343_420, -5, 1500, 1500).is_some());
}

#[test]
fn shadow_is_zero_at_the_horizon_and_never_decreases() {
    let t = tables();
    let d_h = 203_002;
    for band in 0..3 {
        assert_eq!(shadow_cdb(&t, band, -432, d_h, Some(d_h)), 0);
        assert_eq!(shadow_cdb(&t, band, -432, d_h - 1, Some(d_h)), 0);
        let cap = t.bands()[band].shadow_cap_cdb as i32;
        let mut last = 0;
        for k in 0..400 {
            let d = d_h + 1 + k * 250_000;
            let s = shadow_cdb(&t, band, -432, d, Some(d_h));
            assert!(s >= last, "band {band} d {d}: {s} < {last}");
            assert!(s <= cap, "band {band} d {d}: {s} above the cap {cap}");
            last = s;
        }
        assert_eq!(
            last, cap,
            "band {band} reaches its cap far beyond the horizon"
        );
    }
}

#[test]
fn shadow_grows_with_the_slope_just_beyond_the_horizon() {
    let t = tables();
    // 100 m beyond the horizon: slope * 100000 / 10^6.
    for band in 0..3 {
        let slope = t.bands()[band].shadow_slope_cdb_per_km as i32;
        assert_eq!(
            shadow_cdb(&t, band, -432, 303_002, Some(203_002)),
            slope / 10,
            "band {band}"
        );
    }
}

#[test]
fn weak_gradient_gives_a_weak_shadow() {
    let t = tables();
    let far = 50_000_000;
    for band in 0..3 {
        let cap = t.bands()[band].shadow_cap_cdb as i32;
        // Below saturation the cap scales with |g| / 300; at and above it the cap is whole.
        assert_eq!(shadow_cdb(&t, band, -150, far, Some(203_002)), cap / 2);
        assert_eq!(shadow_cdb(&t, band, -30, far, Some(203_002)), cap / 10);
        assert_eq!(shadow_cdb(&t, band, -300, far, Some(203_002)), cap);
        assert_eq!(shadow_cdb(&t, band, -900, far, Some(203_002)), cap);
    }
}

#[test]
fn no_shadow_without_a_downward_gradient_or_a_horizon() {
    let t = tables();
    assert_eq!(shadow_cdb(&t, 1, 100, 50_000_000, Some(203_002)), 0);
    assert_eq!(shadow_cdb(&t, 1, 0, 50_000_000, None), 0);
    assert_eq!(shadow_cdb(&t, 1, -2, 50_000_000, None), 0);
}
