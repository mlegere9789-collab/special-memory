//! T7-15 (SPEC-007 section 10): `run_command_bands` over seeds 1 to 64 puts every CT-007-1,
//! CT-007-2, CT-007-3, CT-007-6 and CT-007-7 quantity inside its band. Every row is printed, so a
//! missed band shows its mean, minimum and maximum next to the limits. A band is never edited to
//! turn this test green.

use battle_rules::calibration::report::render_text;
use battle_rules::calibration::{parse_bands, run_command_bands};
use battle_rules::command::CommandTable;
use battle_rules::Tables;

const RULES_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/rules");
const BANDS: &str = include_str!("../../../qc/calibration/command-bands.toml");

fn seeds() -> Vec<u64> {
    (1..=64).collect()
}

fn tables() -> (Tables, CommandTable) {
    let text = std::fs::read_to_string(format!("{RULES_DIR}/command.toml")).unwrap();
    (
        Tables::load_dir(RULES_DIR).unwrap(),
        CommandTable::from_toml_str(&text).unwrap(),
    )
}

#[test]
fn the_band_file_covers_the_five_targets_with_provenance() {
    let bands = parse_bands(BANDS).unwrap();
    assert!(bands.iter().all(|b| b.family == "command"));
    assert!(bands
        .iter()
        .all(|b| b.provenance.placeholder || !b.provenance.sources.is_empty()));
    for ct in ["CT-007-1", "CT-007-2", "CT-007-3", "CT-007-6", "CT-007-7"] {
        let text = format!("calibration_target = \"{ct}\"");
        assert!(BANDS.contains(&text), "no band for {ct}");
    }
    assert!(
        !BANDS.contains("confidence = \"verified\""),
        "no band is verified"
    );
}

#[test]
fn a_band_file_with_a_float_is_refused() {
    let float = BANDS.replacen("high = 40000", "high = 40.5", 1);
    assert!(run_command_bands(&tables().0, &tables().1, &float, &[1]).is_err());
}

#[test]
fn an_impossible_band_is_reported_as_a_miss() {
    let tiny = r#"
schema = "bands/1"
[[band]]
id = "X1"
family = "command"
metric = "delivery"
weapon = "courier"
scale = 1000
points = [{ at_mm = 3000000, low = 1, high = 2 }]
confidence = "unverified"
placeholder = true
calibration_target = "CT-007-1"
"#;
    let (t, c) = tables();
    let report = run_command_bands(&t, &c, tiny, &[1, 2, 3]).unwrap();
    assert_eq!(report.rows.len(), 1);
    assert!(!report.rows[0].pass && !report.passed());
}

#[test]
fn an_unknown_metric_is_refused() {
    let bad = BANDS.replacen("metric = \"delivery\"", "metric = \"nonsense\"", 1);
    let (t, c) = tables();
    assert!(run_command_bands(&t, &c, &bad, &[1]).is_err());
}

#[test]
fn every_command_band_holds_over_64_seeds() {
    let (t, c) = tables();
    let report = run_command_bands(&t, &c, BANDS, &seeds()).unwrap();
    let text = render_text(&report);
    println!("{text}");
    assert!(!report.rows.is_empty());
    assert!(report.passed(), "bands missed:\n{text}");
}
