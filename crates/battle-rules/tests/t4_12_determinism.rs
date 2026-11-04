//! T4-12: determinism. The three scenario hashes equal literals recorded once on Linux, and a
//! second process that builds and runs the same scenarios prints the same values (SPEC-004
//! section 13 and Amendment A2).

mod t019_support;

use battle_rules::scenarios;
use std::process::Command;
use t019_support::tables;

/// `(scenario, ticks, seed, hash)`.
const LITERALS: [(&str, u64, u64, u64); 3] = [
    ("march_single", 4000, 1, 0x49a6_1801_5d2d_9397),
    ("deploy_pair", 3000, 2, 0xbd8a_5938_d0a7_7d43),
    ("march_large", 600, 42, 0xcac6_ac5c_d49e_f04b),
];

const CHILD_ENV: &str = "T4_12_CHILD";

fn hash_of(name: &str, ticks: u64, seed: u64) -> u64 {
    let mut b = scenarios::by_name(name, &tables(), seed).unwrap();
    for _ in 0..ticks {
        b.step_movement();
    }
    b.hash()
}

fn is_large(name: &str) -> bool {
    name == "march_large"
}

fn check(name: &str) {
    let &(_, ticks, seed, want) = LITERALS.iter().find(|l| l.0 == name).unwrap();
    let got = hash_of(name, ticks, seed);
    println!("{name} tick {ticks} seed {seed}: {got:016x}");
    assert_eq!(got, want, "{name} hash changed: {got:016x}");
    assert_eq!(
        hash_of(name, ticks, seed),
        got,
        "{name}: two runs in one process differ"
    );
}

#[test]
fn march_single_hash_is_recorded() {
    check("march_single");
}

#[test]
fn deploy_pair_hash_is_recorded() {
    check("deploy_pair");
}

/// 20,000 soldiers for 600 ticks, twice: release builds only.
#[test]
#[cfg_attr(
    debug_assertions,
    ignore = "large scenario: run with --release or --ignored"
)]
fn march_large_hash_is_recorded() {
    check("march_large");
}

/// In a child process (see `a_second_process_agrees`) prints the hashes; otherwise does nothing.
#[test]
fn child_probe() {
    if std::env::var(CHILD_ENV).is_err() {
        return;
    }
    for &(name, ticks, seed, _) in &LITERALS {
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
        let want = LITERALS.iter().find(|l| l.0 == name).unwrap().3;
        assert_eq!(
            u64::from_str_radix(hex, 16).unwrap(),
            want,
            "{name} differs in the child"
        );
        seen += 1;
    }
    assert_eq!(seen, if cfg!(debug_assertions) { 2 } else { 3 });
}
