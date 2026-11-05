//! T9-18 (SPEC-009 section 12, hypothesis H9), ignored: `f0_large` with the environment on, and
//! the full-grid smoke update on 750 by 625 cells, release build, single thread. Prints mean,
//! p99 and max per tick in microseconds (the update's own time for the grid test). The numbers
//! are provisional; the tests assert only that the work happened, never a time. Run with
//! `cargo test --release -p battle-rules --test t9_18_perf -- --ignored --nocapture`.

use battle_rules::scenarios::env::f0_large_env;
use battle_rules::{EnvTable, SmokeGrid, Tables, TerrainTable};
use core_math::Vec2Fx;
use std::time::Instant;

const RULES_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/rules");
const TICKS: usize = 1200;

fn stats(name: &str, mut ns: Vec<u64>) {
    ns.sort_unstable();
    let mean = ns.iter().sum::<u64>() / ns.len() as u64;
    println!(
        "H9 {name}: mean {} ns, p99 {} ns, max {} ns over {} samples",
        mean,
        ns[ns.len() * 99 / 100],
        ns[ns.len() - 1],
        ns.len()
    );
}

#[test]
#[ignore = "performance measurement: run with --release --ignored --nocapture"]
fn f0_large_with_the_environment_on() {
    let tables = Tables::load_dir(RULES_DIR).unwrap();
    let mut b = f0_large_env(&tables, 42);
    assert!(b.env.is_some());
    // The same battle with the environment off, to read the added cost.
    let mut base = b.clone();
    base.env = None;
    let (mut on, mut off) = (Vec::with_capacity(TICKS), Vec::with_capacity(TICKS));
    for _ in 0..TICKS {
        let t0 = Instant::now();
        b.step();
        on.push(t0.elapsed().as_nanos() as u64);
        let t0 = Instant::now();
        base.step();
        off.push(t0.elapsed().as_nanos() as u64);
    }
    stats("f0_large env on, step", on);
    stats("f0_large env off, step", off);
    assert!(b.counters().shots_fired > 0 && b.smoke.total() > 0);
}

#[test]
#[ignore = "performance measurement: run with --release --ignored --nocapture"]
fn the_full_grid_update_on_750_by_625_cells() {
    let tables = Tables::load_dir(RULES_DIR).unwrap();
    let text = std::fs::read_to_string(format!("{RULES_DIR}/environment.toml")).unwrap();
    let terrain = TerrainTable::from_toml_str(
        &std::fs::read_to_string(format!("{RULES_DIR}/terrain.toml")).unwrap(),
    )
    .unwrap();
    let env = EnvTable::from_toml_str(&text, &terrain).unwrap();
    let _ = &tables;
    let mut g = SmokeGrid::new(8000, 750, 625, Vec2Fx::ZERO).unwrap();
    for (i, d) in g.density.iter_mut().enumerate() {
        *d = 1 + (i as u32 * 7919) % 5000;
    }
    let mut ns = Vec::new();
    for _ in 0..200 {
        let t0 = Instant::now();
        g.update(&env.smoke, 2000, 1000, 0, &|_| 0);
        ns.push(t0.elapsed().as_nanos() as u64);
    }
    stats("smoke update, 750 x 625 full grid", ns);
    assert!(g.total() > 0);
}
