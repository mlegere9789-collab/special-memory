//! T12-13 (SPEC-012 section 11): the scenes `seven_pines`, `perryville`, `bombardment`,
//! `ridge_pair` and `volley_20k` hash to the literals recorded once on Linux in
//! `qc/golden-replays/expected-acoustics.json`; a second process agrees and the order of the
//! events never matters. Set `HD_SIM_BLESS=1` to rewrite the file.

mod common;
use acoustics_model::{packet_hash, report_hash, scenes};
use common::*;
use std::process::Command;

const FILE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../qc/golden-replays/expected-acoustics.json"
);
const CHILD_ENV: &str = "T12_13_CHILD";

/// `(row prefix, packet hash, report hash)` for every case of every scene.
fn digest() -> Vec<(String, u64, u64)> {
    let t = tables();
    let mut rows = Vec::new();
    for s in scenes::all() {
        for &(weather, at_mm) in &s.cases {
            let o = s.run(&t, weather, at_mm);
            rows.push((
                format!(
                    "{{\"scene\": \"{}\", \"weather\": \"{weather}\", \"at_mm\": {at_mm}, ",
                    s.name
                ),
                packet_hash(&o.packet),
                report_hash(&o.report),
            ));
        }
    }
    rows
}

fn line((prefix, p, r): &(String, u64, u64)) -> String {
    format!("    {prefix}\"packet_hash\": \"{p:016x}\", \"report_hash\": \"{r:016x}\"}}")
}

fn recorded() -> Vec<String> {
    let text = std::fs::read_to_string(FILE).expect("expected-acoustics.json is present");
    text.lines()
        .filter(|l| l.contains("\"scene\""))
        .map(|l| l.trim_end_matches(',').to_string())
        .collect()
}

#[test]
fn the_scenes_have_the_stated_shape() {
    let t = tables();
    let names: Vec<&str> = scenes::all().iter().map(|s| s.name).collect();
    assert_eq!(
        names,
        [
            "seven_pines",
            "perryville",
            "bombardment",
            "ridge_pair",
            "volley_20k"
        ]
    );
    let count = |n: &str| {
        let s = scenes::by_name(n).unwrap();
        let cl = s.clusters();
        (
            s.events.len(),
            cl.len(),
            cl.iter().map(|c| c.count).collect::<Vec<_>>(),
        )
    };
    assert_eq!(count("seven_pines"), (240, 6, vec![40; 6]));
    assert_eq!(count("perryville"), (800, 20, vec![40; 20]));
    assert_eq!(count("bombardment"), (3, 1, vec![3]));
    assert_eq!(count("volley_20k").0, 20_000);
    assert!(scenes::by_name("ridge_pair").unwrap().relief.is_some());
    assert!(scenes::by_name("nowhere").is_none());
    // The listener stands exactly at the stated distance from the scene origin.
    let l = scenes::by_name("seven_pines")
        .unwrap()
        .listener(&t, 3_218_688);
    assert_eq!(acoustics_model::to_mm(l.pos.x), 3_218_688);
}

#[test]
fn the_ridges_and_woods_of_ridge_pair_cost_level() {
    let t = tables();
    let mut s = scenes::by_name("ridge_pair").unwrap();
    let with = s.run(&t, "calm", 2_000_000).report.margin_cdb;
    s.relief = None;
    let without = s.run(&t, "calm", 2_000_000).report.margin_cdb;
    assert!(with < without, "{with} against {without}");
}

#[test]
fn the_hashes_are_recorded() {
    let got: Vec<String> = digest().iter().map(line).collect();
    for l in &got {
        println!("{l}");
    }
    assert_eq!(got, recorded(), "an acoustics golden changed");
}

#[test]
fn the_order_of_the_events_never_matters() {
    let t = tables();
    for mut s in scenes::all() {
        let (w, d) = s.cases[0];
        let a = s.run(&t, w, d);
        let mut rng = Rng(0x5eed);
        rng.shuffle(&mut s.events);
        let b = s.run(&t, w, d);
        assert_eq!(packet_hash(&a.packet), packet_hash(&b.packet), "{}", s.name);
        assert_eq!(report_hash(&a.report), report_hash(&b.report), "{}", s.name);
    }
}

/// Rewrites `expected-acoustics.json` from the current build when `HD_SIM_BLESS=1`.
#[test]
fn bless_when_requested() {
    if std::env::var("HD_SIM_BLESS").as_deref() != Ok("1") {
        return;
    }
    let rows: Vec<String> = digest().iter().map(line).collect();
    std::fs::write(
        FILE,
        format!(
            "{{\n  \"format\": 1,\n  \"rows\": [\n{}\n  ]\n}}\n",
            rows.join(",\n")
        ),
    )
    .unwrap();
}

/// In a child process prints every row; otherwise does nothing.
#[test]
fn child_probe() {
    if std::env::var(CHILD_ENV).is_ok() {
        for r in digest() {
            println!("PROBE {}", line(&r));
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
    let got: Vec<String> = text
        .lines()
        .filter_map(|l| l.split_once("PROBE ").map(|p| p.1.to_string()))
        .collect();
    assert_eq!(got, recorded());
}
