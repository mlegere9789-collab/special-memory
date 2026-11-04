//! T7-17 (SPEC-007 section 12, hypothesis H7), ignored: `step_command` with 330 officers, 270
//! regiments and 300 messages in flight, release build, single thread. Prints mean, p99 and max
//! per tick in microseconds. The numbers are provisional; the test asserts only that the layer
//! was busy, never a time. Run with
//! `cargo test --release -p battle-rules --test t7_17_perf -- --ignored --nocapture`.

use battle_rules::command::CommandTable;
use battle_rules::scenarios::command::command_perf;
use battle_rules::Tables;
use std::time::Instant;

const RULES_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/rules");
const TICKS: usize = 2000;

#[test]
#[ignore = "performance measurement: run with --release --ignored --nocapture"]
fn step_command_with_330_officers_270_regiments_300_messages() {
    let tables = Tables::load_dir(RULES_DIR).unwrap();
    let text = std::fs::read_to_string(format!("{RULES_DIR}/command.toml")).unwrap();
    let table = CommandTable::from_toml_str(&text).unwrap();
    let mut b = command_perf(&tables, &table, 1);
    let cmd = b.cmd.as_ref().unwrap();
    assert_eq!((cmd.officers.len(), b.regiments.len()), (330, 280));
    let mut in_flight = 0;
    let mut micros = Vec::with_capacity(TICKS);
    for _ in 0..TICKS {
        in_flight = in_flight.max(b.cmd.as_ref().unwrap().msgs.len());
        let t0 = Instant::now();
        b.step_command(&table);
        micros.push(t0.elapsed().as_nanos() as u64);
        b.world.advance_tick();
    }
    micros.sort_unstable();
    let mean = micros.iter().sum::<u64>() / micros.len() as u64;
    println!(
        "H7 step_command: mean {} ns, p99 {} ns, max {} ns, peak messages in flight {in_flight}",
        mean,
        micros[micros.len() * 99 / 100],
        micros[micros.len() - 1]
    );
    assert!(in_flight >= 300);
}
