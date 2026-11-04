//! T7-16 (SPEC-007 section 13): `hd-sim battle --scenario command_chain --ticks 6000 --seed 7`
//! prints the hash recorded in `qc/golden-replays/expected-command.json`, and a movement scenario
//! runs too; an unknown scenario or a missing option exits 2.

mod common;

use common::{cli, process};

const REPO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

fn recorded() -> String {
    let text =
        std::fs::read_to_string(format!("{REPO}/qc/golden-replays/expected-command.json")).unwrap();
    let line = text.lines().find(|l| l.contains("command_chain")).unwrap();
    line.split("\"final_hash\": \"").nth(1).unwrap()[..16].to_string()
}

#[test]
fn command_chain_prints_the_recorded_hash_in_process_and_in_a_child() {
    let data = format!("{REPO}/data");
    let args = [
        "battle",
        "--scenario",
        "command_chain",
        "--ticks",
        "6000",
        "--seed",
        "7",
        "--data-dir",
        &data,
    ];
    let want = format!("final_hash={}", recorded());
    let (code, out, _) = cli(&args);
    assert_eq!(code, 0);
    assert!(out.contains(&want), "{out}");
    let (code, out, _) = process(&args);
    assert_eq!(code, 0);
    assert!(out.contains(&want), "{out}");
}

#[test]
fn a_movement_scenario_runs() {
    let data = format!("{REPO}/data");
    let (code, out, _) = cli(&[
        "battle",
        "--scenario",
        "march_single",
        "--ticks",
        "100",
        "--seed",
        "1",
        "--data-dir",
        &data,
    ]);
    assert_eq!(code, 0);
    assert!(out.contains("final_hash="), "{out}");
}

#[test]
fn an_unknown_scenario_or_missing_option_exits_2() {
    let data = format!("{REPO}/data");
    let (code, _, err) = cli(&[
        "battle",
        "--scenario",
        "nope",
        "--ticks",
        "1",
        "--seed",
        "1",
        "--data-dir",
        &data,
    ]);
    assert_eq!(code, 2);
    assert!(err.contains("nope"));
    assert_eq!(cli(&["battle", "--ticks", "1", "--seed", "1"]).0, 2);
}
