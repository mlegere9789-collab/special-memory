//! `verify` of a replay bound to a data scenario (SPEC-003 amendment A2): the ScenarioBind input
//! is checked against the scenario file, then the replay is re-run with [`Battle::run`].

use crate::resolve::{agent_bound, load_scenario, make_battle, Loaded};
use crate::scenario::{check_limits, Verdict};
use battle_rules::registry::{ACTOR_SYSTEM, SEQ_SCENARIO_BIND, TAG_SCENARIO_BIND};
use sim_state::Replay;
use std::path::Path;

/// The hash a replay's ScenarioBind input carries, if it is a bound replay: exactly one input,
/// at `(0, 0xFF, 0)`, nine bytes, tag `0x20`.
pub fn bound_hash(replay: &Replay) -> Result<u64, String> {
    let [one] = replay.inputs() else {
        return Err("expected exactly one input, the scenario binding".to_owned());
    };
    if (one.tick, one.actor, one.seq) != (0, ACTOR_SYSTEM, SEQ_SCENARIO_BIND) {
        return Err("the first input is not the scenario binding at (0, 0xFF, 0)".to_owned());
    }
    match one.payload.as_slice() {
        [TAG_SCENARIO_BIND, rest @ ..] if rest.len() == 8 => {
            let mut b = [0u8; 8];
            b.copy_from_slice(rest);
            Ok(u64::from_le_bytes(b))
        }
        _ => Err("the scenario binding is not `0x20` and eight bytes of hash".to_owned()),
    }
}

/// Whether the checkpoints are exactly one per interval over the run.
fn schedule_ok(replay: &Replay) -> bool {
    let interval = u64::from(replay.checkpoint_interval());
    let expected = replay.total_ticks().checked_div(interval).unwrap_or(0);
    replay.checkpoints().len() as u64 == expected
        && replay
            .checkpoints()
            .iter()
            .zip(1u64..)
            .all(|(c, n)| c.tick == n * interval)
}

/// Checks that `replay` belongs to the scenario file: same hash, same class, tick rate 20.
fn check_binding(replay: &Replay, l: &Loaded) -> Result<(), String> {
    let stored = bound_hash(replay)?;
    if stored != l.hash {
        return Err(format!(
            "scenario hash mismatch: replay {stored:016x}, file {:016x}",
            l.hash
        ));
    }
    if replay.fidelity_class() != l.scenario.fidelity_class {
        return Err(format!(
            "fidelity class mismatch: replay {}, scenario {}",
            replay.fidelity_class(),
            l.scenario.fidelity_class
        ));
    }
    if u32::from(replay.tick_hz()) != l.scenario.tick_hz {
        return Err(format!(
            "tick rate {} is not the scenario's",
            replay.tick_hz()
        ));
    }
    Ok(())
}

/// Re-runs a bound replay against `data` and compares every checkpoint and the final hash.
pub fn verify_bound(replay: &Replay, data: &Path) -> Verdict {
    match verify(replay, data) {
        Ok(v) => v,
        Err(why) => Verdict::Malformed(why),
    }
}

fn verify(replay: &Replay, data: &Path) -> Result<Verdict, String> {
    let l = load_scenario(data, replay.scenario_id())?;
    check_binding(replay, &l)?;
    let total = replay.total_ticks();
    check_limits(agent_bound(&l.scenario), total).map_err(|e| e.to_string())?;
    if replay.checkpoint_interval() == 0 && total > 0 {
        return Err("checkpoint interval 0 with a non-zero tick count".to_owned());
    }
    if !schedule_ok(replay) {
        return Err("checkpoints do not follow the header interval".to_owned());
    }
    let mut battle = make_battle(&l, replay.seed())?;
    for c in replay.checkpoints() {
        battle.run(c.tick);
        let actual = battle.hash();
        if actual != c.hash {
            return Ok(Verdict::Mismatch {
                tick: c.tick,
                expected: c.hash,
                actual,
            });
        }
    }
    battle.run(total);
    let actual = battle.hash();
    if actual != replay.final_hash() {
        return Ok(Verdict::Mismatch {
            tick: total,
            expected: replay.final_hash(),
            actual,
        });
    }
    Ok(Verdict::Match)
}
