//! T5-21 (ignored, prints numbers): `f0_large`, 20,000 soldiers all at will, 600 ticks of
//! `BattleState::step`, and the one-tick volley of all 20,000.
//! Run with `cargo test --release -p battle-rules --test t5_21_perf -- --ignored --nocapture`.

use battle_rules::scenarios::fire::{f0_large, f0_volley, F0_LARGE_MEN};
use battle_rules::Tables;
use std::time::Instant;

const RULES_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/rules");

#[test]
#[ignore = "performance measurement: prints numbers, asserts nothing about speed"]
fn f0_large_step_timing() {
    let mut b = f0_large(&Tables::load_dir(RULES_DIR).unwrap(), 42);
    assert_eq!(b.world.agents.len(), F0_LARGE_MEN);
    let (mut us, mut in_flight) = (Vec::new(), 0usize);
    for _ in 0..600 {
        let t = Instant::now();
        b.step();
        us.push(t.elapsed().as_micros());
        in_flight = in_flight.max(b.pool.len());
    }
    us.sort_unstable();
    let mean = us.iter().sum::<u128>() / us.len() as u128;
    let n = b.counters();
    println!(
        "f0_large step over 600 ticks: mean {mean} us, p99 {} us, max {} us; shots {} misfires {} peak in flight {in_flight} casualties {} hash {:016x}",
        us[us.len() * 99 / 100],
        us[us.len() - 1],
        n.shots_fired,
        n.misfires,
        b.casualties().len(),
        b.hash()
    );
}

#[test]
#[ignore = "performance measurement: prints numbers, asserts nothing about speed"]
fn one_tick_volley_of_20000() {
    let mut b = f0_volley(&Tables::load_dir(RULES_DIR).unwrap(), 42);
    let t = Instant::now();
    b.step();
    let first = t.elapsed().as_micros();
    let t = Instant::now();
    b.step();
    println!(
        "volley tick: {first} us (shots {}), next tick {} us (in flight {})",
        b.counters().shots_fired,
        t.elapsed().as_micros(),
        b.pool.len()
    );
}
