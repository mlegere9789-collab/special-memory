//! T11-06: the Gettysburg band table (SPEC-011 section 7): every row loads with provenance, `low`
//! and `high` recompute from `phrase` and `source_value`, ABT components sum to their totals, no
//! row is `verified`, and the ids are exactly HB-01 to HB-24.

use history_cal::{check_bands, widen, BandSet, Policy, ScriptError};

const BANDS: &str = include_str!("../../../qc/calibration/gettysburg-bands.toml");
const POLICY: &str = include_str!("../../../qc/calibration/policy.toml");

fn policy() -> Policy {
    Policy::from_toml_str(POLICY).unwrap()
}

fn set() -> BandSet {
    BandSet::from_toml_str(BANDS).unwrap()
}

#[test]
fn ids_are_the_section_7_list() {
    let want: Vec<String> = (1..=24).map(|n| format!("HB-{n:02}")).collect();
    let got: Vec<String> = set().bands().iter().map(|b| b.id.clone()).collect();
    assert_eq!(got, want);
}

#[test]
fn every_row_has_provenance_and_none_is_verified() {
    for b in set().bands() {
        let p = &b.provenance;
        assert!(
            !p.sources.is_empty() || (p.placeholder && p.calibration_target.is_some()),
            "{} has no provenance",
            b.id
        );
        assert!(p.second_source.is_none(), "{} claims a second source", b.id);
        assert_ne!(
            p.confidence,
            battle_rules::Confidence::Verified,
            "{} is verified",
            b.id
        );
        assert!(p
            .sources
            .iter()
            .all(|s| s.document == "CWV" && s.tag.is_some()));
    }
}

#[test]
fn low_and_high_recompute_from_phrase_and_source_value() {
    let p = policy();
    let set = set();
    assert_eq!(check_bands(&set, &p), Vec::<String>::new());
    for b in set.bands() {
        let (low, high) = widen(&b.phrase, b.source_value, &p).unwrap();
        assert_eq!((low, high), (b.low, b.high), "{}", b.id);
    }
}

#[test]
fn widening_rules_of_section_7() {
    let p = policy();
    assert_eq!(widen("exact", Some(254), &p).unwrap(), (215, Some(293)));
    assert_eq!(widen("about", Some(1500), &p).unwrap(), (1200, Some(1800)));
    assert_eq!(widen("nearly", Some(9000), &p).unwrap(), (7650, Some(9000)));
    assert_eq!(widen("over", Some(6000), &p).unwrap(), (6000, Some(6900)));
    assert_eq!(widen("about_open", Some(10000), &p).unwrap(), (8000, None));
    assert_eq!(
        widen("range 6555 8892", None, &p).unwrap(),
        (6555, Some(8892))
    );
    assert_eq!(widen("range_open 4000", None, &p).unwrap(), (4000, None));
    for bad in [
        "",
        "roughly",
        "range 5",
        "range 9 3",
        "exact 5",
        "range_open",
    ] {
        assert!(
            matches!(widen(bad, Some(5), &p), Err(ScriptError::Data(_))),
            "{bad:?}"
        );
    }
    assert!(widen("exact", None, &p).is_err());
}

/// ABT components from the consulted page 79 and page 84 figures, each with the bands that carry
/// the total or a partial sum.
#[test]
fn abt_components_sum_to_their_totals() {
    let rows: [(&[i64], i64, &str, Option<i64>); 5] = [
        (&[3155, 14529, 5365], 23049, "HB-07", Some(23049)),
        (&[3903, 18735, 5425], 28063, "HB-08", None),
        (&[3155, 14529], 17684, "HB-09", Some(17684)),
        (&[3903, 18735], 22638, "HB-10", Some(22638)),
        (&[1123, 4019, 3750], 8892, "HB-06", None),
    ];
    let set = set();
    for (parts, total, id, stated) in rows {
        assert_eq!(parts.iter().sum::<i64>(), total, "{id}");
        let b = set.get(id).unwrap();
        assert_eq!(b.source_value, stated, "{id} source_value");
        assert!(
            b.high == Some(total) || b.high.is_some_and(|h| h > total),
            "{id} high"
        );
    }
}

#[test]
fn gating_follows_confidence() {
    let set = set();
    let gated: Vec<&str> = set
        .bands()
        .iter()
        .filter(|b| b.gates())
        .map(|b| b.id.as_str())
        .collect();
    assert_eq!(
        gated,
        [
            "HB-01", "HB-02", "HB-03", "HB-04", "HB-05", "HB-11", "HB-12", "HB-13", "HB-14",
            "HB-17"
        ]
    );
}

#[test]
fn a_changed_edge_is_caught() {
    let text = BANDS.replacen("low = 7650", "low = 7000", 1);
    let set = BandSet::from_toml_str(&text).unwrap();
    let errors = check_bands(&set, &policy());
    assert_eq!(errors.len(), 1);
    assert!(errors[0].contains("HB-01"), "{errors:?}");
}
