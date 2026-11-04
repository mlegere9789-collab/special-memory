//! T7-01 (SPEC-007 sections 3 and 13): `command.toml`, the officers sidecar and the trait masks
//! load, and the loaders refuse a float, an unknown key, missing provenance, a cycle, a level
//! that is not parent level plus one, two armies on a side and a regiment owned twice.

use battle_rules::command::{
    load_officers, trait_mask, CommandTable, Latitude, Level, Medium, OfficerId, TraitMask,
};
use battle_rules::registry::{T_AGGRESSIVE, T_CAUTIOUS, T_HIGH_DISC, T_LOW_DISC};
use battle_rules::{DataError, RegimentId};

const TABLE: &str = include_str!("../../../data/rules/command.toml");
const OFFICERS: &str = include_str!("../../../qc/fixtures/command/officers_good.toml");
const EWELL: &str = include_str!("../../../data/history/commanders/richard_s_ewell.toml");
const LEE: &str = include_str!("../../../data/history/commanders/robert_e_lee.toml");
const REYNOLDS: &str = include_str!("../../../data/history/commanders/john_f_reynolds.toml");

fn table_with(from: &str, to: &str) -> Result<CommandTable, DataError> {
    assert!(TABLE.contains(from), "test pattern missing: {from}");
    CommandTable::from_toml_str(&TABLE.replacen(from, to, 1))
}

fn officers_with(from: &str, to: &str) -> Result<battle_rules::command::OfficerSet, DataError> {
    assert!(OFFICERS.contains(from), "test pattern missing: {from}");
    load_officers(&OFFICERS.replacen(from, to, 1))
}

fn range_msg(r: Result<battle_rules::command::OfficerSet, DataError>) -> String {
    match r {
        Err(DataError::Range(m)) => m,
        other => panic!("expected a Range error, got {other:?}"),
    }
}

#[test]
fn shipped_table_loads_with_the_spec_values() {
    let t = CommandTable::from_toml_str(TABLE).unwrap();
    let c = t.medium(Medium::Courier);
    assert_eq!(
        (c.write_ticks, c.base_ticks, c.per_word_ticks, c.speed_mm_s),
        (600, 400, 0, 3500)
    );
    assert_eq!((c.loss_base_ppm, c.garble_ppm), (3000, 30000));
    let f = t.medium(Medium::Flags);
    assert_eq!(
        (f.write_ticks, f.base_ticks, f.per_word_ticks),
        (200, 400, 60)
    );
    assert_eq!(f.garble_ppm, 40000);
    let g = t.medium(Medium::Telegraph);
    assert_eq!(
        (g.write_ticks, g.base_ticks, g.per_word_ticks),
        (400, 200, 40)
    );
    assert_eq!(g.garble_ppm, 5000);
    let staff: Vec<u32> = [
        Level::Army,
        Level::Corps,
        Level::Division,
        Level::Brigade,
        Level::Regiment,
    ]
    .iter()
    .map(|l| t.staff_ticks(*l))
    .collect();
    assert_eq!(staff, [1200, 800, 600, 400, 200]);
    let a = t.trait_row(TraitMask(T_AGGRESSIVE)).unwrap();
    assert_eq!(
        (a.process_pm, a.go_bias_pm, a.reach_mm, a.gait_shift),
        (800, 250, 200_000, 1)
    );
    let c = t.trait_row(TraitMask(T_CAUTIOUS)).unwrap();
    assert_eq!(
        (c.process_pm, c.go_bias_pm, c.reach_mm, c.gait_shift),
        (1300, -250, -200_000, 0)
    );
    assert_eq!(t.trait_row(TraitMask(T_HIGH_DISC)).unwrap().go_bias_pm, 100);
    assert_eq!(t.trait_row(TraitMask(T_LOW_DISC)).unwrap().go_bias_pm, -200);
    assert!(t.trait_row(TraitMask(3)).is_none());
    let g = &t.globals;
    assert_eq!(
        (g.path_factor_pm, g.jitter_ticks, g.max_legs),
        (1300, 200, 3)
    );
    assert_eq!(
        (g.flag_range_mm, g.smoke_limit, g.smoke_max_waits),
        (8_000_000, 200, 6)
    );
    assert_eq!((g.go_threshold_pm, g.judge_sigma_pm), (500, 120));
    assert_eq!(
        (g.sight_range_mm, g.drift_mm_per_tick, g.report_max),
        (1_800_000, 53, 16)
    );
}

#[test]
fn every_row_is_a_placeholder_with_a_007_target() {
    let t = CommandTable::from_toml_str(TABLE).unwrap();
    let mut rows = vec![&t.globals.provenance];
    rows.extend(t.media.iter().map(|r| &r.provenance));
    rows.extend(t.levels.iter().map(|r| &r.provenance));
    rows.extend(t.traits.iter().map(|r| &r.provenance));
    assert_eq!(rows.len(), 1 + 3 + 5 + 4);
    for p in rows {
        assert!(p.placeholder);
        assert_eq!(p.calibration_spec, Some(7));
        assert!(p.calibration_target.is_some());
    }
}

#[test]
fn table_refuses_float_unknown_key_and_missing_provenance() {
    let e = table_with("speed_mm_s = 3500", "speed_mm_s = 3500.0").unwrap_err();
    assert_eq!(e, DataError::Float("medium[0].speed_mm_s".to_string()));
    let e = table_with(
        "jitter_ticks = 200\nfind",
        "jitter_ticks = 200\nbogus = 1\nfind",
    )
    .unwrap_err();
    assert_eq!(e, DataError::UnknownKey("globals.bogus".to_string()));
    let e = table_with(
        "confidence = \"unverified\"\nplaceholder = true\ncalibration_target = \"CT-007-1\"",
        "placeholder = true",
    )
    .unwrap_err();
    assert!(matches!(e, DataError::Provenance(_)), "{e:?}");
    let e = table_with(
        "placeholder = true\ncalibration_target = \"CT-007-4\"",
        "placeholder = false",
    )
    .unwrap_err();
    assert!(matches!(e, DataError::Provenance(_)), "{e:?}");
    let e = table_with("schema = \"command/1\"", "schema = \"command/2\"").unwrap_err();
    assert!(matches!(e, DataError::Parse(_)), "{e:?}");
}

#[test]
fn table_refuses_structural_faults() {
    for (from, to) in [
        ("id = \"flags\"", "id = \"courier\""),
        ("id = \"flags\"", "id = \"pigeon\""),
        ("level = 2\n", "level = 1\n"),
        ("trait = \"cautious\"", "trait = \"aggressive\""),
        ("speed_mm_s = 3500", "speed_mm_s = 0"),
        ("max_legs = 3", "max_legs = 0"),
        ("capture_share_pm = 600", "capture_share_pm = 1001"),
        ("process_pm = 800", "process_pm = 0"),
    ] {
        let e = table_with(from, to).unwrap_err();
        assert!(matches!(e, DataError::Range(_)), "{from} -> {to}: {e:?}");
    }
}

#[test]
fn traits_of_the_historical_records() {
    assert_eq!(trait_mask(EWELL).unwrap(), TraitMask(T_LOW_DISC));
    assert_eq!(trait_mask(REYNOLDS).unwrap(), TraitMask(0));
    // Lee carries both high_discretion and aggressive; all traits count whatever their confidence.
    let lee = trait_mask(LEE).unwrap();
    assert!(lee.contains(TraitMask(T_HIGH_DISC)));
    assert_eq!(lee, TraitMask(T_HIGH_DISC | T_AGGRESSIVE));
}

fn record(traits: &[&str]) -> String {
    let mut s = String::from("id = \"x\"\nname = \"X\"\nside = \"union\"\n");
    for t in traits {
        s.push_str(&format!(
            "[[traits]]\ntrait = \"{t}\"\nevidence = \"e\"\nconfidence = \"unverified\"\n"
        ));
    }
    s
}

#[test]
fn opposing_traits_cancel_and_others_are_ignored() {
    assert_eq!(trait_mask(&record(&[])).unwrap(), TraitMask(0));
    assert_eq!(
        trait_mask(&record(&["aggressive", "cautious"])).unwrap(),
        TraitMask(0)
    );
    assert_eq!(
        trait_mask(&record(&["high_discretion", "low_discretion"])).unwrap(),
        TraitMask(0)
    );
    assert_eq!(
        trait_mask(&record(&["cautious", "low_discretion", "engineer"])).unwrap(),
        TraitMask(T_CAUTIOUS | T_LOW_DISC)
    );
    assert_eq!(
        trait_mask(&record(&["aggressive", "cautious", "low_discretion"])).unwrap(),
        TraitMask(T_LOW_DISC)
    );
    assert!(matches!(trait_mask("= nonsense"), Err(DataError::Parse(_))));
    assert!(matches!(
        trait_mask("[[traits]]\nevidence = \"e\"\n"),
        Err(DataError::Parse(_))
    ));
}

#[test]
fn officers_load_in_id_order() {
    let set = load_officers(OFFICERS).unwrap();
    assert_eq!(set.officers.len(), 11);
    let ids: Vec<u16> = set.officers.iter().map(|o| o.id.0).collect();
    assert_eq!(ids, [1, 2, 3, 4, 5, 6, 10, 11, 12, 13, 14]);
    let o2 = &set.officers[1];
    assert_eq!(
        (o2.side, o2.level, o2.parent),
        (0, Level::Corps, Some(OfficerId(1)))
    );
    assert_eq!(
        (o2.deputy, o2.commander.as_deref()),
        (Some(OfficerId(3)), Some("richard_s_ewell"))
    );
    assert_eq!((o2.hq, o2.post_range_pm), (None, 1000));
    let o11 = &set.officers[7];
    assert_eq!(
        (o11.hq, o11.post_range_pm),
        (Some((1_000_000, 2_000_000)), 1500)
    );
    assert_eq!(set.officers[4].regiment, Some(RegimentId(0)));
    assert_eq!(set.officers[0].parent, None);
    assert_eq!(set.links.len(), 2);
    // Links are symmetric: stored with the lower id first.
    assert_eq!(
        (set.links[0].medium, set.links[0].a, set.links[0].b),
        (Medium::Flags, OfficerId(5), OfficerId(6))
    );
    assert_eq!(set.links[1].medium, Medium::Telegraph);
}

#[test]
fn officers_refuse_float_unknown_key_and_missing_keys() {
    let e = officers_with("post_range_pm = 1500", "post_range_pm = 1500.5").unwrap_err();
    assert!(matches!(e, DataError::Float(_)), "{e:?}");
    let e = officers_with("hq_x_mm", "rank = 3\nhq_x_mm").unwrap_err();
    assert_eq!(e, DataError::UnknownKey("officer[7].rank".to_string()));
    let e = officers_with("schema = \"officers/1\"\n", "").unwrap_err();
    assert!(matches!(e, DataError::Parse(_)), "{e:?}");
    let e = officers_with("hq_y_mm = 2000000\n", "").unwrap_err();
    assert!(matches!(e, DataError::Range(_)), "{e:?}");
}

#[test]
fn cycle_is_refused() {
    // Officers 3 and 4 name each other as parent.
    let text = OFFICERS.replacen("level = 2\nparent = 2", "level = 2\nparent = 4", 1);
    assert!(range_msg(load_officers(&text)).contains("cycle"));
    let selfloop = OFFICERS.replacen("level = 1\nparent = 1", "level = 1\nparent = 2", 1);
    assert!(range_msg(load_officers(&selfloop)).contains("cycle"));
}

#[test]
fn wrong_level_is_refused() {
    let m = range_msg(officers_with(
        "level = 3\nparent = 3",
        "level = 4\nparent = 3\nregiment = 9",
    ));
    assert!(m.contains("parent level + 1"), "{m}");
    let m = range_msg(officers_with(
        "level = 2\nparent = 2",
        "level = 3\nparent = 2",
    ));
    assert!(m.contains("parent level + 1"), "{m}");
}

#[test]
fn two_armies_on_a_side_are_refused() {
    let m = range_msg(officers_with("id = 10\nside = 1", "id = 10\nside = 0"));
    assert!(m.contains("two armies"), "{m}");
    let m = range_msg(officers_with(
        "id = 2\nside = 0\nlevel = 1\nparent = 1",
        "id = 2\nside = 0\nlevel = 0",
    ));
    assert!(m.contains("two armies"), "{m}");
}

#[test]
fn regiment_owned_twice_is_refused() {
    let m = range_msg(officers_with("regiment = 1", "regiment = 0"));
    assert!(m.contains("owned twice"), "{m}");
}

#[test]
fn other_structure_faults_are_refused() {
    for (from, to) in [
        ("id = 3\nside = 0", "id = 1\nside = 0"),
        (
            "id = 12\nside = 1\nlevel = 2\nparent = 11",
            "id = 12\nside = 1\nlevel = 2\nparent = 99",
        ),
        (
            "id = 12\nside = 1\nlevel = 2\nparent = 11",
            "id = 12\nside = 1\nlevel = 2\nparent = 4",
        ),
        ("regiment = 2\n", ""),
        ("deputy = 3", "deputy = 2"),
        ("deputy = 3", "deputy = 14"),
        ("deputy = 3", "deputy = 77"),
        ("level = 1\nparent = 1", "level = 5\nparent = 1"),
        ("a = 6\nb = 5", "a = 6\nb = 6"),
        ("a = 6\nb = 5", "a = 6\nb = 99"),
        ("medium = \"telegraph\"", "medium = \"courier\""),
        ("post_range_pm = 1500", "post_range_pm = 0"),
    ] {
        let r = officers_with(from, to);
        assert!(r.is_err(), "{from} -> {to} was accepted");
    }
    // A repeated link, written the other way round, is refused too.
    let dup = format!("{OFFICERS}\n[[link]]\nmedium = \"flags\"\na = 5\nb = 6\n");
    assert!(range_msg(load_officers(&dup)).contains("link"));
    // A flag link between the two sides is allowed to exist only within one side.
    let cross = format!("{OFFICERS}\n[[link]]\nmedium = \"flags\"\na = 1\nb = 10\n");
    assert!(range_msg(load_officers(&cross)).contains("side"));
}

#[test]
fn latitude_and_medium_tags() {
    assert_eq!(Latitude::from_tag(0), Some(Latitude::Strict));
    assert_eq!(Latitude::from_tag(1), Some(Latitude::Discretionary));
    assert_eq!(Latitude::from_tag(2), None);
    assert_eq!(Medium::from_tag(0), None);
    assert_eq!(
        [1, 2, 3].map(|t| Medium::from_tag(t).unwrap().tag()),
        [1, 2, 3]
    );
    assert_eq!(Medium::from_tag(4), None);
    assert_eq!(
        [0, 1, 2, 3, 4].map(|t| Level::from_tag(t).unwrap().tag()),
        [0, 1, 2, 3, 4]
    );
    assert_eq!(Level::from_tag(5), None);
}
