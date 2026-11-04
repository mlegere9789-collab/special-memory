//! T10-02: personality values from the committed commander records.

use battle_ai::{personality_for, AiTables, Personality};
use proptest::prelude::*;

const AI: &str = include_str!("../../../data/rules/ai.toml");
const TRAITS: &str = include_str!("../../../data/rules/ai_traits.toml");

fn load() -> AiTables {
    AiTables::from_toml_str(AI, TRAITS).expect("committed tables load")
}

/// risk, reserve, assault, withdraw, entrench, discretion, reaction.
fn v(p: &Personality) -> [u32; 7] {
    [
        u32::from(p.risk_tolerance),
        u32::from(p.reserve),
        u32::from(p.assault_ratio),
        u32::from(p.withdraw_ratio),
        u32::from(p.entrench),
        u32::from(p.discretion),
        p.reaction_ticks,
    ]
}

const BASE: [u32; 7] = [500, 200, 1500, 600, 300, 400, 0];

#[test]
fn base_personality_matches_the_spec() {
    assert_eq!(v(&load().base), BASE);
}

#[test]
fn meade_defensive_skill_partial_and_cautious_unverified() {
    let p = personality_for("george_g_meade", &load());
    assert_eq!(v(&p), [365, 240, 1840, 590, 500, 400, 8]);
}

#[test]
fn ewell_low_discretion_partial() {
    let p = personality_for("richard_s_ewell", &load());
    assert_eq!(v(&p), [500, 200, 1500, 600, 300, 190, 14]);
}

#[test]
fn lee_high_discretion_and_aggressive_partial() {
    let p = personality_for("robert_e_lee", &load());
    assert_eq!(p.discretion, 680);
    assert_eq!(p.risk_tolerance, 675);
}

#[test]
fn hancock_aggressive_partial() {
    let p = personality_for("winfield_scott_hancock", &load());
    assert_eq!(p.assault_ratio, 1220);
}

#[test]
fn no_traits_equals_base() {
    let t = load();
    assert_eq!(personality_for("nobody_at_all", &t), t.base);
    assert!(personality_for("nobody_at_all", &t).feuds.is_empty());
}

#[test]
fn cavalry_flair_applies_to_cavalry_seats_only() {
    let p = personality_for("jeb_stuart", &load());
    assert_eq!(p.risk_for(false), 500);
    assert_eq!(p.risk_for(true), 605);
    assert_eq!(p.risk_tolerance, 500);
}

#[test]
fn logistics_adds_to_reserve() {
    let p = personality_for("henry_j_hunt", &load());
    assert_eq!(p.reserve, 235);
}

#[test]
fn zero_row_traits_leave_the_base() {
    let t = load();
    assert_eq!(v(&personality_for("lewis_a_armistead", &t)), BASE);
}

#[test]
fn a_feud_bars_reserve_commit_unless_the_sector_is_nearly_lost() {
    let t = load();
    let bragg = personality_for("braxton_bragg", &t);
    assert_eq!(bragg.feuds, ["leonidas_polk"]);
    // withdraw_ratio / 2 is 300 for the base personality.
    assert!(!bragg.reserve_commit_allowed("leonidas_polk", 300));
    assert!(!bragg.reserve_commit_allowed("leonidas_polk", 1200));
    assert!(bragg.reserve_commit_allowed("leonidas_polk", 299));
    assert!(bragg.reserve_commit_allowed("some_other_general", 1200));
    let free = personality_for("nobody_at_all", &t);
    assert!(free.reserve_commit_allowed("leonidas_polk", 1200));
}

/// The committed tables with the first occurrence of `from` replaced by `to`.
fn edited(edits: &[(&str, &str)]) -> AiTables {
    let mut text = AI.to_owned();
    for (from, to) in edits {
        assert!(text.contains(from), "pattern {from:?} must occur");
        text = text.replacen(from, to, 1);
    }
    AiTables::from_toml_str(&text, &commander_x(&["aggressive", "cautious"]))
        .expect("edited tables load")
}

/// Trait rows giving commander `x` each named trait at `partial` confidence (weight 700).
fn commander_x(traits: &[&str]) -> String {
    traits
        .iter()
        .map(|t| {
            format!(
                "[[trait]]\ncommander = \"x\"\ntrait = \"{t}\"\nconfidence = \"partial\"\nsource_page = 1\nsources = [{{ document = \"CWV\", page = 1 }}]\nnote = \"An act.\"\n\n"
            )
        })
        .collect()
}

#[test]
fn each_term_truncates_toward_zero_before_the_sum() {
    // 700 * -1 / 1000 is -0.7 and truncates to 0 (flooring would give -1); two such terms stay 0.
    let t = edited(&[
        ("risk = 250\n", "risk = -1\n"),
        ("risk = -250\n", "risk = -1\n"),
    ]);
    assert_eq!(personality_for("x", &t).risk_tolerance, 500);
    // 700 * 1 / 1000 is 0.7 and truncates to 0 as well.
    let t = edited(&[
        ("risk = 250\n", "risk = 1\n"),
        ("risk = -250\n", "risk = 1\n"),
    ]);
    assert_eq!(personality_for("x", &t).risk_tolerance, 500);
    // Exact terms: aggressive reserve -80 * 700 / 1000 = -56 and cautious +100 * 700 / 1000 = +70.
    let p = personality_for("x", &edited(&[]));
    assert_eq!(p.reserve, 200 - 56 + 70);
}

#[test]
fn the_sum_is_clamped_to_the_bounds() {
    let t = edited(&[
        ("risk = 250\n", "risk = 2000\n"),
        ("risk = -250\n", "risk = 0\n"),
    ]);
    assert_eq!(personality_for("x", &t).risk_tolerance, 1000);
    let t = edited(&[
        ("risk = 250\n", "risk = -2000\n"),
        ("risk = -250\n", "risk = 0\n"),
    ]);
    assert_eq!(personality_for("x", &t).risk_tolerance, 0);
    let t = edited(&[
        ("reserve = -80\n", "reserve = -2000\n"),
        ("reserve = 100\n", "reserve = 0\n"),
    ]);
    assert_eq!(personality_for("x", &t).reserve, 0);
}

proptest! {
    #[test]
    fn every_value_stays_inside_its_bounds(pick in 0usize..60) {
        let t = load();
        let ids: Vec<&str> = t.trait_commanders().collect();
        let p = personality_for(ids[pick % ids.len()], &t);
        prop_assert!(p.risk_tolerance <= 1000 && p.reserve <= 1000 && p.entrench <= 1000 && p.discretion <= 1000);
        prop_assert!(p.assault_ratio <= 10_000 && p.withdraw_ratio <= 10_000 && p.reaction_ticks <= 1000);
        prop_assert!(p.risk_for(true) <= 1000);
    }
}
