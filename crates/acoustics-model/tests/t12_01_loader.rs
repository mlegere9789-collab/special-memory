//! T12-01: the committed `acoustics.toml` loads with the SPEC-012 section 3 values, and each broken
//! variant returns its named `DataError`.

use acoustics_model::AcousticTables;
use battle_rules::{Confidence, DataError};

const TEXT: &str = include_str!("../../../data/rules/acoustics.toml");

/// Replaces the first occurrence of `from`, panicking if the fixture text changed.
fn swap(from: &str, to: &str) -> String {
    assert!(TEXT.contains(from), "fixture lacks {from:?}");
    TEXT.replacen(from, to, 1)
}

fn load(text: &str) -> Result<AcousticTables, DataError> {
    AcousticTables::from_toml_str(text)
}

#[test]
fn committed_file_loads_with_spec_values() {
    let t = load(TEXT).unwrap();
    let g = t.globals();
    assert_eq!(
        (g.c0_mm_s, g.c_per_k_mm_s, g.shear_ppm),
        (331_300, 606, 70_000)
    );
    assert_eq!(
        (g.g_min, g.shadow_sat_g, g.duct_loss_cdb_per_km),
        (5, 300, 100)
    );
    assert_eq!(
        (g.barrier_cap_cdb, g.profile_step_mm, g.max_samples),
        (2400, 20_000, 64)
    );
    assert_eq!(
        (g.max_range_mm, g.cluster_cell_mm, g.hyst_cdb),
        (100_000_000, 100_000, 300)
    );
    assert_eq!((g.drop_below_cdb, g.class_faint_cdb), (-1000, 0));
    assert_eq!((g.class_clear_cdb, g.class_loud_cdb), (1000, 3000));
    assert_eq!((g.camera_window_ticks, g.hq_window_ticks), (4, 20));
    assert_eq!(t.air_provenance().confidence, Confidence::Partial);
    assert!(!t.air_provenance().sources.is_empty());

    let b = t.bands();
    assert_eq!([b[0].f_hz, b[1].f_hz, b[2].f_hz], [125, 500, 2000]);
    assert_eq!(
        [
            b[0].abs_cdb_per_km,
            b[1].abs_cdb_per_km,
            b[2].abs_cdb_per_km
        ],
        [40, 280, 900]
    );
    assert_eq!(
        [b[0].ambient_cdb, b[1].ambient_cdb, b[2].ambient_cdb],
        [4000, 3500, 3000]
    );
    assert_eq!(
        [b[0].foliage_pm, b[1].foliage_pm, b[2].foliage_pm],
        [200, 1000, 2000]
    );
    assert_eq!(b[2].shadow_slope_cdb_per_km, 4000);
    assert_eq!(b[2].shadow_cap_cdb, 6000);
    assert_eq!(b[1].provenance.calibration_target, Some(3));
}

#[test]
fn sources_ground_weather_budget() {
    let t = load(TEXT).unwrap();
    let s = t.sources();
    assert_eq!(s.len(), 9);
    assert_eq!(s[0].name, "musket");
    assert_eq!(s[0].level_cdb, [10_600, 10_800, 10_200]);
    assert_eq!(s[3].name, "gun_launch");
    assert_eq!((s[3].class, s[3].level_cdb), (1, [13_000, 12_800, 11_600]));
    assert_eq!(s[6].priority_cdb, 1500);
    assert_eq!(s[5].priority_cdb, 0);
    let classes: Vec<u8> = s.iter().map(|r| r.class).collect();
    assert_eq!(classes, [0, 0, 0, 1, 2, 2, 3, 3, 3]);

    let ground = |c: u8| {
        let g = t.ground(c).unwrap();
        (g.canopy_mm, g.foliage_cdb_per_100m)
    };
    assert_eq!(ground(2), (6000, 150));
    assert_eq!(ground(3), (15_000, 600));
    assert_eq!(ground(4), (3000, 900));
    assert_eq!(ground(0), (0, 0));
    assert_eq!(ground(7), (0, 0));
    assert!(
        t.ground(200).is_none(),
        "a class without a row has no foliage"
    );

    let w = t.weather("toward_minus_x").unwrap();
    assert_eq!(
        (w.temp_dc, w.wind_x_mm_s, w.wind_y_mm_s, w.lapse_mk_per_m),
        (200, -6000, 0, -20)
    );
    let w = t.weather("toward_plus_x_inversion").unwrap();
    assert_eq!((w.wind_x_mm_s, w.lapse_mk_per_m), (4000, 30));
    assert_eq!(t.weather("calm_sunny").unwrap().lapse_mk_per_m, -20);
    assert!(t.weather("calm").is_some() && t.weather("nope").is_none());

    let bu = t.budget();
    assert_eq!(bu.max_total, 32);
    assert_eq!(bu.class_caps, [12, 10, 6, 8]);
}

#[test]
fn float_is_refused() {
    let e = load(&swap("g_min = 5", "g_min = 5.0")).unwrap_err();
    assert!(matches!(e, DataError::Float(p) if p == "globals.propagation.g_min"));
}

#[test]
fn unknown_keys_are_refused() {
    let e = load(&swap("c_per_k_mm_s = 606", "c_per_k_mm_s = 606\nbogus = 1")).unwrap_err();
    assert!(matches!(e, DataError::UnknownKey(p) if p == "globals.air.bogus"));
    let e = load(&format!("{TEXT}\n[extra]\nx = 1\n")).unwrap_err();
    assert!(matches!(e, DataError::UnknownKey(p) if p == "extra"));
    let e = load(&swap("f_hz = 125", "f_hz = 125\nwidth_hz = 3")).unwrap_err();
    assert!(matches!(e, DataError::UnknownKey(p) if p == "band[0].width_hz"));
}

#[test]
fn provenance_violations_are_refused() {
    // A placeholder without a calibration target.
    let e = load(&swap(
        "placeholder = true\ncalibration_target = \"CT-012-1\"",
        "placeholder = true",
    ))
    .unwrap_err();
    assert!(matches!(e, DataError::Provenance(_)), "{e}");
    // A non-placeholder without sources.
    let e = load(&swap("sources = [{ document = \"CWV\", page = 457, note = \"343 m/s; the linear law gives 343420 mm/s at 20 C\" }]\n", "")).unwrap_err();
    assert!(matches!(e, DataError::Provenance(_)), "{e}");
    // Verified without a second source.
    let e = load(&swap(
        "confidence = \"partial\"",
        "confidence = \"verified\"",
    ))
    .unwrap_err();
    assert!(matches!(e, DataError::Provenance(_)), "{e}");
    // A bare calibration id.
    let e = load(&swap("CT-012-5", "CT-5")).unwrap_err();
    assert!(matches!(e, DataError::Provenance(_)), "{e}");
}

#[test]
fn structure_violations_are_refused() {
    let e = load(&swap("acoustics/1", "acoustics/2")).unwrap_err();
    assert!(matches!(e, DataError::Parse(_)));
    let e = load(&swap("g_min = 5\n", "")).unwrap_err();
    assert!(matches!(e, DataError::Parse(m) if m.contains("globals.propagation.g_min")));
    let e = load(&swap("g_min = 5", "g_min = 0")).unwrap_err();
    assert!(matches!(e, DataError::Range(_)));
    let e = load(&swap("f_hz = 500", "f_hz = 100")).unwrap_err();
    assert!(
        matches!(e, DataError::Range(_)),
        "bands ascend in frequency"
    );
    let e = load(&swap(
        "id = 3\nname = \"gun_launch\"",
        "id = 4\nname = \"gun_launch\"",
    ))
    .unwrap_err();
    assert!(
        matches!(e, DataError::Range(_)),
        "source ids follow their position"
    );
    let e = load(&swap("class_id = 1\n", "class_id = 0\n")).unwrap_err();
    assert!(matches!(e, DataError::Range(_)), "ground ids ascend");
    let e = load(&swap("id = \"calm_sunny\"", "id = \"calm\"")).unwrap_err();
    assert!(matches!(e, DataError::Parse(_)), "weather ids are unique");
    let e = load(&swap("class = 3", "class = 4")).unwrap_err();
    assert!(matches!(e, DataError::Range(_)));
    let e = load(&swap("hq_window_ticks = 20", "hq_window_ticks = 0")).unwrap_err();
    assert!(matches!(e, DataError::Range(_)));
    let e = load(&swap("class_clear_cdb = 1000", "class_clear_cdb = -5")).unwrap_err();
    assert!(matches!(e, DataError::Range(_)), "class thresholds ascend");
}
