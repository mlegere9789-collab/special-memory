//! T5-20: determinism of the F0 scenarios. `cal_line(rifle_58, trained, 91440, false)` (tick
//! 2400, seed 1), `cal_canister(183000)` (tick 1200, seed 1) and `f0_large` (20,000 soldiers,
//! tick 600, seed 42) hash to the literals recorded once on Linux in
//! `qc/golden-replays/expected-f0.json`; a second process agrees. Set `HD_SIM_BLESS=1` to
//! rewrite the file.

use battle_rules::scenarios::fire::{cal_canister, cal_line, f0_large};
use battle_rules::{BattleState, Tables, Training};
use std::process::Command;

const RULES_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/rules");
const FILE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../qc/golden-replays/expected-f0.json"
);
const CHILD_ENV: &str = "T5_20_CHILD";

/// `(scenario, parameters, ticks, seed)`.
const CASES: [(&str, &str, u64, u64); 3] = [
    ("cal_line", "rifle_58 trained 91440 dry", 2400, 1),
    ("cal_canister", "napoleon_12 canister 183000", 1200, 1),
    ("f0_large", "rifle_58 25 v 25 regiments of 400", 600, 42),
];

fn build(name: &str, t: &Tables, seed: u64) -> BattleState {
    match name {
        "cal_line" => cal_line(t, seed, "rifle_58", Training::Trained, 91_440, false),
        "cal_canister" => cal_canister(t, seed, 183_000),
        _ => f0_large(t, seed),
    }
}

fn hash_of(name: &str, ticks: u64, seed: u64) -> u64 {
    let mut b = build(name, &Tables::load_dir(RULES_DIR).unwrap(), seed);
    for _ in 0..ticks {
        b.step();
    }
    b.hash()
}

fn is_large(name: &str) -> bool {
    name == "f0_large"
}

fn render(hashes: &[u64]) -> String {
    let mut s = String::from("{\n  \"format\": 1,\n  \"rows\": [\n");
    for (i, (c, h)) in CASES.iter().zip(hashes).enumerate() {
        s.push_str(&format!(
            "    {{\"scenario\": \"{}\", \"parameters\": \"{}\", \"ticks\": {}, \"seed\": {}, \"final_hash\": \"{h:016x}\"}}",
            c.0, c.1, c.2, c.3
        ));
        s.push_str(if i + 1 < CASES.len() { ",\n" } else { "\n" });
    }
    s.push_str("  ]\n}\n");
    s
}

fn recorded(name: &str) -> u64 {
    let text = std::fs::read_to_string(FILE).expect("expected-f0.json is present");
    let line = text
        .lines()
        .find(|l| l.contains(&format!("\"scenario\": \"{name}\"")))
        .expect("row present");
    let hex = line.split("\"final_hash\": \"").nth(1).unwrap();
    u64::from_str_radix(&hex[..16], 16).unwrap()
}

fn check(name: &str) {
    let &(_, _, ticks, seed) = CASES.iter().find(|c| c.0 == name).unwrap();
    let got = hash_of(name, ticks, seed);
    println!("{name} tick {ticks} seed {seed}: {got:016x}");
    assert_eq!(got, recorded(name), "{name} hash changed: {got:016x}");
    assert_eq!(got, hash_of(name, ticks, seed), "{name}: two runs differ");
}

#[test]
fn cal_line_hash_is_recorded() {
    check("cal_line");
}

#[test]
fn cal_canister_hash_is_recorded() {
    check("cal_canister");
}

/// 20,000 soldiers for 600 ticks, twice: release builds only.
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "large scenario: run with --release or --ignored"
)]
fn f0_large_hash_is_recorded() {
    check("f0_large");
}

/// Rewrites `expected-f0.json` from the current build when `HD_SIM_BLESS=1` (release builds).
#[test]
fn bless_when_requested() {
    if std::env::var("HD_SIM_BLESS").as_deref() != Ok("1") {
        return;
    }
    let hashes: Vec<u64> = CASES.iter().map(|c| hash_of(c.0, c.2, c.3)).collect();
    std::fs::write(FILE, render(&hashes)).unwrap();
}

/// In a child process prints the hashes; otherwise does nothing.
#[test]
fn child_probe() {
    if std::env::var(CHILD_ENV).is_err() {
        return;
    }
    for &(name, _, ticks, seed) in &CASES {
        if !is_large(name) || !cfg!(debug_assertions) {
            println!("PROBE {name} {:016x}", hash_of(name, ticks, seed));
        }
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
    let mut seen = 0;
    for line in text
        .lines()
        .filter_map(|l| l.split_once("PROBE ").map(|p| p.1))
    {
        let (name, hex) = line.split_once(' ').unwrap();
        assert_eq!(
            u64::from_str_radix(hex, 16).unwrap(),
            recorded(name),
            "{name} differs in the child"
        );
        seen += 1;
    }
    assert_eq!(seen, if cfg!(debug_assertions) { 2 } else { 3 });
}
