//! T9-16 (SPEC-009 section 11): `env_smoke_line` (tick 2400, seed 1), `env_hill_hq` (tick 1200,
//! seed 2) and `env_heat_march` (tick 4000, seed 3) hash to the literals recorded once on Linux in
//! `qc/golden-replays/expected-env.json`; a second process agrees. Set `HD_SIM_BLESS=1` to
//! rewrite the file. With the environment switched off the same battles hash differently, since
//! the environment block is then absent.

use battle_rules::scenarios::env::{env_heat_march, env_hill_hq, env_smoke_line};
use battle_rules::{BattleState, Tables};
use std::process::Command;

const RULES_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/rules");
const FILE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../qc/golden-replays/expected-env.json"
);
const CHILD_ENV: &str = "T9_16_CHILD";

/// `(scenario, parameters, ticks, seed)`.
const CASES: [(&str, &str, u64, u64); 3] = [
    (
        "env_smoke_line",
        "rifle_58 trained 91440 light wind",
        2400,
        1,
    ),
    ("env_hill_hq", "hill observer, valley fight", 1200, 2),
    ("env_heat_march", "400 quick, 14:00 day 2", 4000, 3),
];

fn build(name: &str, t: &Tables, seed: u64) -> BattleState {
    match name {
        "env_smoke_line" => env_smoke_line(t, seed),
        "env_hill_hq" => env_hill_hq(t, seed),
        _ => env_heat_march(t, seed),
    }
}

fn run(name: &str, ticks: u64, seed: u64, env_on: bool) -> BattleState {
    let mut b = build(name, &Tables::load_dir(RULES_DIR).unwrap(), seed);
    if !env_on {
        b.env = None;
    }
    for _ in 0..ticks {
        b.step();
    }
    b
}

fn hash_of(name: &str, ticks: u64, seed: u64) -> u64 {
    run(name, ticks, seed, true).hash()
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
    let text = std::fs::read_to_string(FILE).expect("expected-env.json is present");
    let line = text
        .lines()
        .find(|l| l.contains(&format!("\"scenario\": \"{name}\"")))
        .expect("row present");
    let hex = line.split("\"final_hash\": \"").nth(1).unwrap();
    u64::from_str_radix(&hex[..16], 16).unwrap()
}

#[test]
fn the_three_hashes_are_recorded() {
    for &(name, _, ticks, seed) in &CASES {
        let got = hash_of(name, ticks, seed);
        println!("{name} tick {ticks} seed {seed}: {got:016x}");
        assert_eq!(got, recorded(name), "{name} hash changed: {got:016x}");
    }
}

/// Rewrites `expected-env.json` from the current build when `HD_SIM_BLESS=1`.
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
        println!("PROBE {name} {:016x}", hash_of(name, ticks, seed));
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
    for line in text.lines() {
        let Some((_, rest)) = line.split_once("PROBE ") else {
            continue;
        };
        let (name, hex) = rest.split_once(' ').unwrap();
        assert_eq!(u64::from_str_radix(hex.trim(), 16).unwrap(), recorded(name));
        seen += 1;
    }
    assert_eq!(seen, CASES.len());
}

#[test]
fn the_seed_matters_and_the_environment_block_is_part_of_the_hash() {
    for &(name, _, _, seed) in &CASES {
        assert_eq!(hash_of(name, 200, seed), hash_of(name, 200, seed), "{name}");
        assert_ne!(
            hash_of(name, 200, seed),
            hash_of(name, 200, seed + 1),
            "{name}"
        );
        assert_ne!(
            run(name, 200, seed, true).hash(),
            run(name, 200, seed, false).hash(),
            "{name}"
        );
    }
}

#[test]
fn the_scenarios_do_what_their_names_say() {
    let smoke = run("env_smoke_line", 2400, 1, true);
    assert!(smoke.smoke.total() > 0 && smoke.counters().shots_fired > 0);
    let hq = run("env_hill_hq", 1200, 2, true);
    assert!(!hq.sightings(0).is_empty(), "the hill sees the valley");
    let march = run("env_heat_march", 4000, 3, true);
    let env = march.env.as_ref().unwrap();
    assert!(env.heat().drain_pm > 1000, "14:00 on day 2 is hot");
    assert!(march.loco.iter().any(|l| l.fatigue > 0));
}
