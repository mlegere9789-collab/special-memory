//! T10-01: the AI tables load; floats, unknown keys and provenance violations are refused.

use battle_ai::{AiTables, Difficulty, Trait};
use battle_rules::DataError;

const AI: &str = include_str!("../../../data/rules/ai.toml");
const TRAITS: &str = include_str!("../../../data/rules/ai_traits.toml");

fn load() -> AiTables {
    AiTables::from_toml_str(AI, TRAITS).expect("committed tables load")
}

fn ai_with(from: &str, to: &str) -> Result<AiTables, DataError> {
    assert!(AI.contains(from), "pattern {from:?} must occur");
    AiTables::from_toml_str(&AI.replacen(from, to, 1), TRAITS)
}

#[test]
fn committed_tables_load_from_the_directory() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/rules");
    let t = AiTables::load_dir(dir).expect("load_dir");
    assert_eq!(t, load());
    assert!(AiTables::load_dir("/nonexistent/rules").is_err());
}

#[test]
fn difficulty_rows_match_the_spec() {
    let t = load();
    let row = |d| {
        let r = t.difficulty(d);
        (
            r.op_interval_ticks,
            r.tac_interval_ticks,
            r.op_think_ticks,
            r.tac_think_ticks,
            r.forecast_candidates,
            r.forecast_samples,
            r.forecast_budget_soldier_ticks,
            r.estimate_noise_pm,
            r.sync_margin_pm,
        )
    };
    assert_eq!(
        row(Difficulty::Easy),
        (1800, 200, 1200, 100, 2, 1, 2_000_000, 300, 0)
    );
    assert_eq!(
        row(Difficulty::Standard),
        (1200, 100, 600, 40, 3, 2, 6_000_000, 150, 500)
    );
    assert_eq!(
        row(Difficulty::Hard),
        (900, 60, 400, 20, 4, 3, 12_000_000, 50, 1000)
    );
}

#[test]
fn globals_match_the_spec() {
    let g = load().globals;
    let placeholders = [
        g.sector_mm,
        i64::from(g.ack_timeout_ticks),
        i64::from(g.max_resends),
        i64::from(g.min_dwell_ticks),
        i64::from(g.withdraw_confirm_ticks),
        i64::from(g.stall_window_ticks),
        g.stall_min_mm,
        g.waypoint_tol_mm,
        i64::from(g.form_safety_ticks),
        i64::from(g.reposition_margin),
        i64::from(g.max_repositions),
        g.lane_interval_mm,
        g.second_line_mm,
        g.stand_off_mm,
        i64::from(g.unseen_margin_pm),
        g.gun_support_mm,
        i64::from(g.gun_cease_pm),
        i64::from(g.fatigue_gate_pm),
    ];
    assert_eq!(
        placeholders,
        [
            400_000, 1200, 2, 1200, 300, 400, 2000, 6000, 200, 200, 2, 20_000, 150_000, 1500, 150,
            150_000, 300, 600
        ]
    );
    assert_eq!(
        (
            g.reg_check_ticks,
            g.canister_mm,
            g.work_ratio,
            g.bled_pm,
            g.advisor_min_interval_ticks
        ),
        (10, 365_760, 2000, 300, 600)
    );
}

#[test]
fn trait_deltas_match_the_spec() {
    let t = load();
    let d = |tr| {
        let r = t.trait_row(tr);
        [
            r.risk,
            r.reserve,
            r.assault,
            r.withdraw,
            r.entrench,
            r.discretion,
            r.reaction,
        ]
    };
    assert_eq!(d(Trait::Aggressive), [250, -80, -400, -150, -100, 0, 0]);
    assert_eq!(d(Trait::Cautious), [-250, 100, 500, 150, 150, 0, 20]);
    assert_eq!(d(Trait::Engineer), [0, 0, 0, 0, 400, 0, 0]);
    assert_eq!(d(Trait::DefensiveSkill), [-50, 0, 200, -100, 200, 0, 0]);
    assert_eq!(d(Trait::HighDiscretion), [0, 0, 0, 0, 0, 400, 0]);
    assert_eq!(d(Trait::LowDiscretion), [0, 0, 0, 0, 0, -300, 20]);
    assert_eq!(d(Trait::CavalryFlair), [150, 0, 0, 0, 0, 0, 0]);
    assert!(t.trait_row(Trait::CavalryFlair).cavalry_only);
    assert_eq!(d(Trait::Logistics), [0, 50, 0, 0, 0, 0, 0]);
    for z in [Trait::Political, Trait::Inspiring, Trait::Feuding] {
        assert_eq!(d(z), [0; 7]);
    }
}

#[test]
fn every_nonzero_trait_row_names_its_documented_act() {
    for r in &load().traits {
        let nonzero = [
            r.risk,
            r.reserve,
            r.assault,
            r.withdraw,
            r.entrench,
            r.discretion,
            r.reaction,
        ]
        .iter()
        .any(|v| *v != 0);
        if nonzero {
            assert!(r.act_from.is_some(), "{:?}", r.id);
            assert!(!r.provenance.sources.is_empty(), "{:?}", r.id);
            assert!(r.provenance.note.is_some(), "{:?}", r.id);
        }
        assert_eq!(r.provenance.calibration_spec, Some(10));
        assert_eq!(r.provenance.calibration_target, Some(2));
    }
}

#[test]
fn sourced_values_are_not_placeholders_and_verified_rows_have_second_sources() {
    let t = load();
    for g in &t.global_provenance {
        if !g.1.placeholder {
            assert!(!g.1.sources.is_empty(), "{}", g.0);
        }
    }
    let all = t
        .global_provenance
        .iter()
        .map(|g| &g.1)
        .chain(t.traits.iter().map(|r| &r.provenance));
    for p in all {
        if p.confidence == battle_rules::Confidence::Verified {
            assert!(p.second_source.is_some());
        }
    }
    assert_eq!(
        t.global_provenance
            .iter()
            .filter(|g| !g.1.placeholder)
            .count(),
        5
    );
}

#[test]
fn extracted_rows_are_loaded() {
    let t = load();
    assert_eq!(t.trait_facts("george_g_meade").count(), 2);
    assert_eq!(t.trait_facts("nobody").count(), 0);
    let bragg: Vec<_> = t.relation_facts("braxton_bragg").collect();
    assert_eq!(bragg.len(), 1);
    assert_eq!(bragg[0].partner, "leonidas_polk");
    assert_eq!(bragg[0].source_page, 305);
}

#[test]
fn float_is_refused() {
    let e = ai_with(
        "name = \"sector_mm\"\nvalue = 400000",
        "name = \"sector_mm\"\nvalue = 400000.0",
    )
    .unwrap_err();
    assert!(matches!(e, DataError::Float(_)), "{e}");
    let e = AiTables::from_toml_str(
        AI,
        &TRAITS.replacen("source_page = 304", "source_page = 304.0", 1),
    )
    .unwrap_err();
    assert!(matches!(e, DataError::Float(_)), "{e}");
}

#[test]
fn unknown_keys_are_refused_so_no_row_can_name_a_rules_value() {
    for key in [
        "sight_range_mm",
        "message_delay_ticks",
        "strength",
        "ammunition",
        "fatigue",
        "rules_table",
    ] {
        let e = ai_with("id = \"easy\"\n", &format!("id = \"easy\"\n{key} = 1\n")).unwrap_err();
        assert!(matches!(e, DataError::UnknownKey(_)), "{key}: {e}");
    }
    let e = AiTables::from_toml_str(AI, &format!("{TRAITS}\n[extra]\nx = 1\n")).unwrap_err();
    assert!(matches!(e, DataError::UnknownKey(_)), "{e}");
}

#[test]
fn provenance_violations_are_refused() {
    let e = ai_with("confidence = \"unverified\"\nplaceholder = true\ncalibration_target = \"CT-010-4\"\nnote = \"Base", "confidence = \"verified\"\nplaceholder = true\ncalibration_target = \"CT-010-4\"\nnote = \"Base").unwrap_err();
    assert!(matches!(e, DataError::Provenance(_)), "{e}");
    let e = ai_with(
        "calibration_target = \"CT-010-6\"",
        "calibration_target = \"CT-6\"",
    )
    .unwrap_err();
    assert!(matches!(e, DataError::Provenance(_)), "{e}");
    let e = ai_with(
        "placeholder = true\ncalibration_target = \"CT-010-6\"",
        "placeholder = true",
    )
    .unwrap_err();
    assert!(matches!(e, DataError::Provenance(_)), "{e}");
    let e = ai_with(
        "name = \"canister_mm\"\nvalue = 365760\nconfidence = \"partial\"",
        "name = \"canister_mm\"\nvalue = 365760\nconfidence = \"verified\"",
    )
    .unwrap_err();
    assert!(matches!(e, DataError::Provenance(_)), "{e}");
    let bad = TRAITS.replacen("confidence = \"partial\"", "confidence = \"verified\"", 1);
    let e = AiTables::from_toml_str(AI, &bad).unwrap_err();
    assert!(matches!(e, DataError::Provenance(_)), "{e}");
    let bad = TRAITS.replacen("sources = [", "unused = [", 1);
    assert!(AiTables::from_toml_str(AI, &bad).is_err());
}

#[test]
fn a_nonzero_trait_row_needs_its_act_and_source() {
    let e = ai_with("act_from = \"george_g_meade\"\n", "").unwrap_err();
    assert!(matches!(e, DataError::Provenance(_)), "{e}");
    let e = ai_with(
        "sources = [{ document = \"CWV\", page = 268, tag = \"[U]\" }]\n",
        "",
    )
    .unwrap_err();
    assert!(matches!(e, DataError::Provenance(_)), "{e}");
}

#[test]
fn structure_errors_are_refused() {
    assert!(matches!(
        ai_with("id = \"hard\"", "id = \"easy\"").unwrap_err(),
        DataError::Range(_)
    ));
    assert!(matches!(
        ai_with("id = \"cautious\"", "id = \"aggressive\"").unwrap_err(),
        DataError::Range(_)
    ));
    assert!(matches!(
        ai_with("name = \"sector_mm\"", "name = \"sector\"").unwrap_err(),
        DataError::Range(_)
    ));
    assert!(matches!(
        ai_with("op_interval_ticks = 1800", "op_interval_ticks = 0").unwrap_err(),
        DataError::Range(_)
    ));
    assert!(matches!(
        ai_with("risk = 250\n", "risk = 2001\n").unwrap_err(),
        DataError::Range(_)
    ));
    assert!(AiTables::from_toml_str("", TRAITS).is_err());
}
