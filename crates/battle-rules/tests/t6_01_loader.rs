//! T6-01: `morale.toml` loads; float, unknown key, provenance, range and order violations return
//! the named `DataError` (SPEC-006 section 3).

use battle_rules::morale::{MoraleKind, MoraleTable};
use battle_rules::{Confidence, DataError, Tables, Training};

const MORALE: &str = include_str!("../../../data/rules/morale.toml");

/// Replaces the first occurrence of `from`, which must exist.
fn mutate(from: &str, to: &str) -> Result<MoraleTable, DataError> {
    assert!(MORALE.contains(from), "fixture text lacks {from:?}");
    MoraleTable::from_toml_str(&MORALE.replacen(from, to, 1))
}

#[test]
fn table_loads_with_the_spec_rows() {
    let t = MoraleTable::from_toml_str(MORALE).unwrap();
    let rows = |l| {
        let r = t.training(l);
        [
            r.base_ppm,
            r.cap_pm,
            r.up_pm,
            r.rally_pm,
            r.return_pm,
            r.nbr_pm,
        ]
    };
    assert_eq!(rows(Training::Raw), [800_000, 355, 30, 8, 500, 1500]);
    assert_eq!(rows(Training::Trained), [880_000, 455, 40, 14, 650, 1000]);
    assert_eq!(rows(Training::Veteran), [950_000, 535, 55, 25, 800, 700]);
    let g = &t.globals;
    assert_eq!(
        [g.break_, g.fall, g.rally, g.fall_exit, g.waver, g.steady],
        [150_000, 350_000, 400_000, 450_000, 600_000, 680_000]
    );
    assert_eq!(
        (g.w_colonel_pm, g.nbr_radius, g.dwell),
        (10_000, 150_000, 60)
    );
    assert_eq!(g.provenance.confidence, Confidence::Unverified);
    assert!(g.provenance.placeholder);
}

#[test]
fn symptom_rows_follow_the_state_tags() {
    let t = MoraleTable::from_toml_str(MORALE).unwrap();
    for k in MoraleKind::ALL {
        assert_eq!(t.symptom(k).id, k);
        assert_eq!(MoraleKind::from_tag(k.tag()), Some(k));
    }
    let s = |k| {
        let r = t.symptom(k);
        [r.fire_pm, r.shake_pm, r.look_pm, r.skulk_ppm]
    };
    assert_eq!(s(MoraleKind::Steady), [1000, 1000, 10, 0]);
    assert_eq!(s(MoraleKind::FallingBack), [0, 1500, 400, 1500]);
    assert_eq!(s(MoraleKind::Broken), [0, 1500, 800, 6000]);
    assert_eq!(s(MoraleKind::Rallied), [800, 1250, 120, 0]);
    let ramp = t.symptom(MoraleKind::Wavering).ramp.unwrap();
    assert_eq!(
        (
            ramp.fire_end_pm,
            ramp.shake_end_pm,
            ramp.look_end_pm,
            ramp.skulk_end_ppm
        ),
        (550, 1500, 300, 400)
    );
    assert_eq!(ramp.skulk_from_ppm, 500_000);
    assert_eq!(s(MoraleKind::Wavering), [1000, 1000, 80, 0]);
    assert_eq!(MoraleKind::from_tag(5), None);
}

#[test]
fn every_row_has_provenance_and_a_morale_target() {
    let t = MoraleTable::from_toml_str(MORALE).unwrap();
    let all = t
        .training
        .iter()
        .map(|r| &r.provenance)
        .chain(t.symptoms.iter().map(|r| &r.provenance))
        .chain([&t.globals.provenance]);
    for p in all {
        assert!(p.placeholder);
        assert_eq!(p.calibration_spec, Some(6));
        assert!(matches!(p.calibration_target, Some(61..=68)));
    }
}

#[test]
fn tables_load_dir_carries_morale() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/rules");
    let tables = Tables::load_dir(dir).unwrap();
    assert_eq!(tables.morale, MoraleTable::from_toml_str(MORALE).unwrap());
}

#[test]
fn float_is_refused() {
    let e = mutate("base_ppm = 800000", "base_ppm = 800000.0").unwrap_err();
    assert!(matches!(e, DataError::Float(p) if p == "training[0].base_ppm"));
}

#[test]
fn unknown_keys_are_refused() {
    let e = mutate("cap_pm = 355", "cap_pm = 355\nextra = 1").unwrap_err();
    assert!(matches!(e, DataError::UnknownKey(p) if p == "training[0].extra"));
    let e = mutate("dwell = 60", "dwell = 60\nextra = 1").unwrap_err();
    assert!(matches!(e, DataError::UnknownKey(p) if p == "globals.extra"));
}

#[test]
fn provenance_violations_are_refused() {
    let e = mutate("calibration_target = \"CT-006-65\"", "").unwrap_err();
    assert!(matches!(e, DataError::Provenance(_)), "{e:?}");
    let e = mutate(
        "calibration_target = \"CT-006-61\"",
        "calibration_target = \"CT-61\"",
    )
    .unwrap_err();
    assert!(matches!(e, DataError::Provenance(_)), "{e:?}");
    let e = mutate("confidence = \"unverified\"", "confidence = \"verified\"").unwrap_err();
    assert!(matches!(e, DataError::Provenance(_)), "{e:?}");
}

#[test]
fn threshold_order_is_enforced() {
    // Each swap breaks one link of break < fall < rally < fall_exit < waver < steady.
    for (from, to) in [
        ("break = 150000", "break = 350000"),
        ("rally = 400000", "rally = 450000"),
        ("fall_exit = 450000", "fall_exit = 400000"),
        ("steady = 680000", "steady = 600000"),
    ] {
        let e = mutate(from, to).unwrap_err();
        assert!(
            matches!(e, DataError::Range(m) if m.contains("thresholds")),
            "{from}"
        );
    }
}

#[test]
fn values_outside_their_range_are_refused() {
    for (from, to) in [
        ("base_ppm = 800000", "base_ppm = 1000001"),
        ("cap_pm = 355", "cap_pm = 0"),
        ("w_colonel_pm = 10000", "w_colonel_pm = 10001"),
        ("fatigue_knee = 400000", "fatigue_knee = 1000000"),
        ("ammo_knee_pm = 250", "ammo_knee_pm = 0"),
        ("nbr_radius = 150000", "nbr_radius = 0"),
        ("skulk_ppm = 6000", "skulk_ppm = 1000001"),
    ] {
        let e = mutate(from, to).unwrap_err();
        assert!(matches!(e, DataError::Range(_)), "{from}: {e:?}");
    }
}

#[test]
fn structure_violations_are_refused() {
    let e = mutate("id = \"raw\"", "id = \"trained\"").unwrap_err();
    assert!(matches!(e, DataError::Range(m) if m.contains("repeated")));
    let e = mutate("id = \"raw\"", "id = \"green\"").unwrap_err();
    assert!(matches!(e, DataError::Range(m) if m.contains("training level")));
    let e = mutate("id = \"broken\"", "id = \"routed\"").unwrap_err();
    assert!(matches!(e, DataError::Range(m) if m.contains("known state")));
    let e = mutate("schema = \"morale/1\"", "schema = \"morale/2\"").unwrap_err();
    assert!(matches!(e, DataError::Parse(_)));
    // A ramp on a state other than Wavering, and a Wavering row without one.
    let e = mutate("skulk_ppm = 0\nconfidence = \"unverified\"", "skulk_ppm = 0\nfire_end_pm = 1\nshake_end_pm = 1\nlook_end_pm = 1\nskulk_end_ppm = 1\nskulk_from_ppm = 1\nconfidence = \"unverified\"").unwrap_err();
    assert!(matches!(e, DataError::Range(m) if m.contains("ramp")));
    let e = mutate("skulk_from_ppm = 500000\n", "").unwrap_err();
    assert!(matches!(e, DataError::Range(m) if m.contains("ramp")));
    // A missing state: drop the last symptom row (the file ends with it).
    let cut = MORALE.rfind("[[symptom]]").unwrap();
    let e = MoraleTable::from_toml_str(&MORALE[..cut]).unwrap_err();
    assert!(matches!(e, DataError::Range(m) if m.contains("every id")));
}
