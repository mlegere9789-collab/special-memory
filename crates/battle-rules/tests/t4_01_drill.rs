//! T4-01 for `data/rules/drill.toml`: the shipped file loads, and the loader refuses a float, an
//! unknown key, a placeholder without target, a non-placeholder without sources and structural
//! faults (SPEC-004 sections 1, 2, 6).

use battle_rules::{DataError, DrillTable, Formation, Training};

const FILE: &str = include_str!("../../../data/rules/drill.toml");

fn mutate(from: &str, to: &str) -> Result<DrillTable, DataError> {
    assert!(FILE.contains(from), "test pattern missing: {from}");
    DrillTable::from_toml_str(&FILE.replacen(from, to, 1))
}

#[test]
fn shipped_file_loads_with_the_spec_values() {
    let t = DrillTable::from_toml_str(FILE).unwrap();
    assert_eq!(t.globals.closeup_interval_ticks, 40);
    assert_eq!(t.globals.settle_tol_mm, 1000);
    assert_eq!(t.globals.settle_budget_ticks, 600);
    assert_eq!(t.transition_pm(Training::Raw), 1300);
    assert_eq!(t.transition_pm(Training::Trained), 1000);
    assert_eq!(t.transition_pm(Training::Veteran), 750);
    assert_eq!(t.transitions.len(), 6);
    let line = t.formation(Formation::Line);
    assert_eq!(
        (line.ranks, line.file_width_mm, line.rank_gap_mm),
        (Some(2), 560, 900)
    );
    let column = t.formation(Formation::Column);
    assert_eq!((column.cols, column.file_width_mm), (Some(8), 560));
    let skirmish = t.formation(Formation::Skirmish);
    assert_eq!(skirmish.deployed_pm, 100);
    assert_eq!(skirmish.interval_mm, Some(3500));
    assert_eq!(skirmish.reserve_offset_mm, Some(100_000));
    assert_eq!(skirmish.cols, Some(8));
}

#[test]
fn every_row_is_a_placeholder_with_its_calibration_target() {
    let t = DrillTable::from_toml_str(FILE).unwrap();
    let mut rows = vec![&t.globals.provenance];
    rows.extend(t.training.iter().map(|r| &r.provenance));
    rows.extend(t.transitions.iter().map(|r| &r.provenance));
    rows.extend(t.formations.iter().map(|r| &r.provenance));
    assert_eq!(rows.len(), 1 + 3 + 6 + 3);
    for p in rows {
        assert!(p.placeholder);
        assert_eq!(p.calibration_target, Some(5));
    }
}

#[test]
fn float_is_rejected() {
    let err = mutate("transition_pm = 1300", "transition_pm = 1300.0").unwrap_err();
    assert_eq!(
        err,
        DataError::Float("training[0].transition_pm".to_string())
    );
}

#[test]
fn unknown_key_is_rejected() {
    let err = mutate("settle_tol_mm = 1000", "settle_tol_mm = 1000\nsurprise = 1").unwrap_err();
    assert_eq!(err, DataError::UnknownKey("globals.surprise".to_string()));
    let err = mutate("id = \"raw\"", "id = \"raw\"\nspeed_pm = 3").unwrap_err();
    assert_eq!(
        err,
        DataError::UnknownKey("training[0].speed_pm".to_string())
    );
}

#[test]
fn placeholder_without_target_is_rejected() {
    let err = mutate(
        "transition_pm = 1300\nconfidence = \"unverified\"\nplaceholder = true\ncalibration_target = \"CT-004-5\"",
        "transition_pm = 1300\nconfidence = \"unverified\"\nplaceholder = true",
    )
    .unwrap_err();
    assert!(matches!(err, DataError::Provenance(_)), "{err:?}");
}

#[test]
fn non_placeholder_without_sources_is_rejected() {
    let err = mutate(
        "transition_pm = 1300\nconfidence = \"unverified\"\nplaceholder = true\ncalibration_target = \"CT-004-5\"",
        "transition_pm = 1300\nconfidence = \"unverified\"",
    )
    .unwrap_err();
    assert!(matches!(err, DataError::Provenance(_)), "{err:?}");
}

#[test]
fn structural_faults_are_range_errors() {
    // from == to
    let err = mutate(
        "to = \"line\"\nbase_ticks = 1200",
        "to = \"column\"\nbase_ticks = 1200",
    );
    assert!(matches!(err, Err(DataError::Range(_))), "{err:?}");
    // unknown id
    let err = mutate("id = \"raw\"", "id = \"green\"");
    assert!(matches!(err, Err(DataError::Range(_))), "{err:?}");
    // repeated training level
    let err = mutate("id = \"trained\"", "id = \"raw\"");
    assert!(matches!(err, Err(DataError::Range(_))), "{err:?}");
    // repeated pair (line to column duplicated instead of line to skirmish)
    let err = mutate(
        "from = \"line\"\nto = \"skirmish\"",
        "from = \"line\"\nto = \"column\"",
    );
    assert!(matches!(err, Err(DataError::Range(_))), "{err:?}");
    // out-of-range value
    let err = mutate("transition_pm = 1300", "transition_pm = 0");
    assert!(matches!(err, Err(DataError::Range(_))), "{err:?}");
    let err = mutate("deployed_pm = 100", "deployed_pm = 1001");
    assert!(matches!(err, Err(DataError::Range(_))), "{err:?}");
    // a line without ranks, a column with ranks
    let err = mutate("ranks = 2\n", "");
    assert!(matches!(err, Err(DataError::Range(_))), "{err:?}");
    let err = mutate(
        "cols = 8\nfile_width_mm = 560\nrank_gap_mm = 900\nconfidence",
        "cols = 8\nranks = 2\nfile_width_mm = 560\nrank_gap_mm = 900\nconfidence",
    );
    assert!(matches!(err, Err(DataError::Range(_))), "{err:?}");
}

#[test]
fn missing_sections_are_refused() {
    let no_transitions = FILE.split("[[transition]]").next().unwrap();
    assert!(DrillTable::from_toml_str(no_transitions).is_err());
    let one_training = FILE.replacen(
        "[[training]]\nid = \"veteran\"",
        "[[ignored]]\nid = \"veteran\"",
        1,
    );
    assert!(DrillTable::from_toml_str(&one_training).is_err());
    assert!(matches!(
        DrillTable::from_toml_str("x ="),
        Err(DataError::Parse(_))
    ));
}
