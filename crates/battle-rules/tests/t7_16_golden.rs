//! T7-16 (SPEC-007 section 13): `command_chain` (27 regiments under a tree of officers, one
//! telegraph and one flag link, a discretionary move sent from the army at tick 0) stepped with
//! `step_with_command` to tick 6000 with seed 7 hashes to the literal recorded once on Linux in
//! `qc/golden-replays/expected-command.json`; a second process agrees. Set `HD_SIM_BLESS=1` to
//! rewrite the file.

use battle_rules::command::CommandTable;
use battle_rules::scenarios::command::command_chain;
use battle_rules::{BattleState, Tables};
use std::process::Command;

const RULES_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/rules");
const FILE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../qc/golden-replays/expected-command.json"
);
const CHILD_ENV: &str = "T7_16_CHILD";
const TICKS: u64 = 6000;
const SEED: u64 = 7;

fn table() -> CommandTable {
    let text = std::fs::read_to_string(format!("{RULES_DIR}/command.toml")).unwrap();
    CommandTable::from_toml_str(&text).unwrap()
}

fn run(seed: u64, ticks: u64) -> BattleState {
    let (t, c) = (Tables::load_dir(RULES_DIR).unwrap(), table());
    let mut b = command_chain(&t, &c, seed);
    for _ in 0..ticks {
        b.step_with_command(&c);
    }
    b
}

fn hash_of(seed: u64, ticks: u64) -> u64 {
    run(seed, ticks).hash()
}

fn recorded() -> u64 {
    let text = std::fs::read_to_string(FILE).expect("expected-command.json is present");
    let line = text
        .lines()
        .find(|l| l.contains("\"scenario\": \"command_chain\""))
        .expect("row present");
    let hex = line.split("\"final_hash\": \"").nth(1).unwrap();
    u64::from_str_radix(&hex[..16], 16).unwrap()
}

#[test]
fn the_scenario_is_a_27_regiment_tree_with_one_link_of_each_medium() {
    let b = run(SEED, 0);
    let cmd = b.cmd.as_ref().expect("command layer on");
    assert_eq!(b.regiments.iter().filter(|r| r.team == 0).count(), 27);
    assert_eq!(cmd.officers.len(), 1 + 1 + 3 + 9 + 27);
    assert_eq!(cmd.links.len(), 2);
    assert_ne!(cmd.links[0].medium, cmd.links[1].medium);
    assert_eq!(
        cmd.records.len(),
        1,
        "the army's first order is sent at tick 0"
    );
}

#[test]
fn the_hash_is_recorded() {
    let got = hash_of(SEED, TICKS);
    println!("command_chain tick {TICKS} seed {SEED}: {got:016x}");
    assert_eq!(got, recorded(), "command_chain hash changed: {got:016x}");
}

#[test]
fn two_runs_agree_and_the_seed_matters() {
    assert_eq!(hash_of(SEED, 600), hash_of(SEED, 600));
    assert_ne!(hash_of(SEED, 600), hash_of(SEED + 1, 600));
}

/// Rewrites `expected-command.json` from the current build when `HD_SIM_BLESS=1`.
#[test]
fn bless_when_requested() {
    if std::env::var("HD_SIM_BLESS").as_deref() != Ok("1") {
        return;
    }
    let row = format!(
        "    {{\"scenario\": \"command_chain\", \"parameters\": \"27 regiments, telegraph and flags\", \"ticks\": {TICKS}, \"seed\": {SEED}, \"final_hash\": \"{:016x}\"}}\n",
        hash_of(SEED, TICKS)
    );
    std::fs::write(
        FILE,
        format!("{{\n  \"format\": 1,\n  \"rows\": [\n{row}  ]\n}}\n"),
    )
    .unwrap();
}

/// In a child process prints the hash; otherwise does nothing.
#[test]
fn child_probe() {
    if std::env::var(CHILD_ENV).is_ok() {
        println!("PROBE {:016x}", hash_of(SEED, TICKS));
    }
}

#[test]
fn a_second_process_agrees() {
    let out = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "child_probe", "--nocapture", "--test-threads=1"])
        .env(CHILD_ENV, "1")
        .output()
        .unwrap();
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    let hex = text
        .lines()
        .find_map(|l| l.split_once("PROBE ").map(|p| p.1))
        .expect("child printed its hash");
    assert_eq!(u64::from_str_radix(hex.trim(), 16).unwrap(), recorded());
}
