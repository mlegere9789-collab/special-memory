//! Loader half of the T8-01 style checks for `fidelity.toml` (SPEC-008 section 2.1): the file
//! loads with the spec values and every violation returns its named `DataError`.

use battle_rules::{Arm, Confidence, DataError, FidelityClass, FidelityTable, F0, F1, F2};

const FIDELITY: &str = include_str!("../../../data/rules/fidelity.toml");

fn mutate(from: &str, to: &str) -> Result<FidelityTable, DataError> {
    assert!(FIDELITY.contains(from), "fixture text lacks {from:?}");
    FidelityTable::from_toml_str(&FIDELITY.replacen(from, to, 1))
}

#[test]
fn table_loads_with_the_spec_values() {
    let t = FidelityTable::from_toml_str(FIDELITY).unwrap();
    let g = &t.globals;
    assert_eq!(
        [
            g.assign_interval_ticks,
            g.min_dwell_ticks,
            g.bundle_ticks,
            g.f2_interval_ticks,
            g.hot_margin_sectors
        ],
        [20, 200, 20, 100, 1]
    );
    assert_eq!(g.hysteresis_mm, 50_000);
    assert_eq!((g.sector_mm, g.grid_cell_mm), (500_000, 4000));
    assert_eq!(g.max_grid_cells, 4_194_304);
    for (arm, f0, f1) in [
        (Arm::Infantry, 450_000, 1_800_000),
        (Arm::Cavalry, 450_000, 1_800_000),
        (Arm::Battery, 1_200_000, 2_400_000),
    ] {
        let r = t.arm_row(arm).unwrap();
        assert_eq!((r.arm, r.f0_mm, r.f1_mm), (arm, f0, f1));
    }
    assert!(t.arm_row(Arm::Hq).is_none());
}

#[test]
fn every_row_has_provenance_and_a_fidelity_target() {
    let t = FidelityTable::from_toml_str(FIDELITY).unwrap();
    let all = t
        .arms
        .iter()
        .map(|r| &r.provenance)
        .chain([&t.globals.provenance]);
    for p in all {
        assert_eq!(p.calibration_spec, Some(8));
        assert!(matches!(p.calibration_target, Some(80..=88)));
        assert_ne!(p.confidence, Confidence::Verified);
    }
    assert!(t.arms.iter().all(|r| !r.provenance.sources.is_empty()));
}

#[test]
fn classes_are_ordered_and_numbered() {
    assert_eq!(
        [F0, F1, F2],
        [FidelityClass(0), FidelityClass(1), FidelityClass(2)]
    );
    assert!(F0 < F1 && F1 < F2);
}

#[test]
fn float_is_refused() {
    let e = mutate("hysteresis_mm = 50000", "hysteresis_mm = 50000.0").unwrap_err();
    assert!(matches!(e, DataError::Float(p) if p == "globals.hysteresis_mm"));
}

#[test]
fn unknown_keys_are_refused() {
    let e = mutate("sector_mm = 500000", "sector_mm = 500000\nextra = 1").unwrap_err();
    assert!(matches!(e, DataError::UnknownKey(p) if p == "globals.extra"));
    let e = mutate("f0_mm = 450000", "f0_mm = 450000\nextra = 1").unwrap_err();
    assert!(matches!(e, DataError::UnknownKey(p) if p == "arm[0].extra"));
}

#[test]
fn provenance_violations_are_refused() {
    let e = mutate("calibration_target = \"CT-008-80\"", "").unwrap_err();
    assert!(matches!(e, DataError::Provenance(_)), "{e:?}");
    let e = mutate(
        "calibration_target = \"CT-008-80\"",
        "calibration_target = \"CT-80\"",
    )
    .unwrap_err();
    assert!(matches!(e, DataError::Provenance(_)), "{e:?}");
    let e = mutate("confidence = \"partial\"", "confidence = \"verified\"").unwrap_err();
    assert!(matches!(e, DataError::Provenance(_)), "{e:?}");
}

#[test]
fn values_outside_their_range_are_refused() {
    for (from, to) in [
        ("assign_interval_ticks = 20", "assign_interval_ticks = 0"),
        ("bundle_ticks = 20", "bundle_ticks = 0"),
        ("f2_interval_ticks = 100", "f2_interval_ticks = 0"),
        ("sector_mm = 500000", "sector_mm = 0"),
        ("sector_mm = 500000", "sector_mm = 12000001"),
        ("grid_cell_mm = 4000", "grid_cell_mm = 0"),
        ("max_grid_cells = 4194304", "max_grid_cells = 0"),
        ("hysteresis_mm = 50000", "hysteresis_mm = -1"),
        ("min_dwell_ticks = 200", "min_dwell_ticks = -1"),
        ("hot_margin_sectors = 1", "hot_margin_sectors = 100"),
        ("f0_mm = 450000", "f0_mm = 0"),
    ] {
        let e = mutate(from, to).unwrap_err();
        assert!(matches!(e, DataError::Range(_)), "{from}: {e:?}");
    }
}

#[test]
fn structure_violations_are_refused() {
    // The first row's F1 limit must exceed its F0 limit.
    let e = mutate("f1_mm = 1800000", "f1_mm = 450000").unwrap_err();
    assert!(
        matches!(&e, DataError::Range(m) if m.contains("f1_mm")),
        "{e:?}"
    );
    let e = mutate("id = \"infantry\"", "id = \"cavalry\"").unwrap_err();
    assert!(matches!(e, DataError::Range(m) if m.contains("repeated")));
    let e = mutate("id = \"infantry\"", "id = \"hq\"").unwrap_err();
    assert!(matches!(e, DataError::Range(m) if m.contains("arm")));
    let e = mutate("schema = \"fidelity/1\"", "schema = \"fidelity/2\"").unwrap_err();
    assert!(matches!(e, DataError::Parse(_)));
    let cut = FIDELITY.rfind("[[arm]]").unwrap();
    let e = FidelityTable::from_toml_str(&FIDELITY[..cut]).unwrap_err();
    assert!(matches!(e, DataError::Range(m) if m.contains("every")));
}
