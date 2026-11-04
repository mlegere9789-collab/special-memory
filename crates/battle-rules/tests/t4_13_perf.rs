//! T4-13 (ignored, prints numbers): the movement step of `march_large`, 20,000 soldiers.
//! Run with `cargo test --release -p battle-rules --test t4_13_perf -- --ignored --nocapture`.

mod t019_support;

use battle_rules::scenarios;
use std::time::Instant;
use t019_support::tables;

#[test]
#[ignore = "performance measurement: prints numbers, asserts nothing about speed"]
fn march_large_movement_step_timing() {
    let mut b = scenarios::march_large(&tables(), 42);
    assert_eq!(b.world.agents.len(), 20_000);
    let mut us: Vec<u128> = Vec::new();
    for _ in 0..600 {
        let t = Instant::now();
        b.step_movement();
        us.push(t.elapsed().as_micros());
    }
    us.sort_unstable();
    let mean = us.iter().sum::<u128>() / us.len() as u128;
    let p99 = us[us.len() * 99 / 100];
    println!(
        "march_large step_movement over 600 ticks: mean {mean} us, p99 {p99} us, max {} us, hash {:016x}",
        us[us.len() - 1],
        b.hash()
    );
}
