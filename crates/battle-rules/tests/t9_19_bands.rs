//! T9-19: `run_band_suite("env", ...)` passes E1 to E8 (SPEC-009 section 10). Every row is
//! printed, so a missed band shows its measured mean, minimum and maximum next to the limits.
//! The band file holds both families; the fire bands are checked by `t5_19_bands.rs`.

use battle_rules::calibration::report::render_text;
use battle_rules::calibration::{parse_bands, run_band_suite};
use battle_rules::Tables;

const RULES_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/rules");
const BANDS: &str = include_str!("../../../qc/calibration/bands.toml");

fn tables() -> Tables {
    Tables::load_dir(RULES_DIR).unwrap()
}

#[test]
fn the_band_file_has_e1_to_e8_with_provenance_and_a_target() {
    let bands: Vec<_> = parse_bands(BANDS)
        .unwrap()
        .into_iter()
        .filter(|b| b.family == "env")
        .collect();
    let ids: Vec<&str> = bands.iter().map(|b| b.id.as_str()).collect();
    let want: Vec<String> = (1..=8).map(|n| format!("E{n}")).collect();
    assert_eq!(ids, want.iter().map(String::as_str).collect::<Vec<_>>());
    for b in &bands {
        assert!(!b.points.is_empty(), "{}", b.id);
        assert!(b.provenance.placeholder || !b.provenance.sources.is_empty());
        assert_eq!(b.provenance.calibration_spec, Some(9), "{}", b.id);
        assert!(b.provenance.calibration_target.is_some(), "{}", b.id);
    }
}

#[test]
fn the_limits_are_those_of_the_spec_table() {
    let want: [(&str, i64, i64); 8] = [
        ("E1", 0, 150),
        ("E2", 200, 900),
        ("E3", 30, 180),
        ("E4", 300, 900),
        ("E5", 1100, 1500),
        ("E6", 2500, 3500),
        ("E7", 800, 1000),
        ("E8", 700, 1000),
    ];
    let bands = parse_bands(BANDS).unwrap();
    for (id, low, high) in want {
        let b = bands.iter().find(|b| b.id == id).unwrap();
        let p = &b.points[0];
        assert_eq!((p.low, p.high), (Some(low), Some(high)), "{id}");
    }
}

/// The whole suite: smoke lines, the march, rain and the hill. Release builds only.
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "long suite: run with --release or --ignored"
)]
fn the_band_suite_passes_e1_to_e8() {
    let r = run_band_suite("env", &tables(), BANDS).unwrap();
    println!("{}", render_text(&r));
    assert_eq!(r.family, "env");
    assert_eq!(r.rows.len(), 8);
    assert!(r.rows.iter().all(|x| x.id.starts_with('E')));
    let missed: Vec<String> = r
        .rows
        .iter()
        .filter(|x| !x.pass)
        .map(|x| format!("{} {}", x.id, x.label))
        .collect();
    assert!(missed.is_empty(), "bands missed: {missed:?}");
}
