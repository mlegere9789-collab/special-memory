//! T8-11 (SPEC-008 section 11): with `fc = 0` and caps that do not bind, the scheduler is off
//! the SPEC-005 path. `cal_line` and `cal_canister` under a `Battle` wrapper hash, tick for
//! tick, as the recorded T5-20 literals, and `unit_level_rolls` stays 0.

mod t084_support;

use battle_rules::scenarios::fire::{cal_canister, cal_line};
use battle_rules::{battle_hash, Battle, Training, F0};
use t084_support::{load, scenario_text, tables, U};

const FILE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../qc/golden-replays/expected-f0.json"
);

fn recorded(name: &str) -> u64 {
    let text = std::fs::read_to_string(FILE).expect("expected-f0.json is present");
    let line = text
        .lines()
        .find(|l| l.contains(&format!("\"scenario\": \"{name}\"")))
        .expect("row present");
    let hex = line.split("\"final_hash\": \"").nth(1).unwrap();
    u64::from_str_radix(&hex[..16], 16).unwrap()
}

/// A scenario naming the two regiments of the set-up, in regiment order.
fn units(second_men: u32, battery: bool) -> Vec<U> {
    let first = if battery {
        U {
            arm: "battery",
            men: 4,
            guns: 1,
            weapon: "napoleon_12",
            ..U::blue("first", 100, 0)
        }
    } else {
        U {
            men: 200,
            ..U::blue("first", 100, 0)
        }
    };
    vec![
        first,
        U {
            men: second_men,
            ..U::grey("second", 140, 0)
        },
    ]
}

fn wrapped(state: battle_rules::BattleState, us: &[U]) -> Battle {
    let scenario = load(&scenario_text(0, 600, us, ""));
    Battle::wrap(state, scenario, tables_fid()).unwrap()
}

fn tables_fid() -> battle_rules::FidelityTable {
    battle_rules::FidelityTable::from_toml_str(include_str!("../../../data/rules/fidelity.toml"))
        .unwrap()
}

#[test]
fn cal_line_hash_is_the_recorded_one_under_the_scheduler() {
    let state = cal_line(&tables(), 1, "rifle_58", Training::Trained, 91_440, false);
    let mut b = wrapped(state, &units(200, false));
    for _ in 0..2400 {
        b.step();
    }
    assert!(b.state.regiments.iter().all(|r| r.class == F0));
    assert_eq!(b.state.counters().unit_level_rolls, 0);
    assert_eq!(battle_hash(&b.state), recorded("cal_line"));
}

#[test]
fn cal_canister_hash_is_the_recorded_one_under_the_scheduler() {
    let state = cal_canister(&tables(), 1, 183_000);
    let mut b = wrapped(state, &units(100, true));
    for _ in 0..1200 {
        b.step();
    }
    assert_eq!(b.state.counters().unit_level_rolls, 0);
    assert_eq!(battle_hash(&b.state), recorded("cal_canister"));
}
