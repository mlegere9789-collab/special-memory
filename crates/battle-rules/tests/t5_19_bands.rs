//! T5-19: `run_band_suite` passes B1 to B11 (SPEC-005 section 9), and the reference curves of
//! `qc/calibration/f0-curves.json` are the curves of that run. Set `HD_SIM_BLESS=1` to rewrite
//! the curves file. Every row is printed, so a missed band shows its measured mean, minimum and
//! maximum next to the limits.

use battle_rules::calibration::report::{render_curves_json, render_text};
use battle_rules::calibration::{parse_bands, run_band_suite};
use battle_rules::Tables;

const RULES_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/rules");
const BANDS: &str = include_str!("../../../qc/calibration/bands.toml");
const CURVES: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../qc/calibration/f0-curves.json"
);

fn tables() -> Tables {
    Tables::load_dir(RULES_DIR).unwrap()
}

#[test]
fn the_band_file_has_b1_to_b11_with_provenance() {
    let bands = parse_bands(BANDS).unwrap();
    let ids: Vec<&str> = bands.iter().map(|b| b.id.as_str()).collect();
    let want: Vec<String> = (1..=11).map(|n| format!("B{n}")).collect();
    assert_eq!(ids, want.iter().map(String::as_str).collect::<Vec<_>>());
    assert!(bands
        .iter()
        .all(|b| b.family == "fire" && !b.points.is_empty()));
    assert!(bands
        .iter()
        .all(|b| b.provenance.placeholder || !b.provenance.sources.is_empty()));
}

#[test]
fn a_band_file_with_a_float_or_missing_provenance_is_refused() {
    let float = BANDS.replacen("low = 3000 }", "low = 3.0 }", 1);
    assert!(parse_bands(&float).is_err());
    let bare = BANDS.replacen("calibration_target = \"CT-005-6\"\n", "", 1);
    assert!(parse_bands(&bare).is_err());
}

const TINY: &str = r#"
schema = "bands/1"
[[band]]
id = "X1"
family = "fire"
metric = "range_to_ground"
scale = 1
points = [
  { ammo = "shell_12", elevation_cdeg = 375, low = 1000, high = 1500 },
  { ammo = "shell_12", elevation_cdeg = 375, low = 1, high = 2 },
]
confidence = "unverified"
placeholder = true
calibration_target = "CT-005-2"
"#;

#[test]
fn the_suite_reports_a_miss_as_a_miss_and_does_not_hide_it() {
    let r = run_band_suite("fire", &tables(), TINY).unwrap();
    assert_eq!(r.rows.len(), 2);
    assert!(r.rows[0].pass && !r.rows[1].pass);
    assert!(!r.passed());
    assert!(render_text(&r).contains("MISS"));
    assert!(run_band_suite("env", &tables(), TINY)
        .unwrap()
        .rows
        .is_empty());
}

/// The whole suite: 88 `cal_line` runs, the rate, range and canister runs. Release builds only.
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "long suite: run with --release or --ignored"
)]
fn the_band_suite_passes_b1_to_b11() {
    let r = run_band_suite("fire", &tables(), BANDS).unwrap();
    println!("{}", render_text(&r));
    let json = render_curves_json(&r);
    if std::env::var("HD_SIM_BLESS").as_deref() == Ok("1") {
        std::fs::write(CURVES, &json).unwrap();
    }
    assert_eq!(
        std::fs::read_to_string(CURVES).expect("f0-curves.json is present"),
        json,
        "f0-curves.json differs from this run"
    );
    let missed: Vec<String> = r
        .rows
        .iter()
        .filter(|x| !x.pass)
        .map(|x| format!("{} {}", x.id, x.label))
        .collect();
    assert!(missed.is_empty(), "bands missed: {missed:?}");
}
