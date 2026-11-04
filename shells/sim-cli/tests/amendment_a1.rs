//! SPEC-003 Amendment A1 (resource limits, checkpoint schedule) and mutation-hardening tests.

mod common;

use common::{craft_replay, tmp_path, verify_guarded};
use hd_sim::bench::{micros, p99_nearest_rank};
use hd_sim::scenario::{
    check_limits, run_replay, verify_replay, ScenarioError, ToyParams, Verdict, MAX_AGENTS,
    MAX_AGENT_TICKS, MAX_TOTAL_TICKS,
};
use sim_state::{Replay, ReplayWriter};
use std::time::Duration;

const GUARD: Duration = Duration::from_secs(20);

fn is_limit_err(r: Result<(), ScenarioError>) -> bool {
    matches!(r, Err(ScenarioError::InvalidParams(_)))
}

// ------------------------------------------------------------------ limits

#[test]
fn limit_constants_match_the_spec() {
    assert_eq!(MAX_TOTAL_TICKS, 10_000_000);
    assert_eq!(MAX_AGENT_TICKS, 20_000_000_000);
    assert_eq!(MAX_AGENTS, 1_000_000);
}

/// Targets the mutants that move the `MAX_AGENTS` and `total_ticks` comparison boundaries:
/// the limit value is accepted, the limit plus one is rejected.
#[test]
fn check_limits_boundaries() {
    // agents
    assert!(check_limits(MAX_AGENTS, 0).is_ok());
    assert!(is_limit_err(check_limits(MAX_AGENTS + 1, 0)));
    assert!(is_limit_err(check_limits(u32::MAX, 0)));
    // total ticks (with no agents the product limit is not in play)
    assert!(check_limits(0, MAX_TOTAL_TICKS).is_ok());
    assert!(is_limit_err(check_limits(0, MAX_TOTAL_TICKS + 1)));
    assert!(is_limit_err(check_limits(0, u64::MAX)));
    // agents * ticks
    assert!(check_limits(2000, 10_000_000).is_ok());
    assert!(is_limit_err(check_limits(2001, 10_000_000)));
    assert!(check_limits(4000, 5_000_000).is_ok());
    assert!(is_limit_err(check_limits(4000, 5_000_001)));
    assert!(check_limits(1, 10_000_000).is_ok());
    assert!(is_limit_err(check_limits(MAX_AGENTS, 20_001)));
    assert!(check_limits(MAX_AGENTS, 20_000).is_ok());
    // no overflow on the extremes
    assert!(is_limit_err(check_limits(u32::MAX, u64::MAX)));
    // readable message
    let Err(ScenarioError::InvalidParams(m)) = check_limits(MAX_AGENTS + 1, 0) else {
        panic!("expected an error");
    };
    assert!(m.contains("agents"), "{m}");
}

#[test]
fn run_and_hash_and_bench_reject_limit_violations_with_exit_2() {
    let out = tmp_path("limit-run");
    let o = out.to_str().unwrap();
    for args in [
        vec!["run", "--agents", "10", "--ticks", "10000001", "--out", o],
        vec!["run", "--agents", "1000001", "--ticks", "0", "--out", o],
        vec!["run", "--agents", "2001", "--ticks", "10000000", "--out", o],
        vec![
            "hash", "--agents", "10", "--ticks", "10000001", "--seed", "1",
        ],
        vec!["hash", "--agents", "1000001", "--ticks", "0", "--seed", "1"],
        vec!["bench", "--agents", "10", "--ticks", "10000001"],
    ] {
        let (code, _, err) = common::process_with_timeout(&args, GUARD);
        assert_eq!(code, 2, "{args:?}: {err}");
        assert!(!err.trim().is_empty(), "{args:?} gave no message");
    }
    assert!(!out.exists(), "a rejected run must not write a file");
}

#[test]
fn run_with_zero_interval_and_ticks_is_rejected_but_zero_ticks_is_fine() {
    let out = tmp_path("interval-zero");
    let o = out.to_str().unwrap();
    let (code, _, err) = common::cli(&[
        "run",
        "--agents",
        "5",
        "--ticks",
        "3",
        "--checkpoint-interval",
        "0",
        "--out",
        o,
    ]);
    assert_eq!(code, 2, "{err}");
    assert!(!out.exists());
    let (code, _, err) = common::cli(&[
        "run",
        "--agents",
        "5",
        "--ticks",
        "0",
        "--checkpoint-interval",
        "0",
        "--out",
        o,
    ]);
    assert_eq!(code, 0, "{err}");
    let (code, _, err) = common::cli(&["verify", o]);
    assert_eq!(code, 0, "{err}");
    let _ = std::fs::remove_file(out);
}

/// The agent limit itself is accepted end to end (a zero-tick replay with `MAX_AGENTS` agents),
/// and one more is rejected, both through `verify`.
#[test]
fn verify_accepts_max_agents_and_rejects_one_more() {
    let ok = craft_replay(MAX_AGENTS, 10, 0, &[], (0, 0, 0));
    let (code, out, err) = verify_guarded(&ok, Duration::from_secs(120));
    // The stored final hash is a dummy, so the run itself is a mismatch (1), not a rejection (2).
    assert_eq!(code, 1, "stdout={out} stderr={err}");
    let bad = craft_replay(MAX_AGENTS + 1, 10, 0, &[], (0, 0, 0));
    let (code, _, err) = verify_guarded(&bad, GUARD);
    assert_eq!(code, 2, "{err}");
}

/// The crafted replay from the review: agents=10, total_ticks=u64::MAX, interval 0, valid body
/// hash. It must be rejected immediately, not run forever.
#[test]
fn crafted_replay_with_huge_total_ticks_exits_2_immediately() {
    let b = craft_replay(10, 0, u64::MAX, &[], (0, 0, 0));
    let started = std::time::Instant::now();
    let (code, _, err) = verify_guarded(&b, Duration::from_secs(10));
    assert_eq!(code, 2, "{err}");
    assert!(!err.trim().is_empty());
    assert!(started.elapsed() < Duration::from_secs(10));
    // The same with a non-zero interval, where the checkpoint schedule would also not fit.
    let b = craft_replay(10, 1, u64::MAX, &[], (0, 0, 0));
    assert_eq!(verify_guarded(&b, Duration::from_secs(10)).0, 2);
    // A huge interval makes the schedule trivially empty; the tick limit alone must reject it.
    let b = craft_replay(10, u32::MAX, MAX_TOTAL_TICKS + 1, &[], (0, 0, 0));
    let (code, _, err) = verify_guarded(&b, Duration::from_secs(10));
    assert_eq!(code, 2, "{err}");
    assert!(err.contains("ticks"), "{err}");
    // And the product limit.
    let b = craft_replay(2001, u32::MAX, MAX_TOTAL_TICKS, &[], (0, 0, 0));
    assert_eq!(verify_guarded(&b, Duration::from_secs(10)).0, 2);
    // Interval zero with a modest total is malformed too.
    let b = craft_replay(10, 0, 5, &[], (0, 0, 0));
    assert_eq!(verify_guarded(&b, Duration::from_secs(10)).0, 2);
}

#[test]
fn verify_replay_checks_limits_before_anything_else_runs() {
    let mut w = ReplayWriter::new(hd_sim::scenario::SCENARIO_ID, 1, 20, 0, u32::MAX);
    let mut payload = Vec::new();
    for v in [10u32, 2000, 2000] {
        payload.extend_from_slice(&v.to_le_bytes());
    }
    w.push_input(0, 0, 0, payload);
    let r = w.finish(MAX_TOTAL_TICKS + 1, 0).unwrap();
    assert!(matches!(verify_replay(&r), Verdict::Malformed(_)));
}

// ------------------------------------------------------------------ checkpoint schedule

fn rebuild_with(
    replay: &Replay,
    keep: impl Fn(usize, u64) -> bool,
    extra: &[(u64, u64)],
) -> Replay {
    let mut w = ReplayWriter::new(
        replay.scenario_id(),
        replay.seed(),
        replay.tick_hz(),
        replay.fidelity_class(),
        replay.checkpoint_interval(),
    );
    for i in replay.inputs() {
        w.push_input(i.tick, i.actor, i.seq, i.payload.clone());
    }
    // The writer requires strictly increasing ticks (SPEC-002 A1), so merge extras in tick order.
    let mut all: Vec<(u64, u64)> = replay
        .checkpoints()
        .iter()
        .enumerate()
        .filter(|&(n, c)| keep(n, c.tick))
        .map(|(_, c)| (c.tick, c.hash))
        .collect();
    all.extend_from_slice(extra);
    all.sort_by_key(|&(t, _)| t);
    for (t, h) in all {
        w.push_checkpoint(t, h);
    }
    w.finish(replay.total_ticks(), replay.final_hash()).unwrap()
}

fn verify_file(r: &Replay) -> i32 {
    let (code, _, _) = verify_guarded(&r.to_bytes(), GUARD);
    code
}

/// Targets the mutant that accepts a replay with fewer checkpoints than scheduled
/// (`len == expected` weakened to `len <= expected`).
#[test]
fn missing_or_extra_checkpoints_are_malformed() {
    let replay = run_replay(&ToyParams::new(40, 5), 300, 100).unwrap();
    assert_eq!(replay.checkpoints().len(), 3);
    assert_eq!(verify_replay(&replay), Verdict::Match);
    assert_eq!(verify_file(&replay), 0);

    // Trailing checkpoint missing.
    let missing_last = rebuild_with(&replay, |n, _| n < 2, &[]);
    assert!(matches!(
        verify_replay(&missing_last),
        Verdict::Malformed(_)
    ));
    assert_eq!(verify_file(&missing_last), 2);
    // Several trailing checkpoints missing.
    let only_first = rebuild_with(&replay, |n, _| n < 1, &[]);
    assert!(matches!(verify_replay(&only_first), Verdict::Malformed(_)));
    assert_eq!(verify_file(&only_first), 2);
    // All missing.
    let none = rebuild_with(&replay, |_, _| false, &[]);
    assert!(matches!(verify_replay(&none), Verdict::Malformed(_)));
    assert_eq!(verify_file(&none), 2);
    // Leading checkpoint missing (count short and ticks shifted).
    let missing_first = rebuild_with(&replay, |n, _| n > 0, &[]);
    assert!(matches!(
        verify_replay(&missing_first),
        Verdict::Malformed(_)
    ));
    assert_eq!(verify_file(&missing_first), 2);
    // One checkpoint too many (at a tick inside the run).
    let extra = rebuild_with(&replay, |_, _| true, &[(250, 7)]);
    assert!(extra.checkpoints().len() == 4);
    assert!(matches!(verify_replay(&extra), Verdict::Malformed(_)));
    assert_eq!(verify_file(&extra), 2);
    // A checkpoint off the schedule.
    // (Same count as scheduled, ticks 100, 250, 300: strictly increasing, so the writer accepts
    // it, but 250 is not a multiple of the interval.)
    let shifted = rebuild_with(&replay, |n, _| n < 1, &[(250, 1), (300, 2)]);
    assert_eq!(shifted.checkpoints().len(), 3);
    assert!(matches!(verify_replay(&shifted), Verdict::Malformed(_)));
    assert_eq!(verify_file(&shifted), 2);
}

/// Non-multiples of the interval: 250 ticks at interval 100 schedules exactly two checkpoints.
#[test]
fn schedule_count_is_floor_of_total_over_interval() {
    let replay = run_replay(&ToyParams::new(20, 6), 250, 100).unwrap();
    assert_eq!(replay.checkpoints().len(), 2);
    assert_eq!(verify_replay(&replay), Verdict::Match);
    let short = rebuild_with(&replay, |n, _| n < 1, &[]);
    assert!(matches!(verify_replay(&short), Verdict::Malformed(_)));
}

// ------------------------------------------------------------------ setup input

/// Targets the mutant that drops the actor (or tick, or seq) comparison of the setup input check.
#[test]
fn setup_input_must_be_actor_0_tick_0_seq_0() {
    let good = craft_replay(20, 10, 0, &[], (0, 0, 0));
    let r = sim_state::ReplayReader::parse(&good).unwrap();
    assert!(hd_sim::scenario::params_from_replay(&r).is_ok());
    for setup in [
        (0u64, 1u8, 0u16),
        (0, 255, 0),
        (1, 0, 0),
        (0, 0, 1),
        (5, 3, 9),
    ] {
        let bytes = craft_replay(20, 10, 0, &[], setup);
        let r = sim_state::ReplayReader::parse(&bytes).unwrap();
        assert!(
            hd_sim::scenario::params_from_replay(&r).is_err(),
            "setup {setup:?} accepted"
        );
        assert!(matches!(verify_replay(&r), Verdict::Malformed(_)));
        let (code, _, err) = verify_guarded(&bytes, GUARD);
        assert_eq!(code, 2, "setup {setup:?}: {err}");
    }
}

// ------------------------------------------------------------------ bench formatting

/// Targets the mutant that rounds the 99th-percentile rank down (or drops the `ceil`).
#[test]
fn p99_uses_nearest_rank_with_ceiling() {
    let seq = |n: u64| (1..=n).collect::<Vec<u64>>();
    assert_eq!(p99_nearest_rank(&[]), 0);
    assert_eq!(p99_nearest_rank(&[42]), 42);
    assert_eq!(p99_nearest_rank(&seq(2)), 2);
    assert_eq!(p99_nearest_rank(&seq(10)), 10); // ceil(9.9) = 10
    assert_eq!(p99_nearest_rank(&seq(50)), 50); // ceil(49.5) = 50
    assert_eq!(p99_nearest_rank(&seq(100)), 99); // exactly 99
    assert_eq!(p99_nearest_rank(&seq(101)), 100); // ceil(99.99) = 100
    assert_eq!(p99_nearest_rank(&seq(150)), 149); // ceil(148.5) = 149
    assert_eq!(p99_nearest_rank(&seq(199)), 198); // ceil(197.01) = 198
    assert_eq!(p99_nearest_rank(&seq(200)), 198);
    assert_eq!(p99_nearest_rank(&seq(1000)), 990);
}

/// Targets the mutant that drops the zero padding of the microsecond fraction.
#[test]
fn micros_pads_three_decimals() {
    assert_eq!(micros(0), "0.000");
    assert_eq!(micros(5), "0.005");
    assert_eq!(micros(42), "0.042");
    assert_eq!(micros(999), "0.999");
    assert_eq!(micros(1000), "1.000");
    assert_eq!(micros(1005), "1.005");
    assert_eq!(micros(1050), "1.050");
    assert_eq!(micros(12_340), "12.340");
    assert_eq!(micros(1_000_007), "1000.007");
    assert_eq!(micros(u64::MAX), "18446744073709551.615");
}
