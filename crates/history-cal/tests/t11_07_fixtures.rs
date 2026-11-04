//! T11-07: the conflict fixtures (SPEC-011 section 7, CF-01 to CF-10): every rule holds on the
//! committed tables, and in-memory mutations of the data make the check fail.

use battle_rules::parse_table;
use history_cal::{check_conflicts, BandSet, ConflictSet, FixtureContext, Policy, ReplayScript};

const BANDS: &str = include_str!("../../../qc/calibration/gettysburg-bands.toml");
const SCRIPT: &str = include_str!("../../../data/scenarios/gb_timeline/script.toml");
const CONFLICTS: &str = include_str!("../../../qc/fixtures/history-conflicts.toml");
const KNOWN: &str = include_str!("../../../qc/fixtures/audit-known-errors.toml");
const CAVALRY: &str = include_str!("../../../data/history/battles/gb_east_cavalry_field.toml");
const POLICY: &str = include_str!("../../../qc/calibration/policy.toml");

/// Replaces the first `from` inside the row block that starts at `id = "<id>"`.
fn edit_row(text: &str, id: &str, from: &str, to: &str) -> String {
    let start = text.find(&format!("id = \"{id}\"")).expect("row");
    let end = text[start..].find("\n[[").map_or(text.len(), |n| start + n);
    let block = &text[start..end];
    assert!(block.contains(from), "{id} lacks {from:?}");
    format!(
        "{}{}{}",
        &text[..start],
        block.replacen(from, to, 1),
        &text[end..]
    )
}

fn known_numbers() -> Vec<i64> {
    KNOWN
        .lines()
        .filter_map(|l| l.strip_prefix("bad_value = ")?.trim().parse().ok())
        .collect()
}

fn cavalry_range() -> (String, i64, i64) {
    let mut root = parse_table(CAVALRY).unwrap();
    let mut t = root
        .take_table("sides")
        .unwrap()
        .take_table("confederate")
        .unwrap()
        .take_table("engaged")
        .unwrap();
    (
        "gb_east_cavalry_field.confederate_engaged".to_string(),
        t.take_int("low").unwrap(),
        t.take_int("high").unwrap(),
    )
}

fn run(bands: &str, script: &str, conflicts: &str) -> Vec<String> {
    let _ = Policy::from_toml_str(POLICY).unwrap();
    let bands = BandSet::from_toml_str(bands).unwrap();
    let script = ReplayScript::from_toml_str(script).unwrap();
    let set = ConflictSet::from_toml_str(conflicts).unwrap();
    let known = known_numbers();
    let ranges = [cavalry_range()];
    let ctx = FixtureContext {
        bands: &bands,
        script: &script,
        audit_values: &known,
        ranges: &ranges,
    };
    check_conflicts(&set, &ctx)
}

#[test]
fn ids_are_cf_01_to_cf_10() {
    let set = ConflictSet::from_toml_str(CONFLICTS).unwrap();
    let want: Vec<String> = (1..=10).map(|n| format!("CF-{n:02}")).collect();
    let got: Vec<String> = set.conflicts().iter().map(|c| c.id.clone()).collect();
    assert_eq!(got, want);
}

#[test]
fn every_rule_holds_on_the_committed_data() {
    assert_eq!(run(BANDS, SCRIPT, CONFLICTS), Vec::<String>::new());
}

#[test]
fn narrowing_hb_08_to_a_point_fails() {
    let bands = edit_row(BANDS, "HB-08", "low = 23000", "low = 28063");
    let errors = run(&bands, SCRIPT, CONFLICTS);
    assert!(errors.iter().any(|e| e.starts_with("CF-01")), "{errors:?}");
}

#[test]
fn narrowing_tl_06_to_a_point_fails() {
    let script = edit_row(SCRIPT, "tl_06", "early_s = 144000\n", "");
    let errors = run(BANDS, &script, CONFLICTS);
    assert!(errors.iter().any(|e| e.starts_with("CF-04")), "{errors:?}");
}

#[test]
fn a_late_time_dropped_from_tl_10_fails() {
    let script = edit_row(SCRIPT, "tl_10", "late_s = 226800\n", "");
    let errors = run(BANDS, &script, CONFLICTS);
    assert!(errors.iter().any(|e| e.starts_with("CF-03")), "{errors:?}");
}

#[test]
fn a_subtracted_band_fails() {
    let bands = edit_row(BANDS, "HB-01", "low = 7650", "low = 5000");
    let errors = run(&bands, SCRIPT, CONFLICTS);
    assert!(errors.iter().any(|e| e.starts_with("CF-06")), "{errors:?}");
}

#[test]
fn a_headline_in_a_partial_row_fails() {
    let bands = edit_row(
        BANDS,
        "HB-07",
        "confidence = \"unverified\"",
        "confidence = \"partial\"",
    );
    let errors = run(&bands, SCRIPT, CONFLICTS);
    assert!(errors.iter().any(|e| e.starts_with("CF-10")), "{errors:?}");
}

#[test]
fn a_collapsed_cavalry_range_fails() {
    let conflicts = edit_row(CONFLICTS, "CF-08", "value_b = 5000", "value_b = 4000");
    let errors = run(BANDS, SCRIPT, &conflicts);
    assert!(errors.iter().any(|e| e.starts_with("CF-08")), "{errors:?}");
}

#[test]
fn a_point_for_reynolds_fails() {
    let conflicts = edit_row(CONFLICTS, "CF-09", "value_b = 41400", "value_b = 37200");
    let errors = run(BANDS, SCRIPT, &conflicts);
    assert!(errors.iter().any(|e| e.starts_with("CF-09")), "{errors:?}");
}

#[test]
fn an_unknown_reference_or_rule_is_refused() {
    let conflicts = edit_row(CONFLICTS, "CF-02", "ref = \"HB-06\"", "ref = \"HB-99\"");
    let errors = run(BANDS, SCRIPT, &conflicts);
    assert!(errors.iter().any(|e| e.starts_with("CF-02")), "{errors:?}");
    let conflicts = edit_row(
        CONFLICTS,
        "CF-02",
        "rule = \"envelope\"",
        "rule = \"vibes\"",
    );
    assert!(ConflictSet::from_toml_str(&conflicts).is_err());
}
