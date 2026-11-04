//! T4-01, loader half: floats, unknown keys and provenance rules (SPEC-004 sections 1, 2).

use battle_rules::{parse_table, Confidence, DataError, Table};

/// Reads a one-row table the way a rules loader does: an integer plus flattened provenance.
fn load_row(text: &str) -> Result<(i64, battle_rules::Provenance), DataError> {
    let mut t = parse_table(text)?;
    let speed = t.take_int("speed_mm_s")?;
    let prov = t.take_provenance()?;
    t.finish()?;
    Ok((speed, prov))
}

const PLACEHOLDER_OK: &str = r#"
speed_mm_s = 1067
confidence = "unverified"
placeholder = true
calibration_target = "CT-1"
"#;

#[test]
fn placeholder_with_target_loads() {
    let (speed, p) = load_row(PLACEHOLDER_OK).unwrap();
    assert_eq!(speed, 1067);
    assert_eq!(p.confidence, Confidence::Unverified);
    assert!(p.placeholder);
    assert_eq!(p.calibration_target, Some(1));
    assert!(p.sources.is_empty());
}

#[test]
fn sourced_row_loads() {
    let text = r#"
speed_mm_s = 5
confidence = "partial"
sources = [{ document = "CWV", page = 511, tag = "[V]", note = "woods" }]
"#;
    let (_, p) = load_row(text).unwrap();
    assert!(!p.placeholder);
    assert_eq!(p.sources.len(), 1);
    assert_eq!(p.sources[0].document, "CWV");
    assert_eq!(p.sources[0].page, 511);
    assert_eq!(p.sources[0].tag.as_deref(), Some("[V]"));
}

#[test]
fn float_is_rejected() {
    let err = load_row(&PLACEHOLDER_OK.replace("1067", "1067.0")).unwrap_err();
    assert!(
        matches!(&err, DataError::Float(p) if p == "speed_mm_s"),
        "{err:?}"
    );
    // Exponent form, nan and inf are floats too, also when nested in arrays and tables.
    for bad in [
        "x = 1e3",
        "x = nan",
        "x = -inf",
        "[a]\nb = [1, 2.5]",
        "[[r]]\nx = 0.5",
    ] {
        assert!(
            matches!(parse_table(bad), Err(DataError::Float(_))),
            "{bad}"
        );
    }
}

#[test]
fn float_path_names_the_location() {
    let err = parse_table("[[row]]\nok = 1\n[[row]]\nbad = 2.0\n").unwrap_err();
    assert_eq!(err, DataError::Float("row[1].bad".to_string()));
}

#[test]
fn unknown_key_is_rejected() {
    let err = load_row(&format!("{PLACEHOLDER_OK}surprise = 3\n")).unwrap_err();
    assert_eq!(err, DataError::UnknownKey("surprise".to_string()));
}

#[test]
fn unknown_key_inside_a_source_is_rejected() {
    let text = r#"
speed_mm_s = 5
confidence = "partial"
sources = [{ document = "CWV", page = 1, color = "red" }]
"#;
    let err = load_row(text).unwrap_err();
    assert_eq!(err, DataError::UnknownKey("sources[0].color".to_string()));
}

#[test]
fn placeholder_without_target_is_rejected() {
    let text = "speed_mm_s = 1\nconfidence = \"unverified\"\nplaceholder = true\n";
    assert!(matches!(load_row(text), Err(DataError::Provenance(_))));
    let bad_target = text.to_string() + "calibration_target = \"first\"\n";
    assert!(matches!(
        load_row(&bad_target),
        Err(DataError::Provenance(_))
    ));
}

#[test]
fn non_placeholder_without_sources_is_rejected() {
    for text in [
        "speed_mm_s = 1\nconfidence = \"partial\"\n",
        "speed_mm_s = 1\nconfidence = \"partial\"\nplaceholder = false\n",
        "speed_mm_s = 1\nconfidence = \"partial\"\nsources = []\n",
    ] {
        assert!(
            matches!(load_row(text), Err(DataError::Provenance(_))),
            "{text}"
        );
    }
}

#[test]
fn verified_needs_a_second_source() {
    let base =
        "speed_mm_s = 1\nconfidence = \"verified\"\nsources = [{ document = \"CWV\", page = 3 }]\n";
    assert!(matches!(load_row(base), Err(DataError::Provenance(_))));
    let ok = format!("{base}second_source = \"Casey 1862 p.10\"\n");
    assert_eq!(load_row(&ok).unwrap().1.confidence, Confidence::Verified);
}

#[test]
fn bad_confidence_and_missing_confidence_are_provenance_errors() {
    let t = "speed_mm_s = 1\nplaceholder = true\ncalibration_target = \"CT-1\"\n";
    assert!(matches!(load_row(t), Err(DataError::Provenance(_))));
    let t = format!("{t}confidence = \"certain\"\n");
    assert!(matches!(load_row(&t), Err(DataError::Provenance(_))));
}

#[test]
fn malformed_toml_is_a_parse_error() {
    assert!(matches!(parse_table("x = "), Err(DataError::Parse(_))));
    assert!(matches!(
        parse_table("x = 1\nx = 2\n"),
        Err(DataError::Parse(_))
    ));
    // An integer beyond i64 is not representable.
    assert!(matches!(
        parse_table("x = 99999999999999999999"),
        Err(DataError::Parse(_))
    ));
}

#[test]
fn missing_and_mistyped_keys_are_parse_errors() {
    let mut t = parse_table("a = \"s\"\n").unwrap();
    assert!(matches!(t.take_int("a"), Err(DataError::Parse(_))));
    assert!(matches!(t.take_int("b"), Err(DataError::Parse(_))));
    assert_eq!(t.take_int_opt("b").unwrap(), None);
}

#[test]
fn range_errors_name_the_key() {
    let mut t = parse_table("n = 70\nm = 5\n").unwrap();
    assert!(matches!(t.take_int_in("n", 1, 64), Err(DataError::Range(m)) if m.contains('n')));
    assert_eq!(t.take_int_in("m", 1, 64).unwrap(), 5);
}

#[test]
fn rows_and_nested_tables_are_walked_in_file_order() {
    let mut t =
        parse_table("[globals]\nk = 1\n[[gait]]\nid = \"walk\"\n[[gait]]\nid = \"run\"\n").unwrap();
    let mut g: Table = t.take_table("globals").unwrap();
    assert_eq!(g.take_int("k").unwrap(), 1);
    g.finish().unwrap();
    let rows = t.take_rows("gait").unwrap();
    let ids: Vec<String> = rows
        .into_iter()
        .map(|mut r| r.take_str("id").unwrap())
        .collect();
    assert_eq!(ids, ["walk", "run"]);
    t.finish().unwrap();
}

#[test]
fn leftover_nested_key_is_reported_with_dotted_path() {
    let mut t = parse_table("[globals]\nk = 1\nz = 2\n").unwrap();
    let mut g = t.take_table("globals").unwrap();
    g.take_int("k").unwrap();
    assert_eq!(
        g.finish().unwrap_err(),
        DataError::UnknownKey("globals.z".to_string())
    );
}
