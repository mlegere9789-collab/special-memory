//! T4-07, `transition_ticks` half: all six transitions by three training levels, integer
//! ceiling (SPEC-004 section 6). The state timing half belongs to the drill state machine.

use battle_rules::{transition_ticks, DrillTable, Formation, Training};
use Formation::{Column, Line, Skirmish};

const FILE: &str = include_str!("../../../data/rules/drill.toml");

fn table() -> DrillTable {
    DrillTable::from_toml_str(FILE).unwrap()
}

#[test]
fn all_six_pairs_by_three_levels() {
    // (from, to, raw, trained, veteran) from base 1200, 1000, 1500, 1500, 1800, 1200
    // with 1300, 1000 and 750 permille. Every product here is exact.
    let rows = [
        (Column, Line, 1560, 1200, 900),
        (Line, Column, 1300, 1000, 750),
        (Line, Skirmish, 1950, 1500, 1125),
        (Skirmish, Line, 1950, 1500, 1125),
        (Column, Skirmish, 2340, 1800, 1350),
        (Skirmish, Column, 1560, 1200, 900),
    ];
    let t = table();
    for (from, to, raw, trained, veteran) in rows {
        assert_eq!(
            transition_ticks(from, to, Training::Raw, &t),
            raw,
            "{from:?}->{to:?}"
        );
        assert_eq!(transition_ticks(from, to, Training::Trained, &t), trained);
        assert_eq!(transition_ticks(from, to, Training::Veteran, &t), veteran);
    }
}

#[test]
fn same_formation_takes_no_time() {
    let t = table();
    for f in Formation::ALL {
        for tr in Training::ALL {
            assert_eq!(transition_ticks(f, f, tr, &t), 0);
        }
    }
}

#[test]
fn ceiling_rounds_up_never_down() {
    // base 7 ticks at 750 permille is 5.25, at 1300 is 9.1, at 1000 is exactly 7.
    let text = FILE.replacen("base_ticks = 1200", "base_ticks = 7", 1);
    let t = DrillTable::from_toml_str(&text).unwrap();
    assert_eq!(transition_ticks(Column, Line, Training::Veteran, &t), 6);
    assert_eq!(transition_ticks(Column, Line, Training::Raw, &t), 10);
    assert_eq!(transition_ticks(Column, Line, Training::Trained, &t), 7);
    // base 1 at 1000 permille stays 1; at 750 it is 0.75 and rounds up to 1.
    let text = FILE.replacen("base_ticks = 1200", "base_ticks = 1", 1);
    let t = DrillTable::from_toml_str(&text).unwrap();
    assert_eq!(transition_ticks(Column, Line, Training::Veteran, &t), 1);
    assert_eq!(transition_ticks(Column, Line, Training::Trained, &t), 1);
}

#[test]
fn shipped_values_sit_inside_the_ct5_band() {
    let t = table();
    let raw = transition_ticks(Column, Line, Training::Raw, &t);
    let vet = transition_ticks(Column, Line, Training::Veteran, &t);
    // Raw column to line 60 to 120 s at 20 ticks per second.
    assert!((60 * 20..=120 * 20).contains(&raw), "{raw}");
    // Veteran/raw ratio 0.4 to 0.8.
    assert!(vet * 10 >= raw * 4 && vet * 10 <= raw * 8, "{vet} {raw}");
}

#[test]
fn trained_is_never_slower_than_raw_nor_faster_than_veteran() {
    let t = table();
    for from in Formation::ALL {
        for to in Formation::ALL {
            let r = transition_ticks(from, to, Training::Raw, &t);
            let m = transition_ticks(from, to, Training::Trained, &t);
            let v = transition_ticks(from, to, Training::Veteran, &t);
            assert!(r >= m && m >= v);
        }
    }
}
