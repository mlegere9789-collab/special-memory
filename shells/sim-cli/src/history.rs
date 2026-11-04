//! `verify` for replay-mode headers (`<scenario>/<variant>`, SPEC-011 section 4.2): the scripted
//! inputs of actor `0xFD` are re-applied to the staged battle of
//! `data/scenarios/<scenario>/` and every checkpoint and the final hash must equal the history
//! hash.

use crate::scenario::{Verdict, MAX_TOTAL_TICKS};
use battle_rules::registry::ACTOR_HISTORY_SCRIPT;
use battle_rules::{Tables, TICK_HZ};
use history_cal::{
    apply_variant, stage_battle, units_from_toml_str, ReplayDriver, ReplayScript, ScriptInput,
    Variant,
};
use sim_state::Replay;
use std::path::Path;

/// True when the header's scenario id has the replay-mode form `<scenario>/<variant>`.
pub fn is_history_replay(replay: &Replay) -> bool {
    replay.scenario_id().contains('/')
}

fn read(root: &Path, rel: &str) -> Result<String, String> {
    let path = root.join(rel);
    std::fs::read_to_string(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))
}

/// Re-runs a replay-mode `replay` with the data files under `root` (the repository root).
///
/// Malformed: an id that is not `<scenario>/<variant>` with a known variant and a scenario of
/// `[a-z0-9_]+`, an unreadable or invalid data file, a header that disagrees with the script
/// (class, tick rate), an input of another actor, a refused input, or a checkpoint schedule that
/// does not follow the header interval.
pub fn verify_history(replay: &Replay, root: &Path) -> Verdict {
    match run(replay, root) {
        Ok(v) => v,
        Err(m) => Verdict::Malformed(m),
    }
}

fn run(replay: &Replay, root: &Path) -> Result<Verdict, String> {
    let (scenario, variant) = replay.scenario_id().split_once('/').unwrap_or_default();
    let valid = !scenario.is_empty()
        && scenario
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
    let variant = Variant::from_name(variant).filter(|_| valid);
    let Some(variant) = variant else {
        return Err(format!(
            "bad replay-mode scenario id {:?}",
            replay.scenario_id()
        ));
    };
    let dir = format!("data/scenarios/{scenario}");
    let script = ReplayScript::from_toml_str(&read(root, &format!("{dir}/script.toml"))?)
        .map_err(|e| e.to_string())?;
    let units = units_from_toml_str(&read(root, &format!("{dir}/units.toml"))?)
        .map_err(|e| e.to_string())?;
    let rules = root.join("data/rules");
    let tables = Tables::load_dir(&rules.to_string_lossy()).map_err(|e| e.to_string())?;
    if script.meta.scenario != scenario
        || u32::from(replay.tick_hz()) != TICK_HZ
        || replay.fidelity_class() != script.meta.fidelity_class
    {
        return Err("replay header disagrees with the script".to_string());
    }
    if replay.fidelity_class() != 0 {
        return Err("only fidelity class 0 replays can be verified".to_string());
    }
    let (interval, total) = (
        u64::from(replay.checkpoint_interval()),
        replay.total_ticks(),
    );
    if total > MAX_TOTAL_TICKS || (interval == 0 && total > 0) {
        return Err(format!(
            "unusable tick count {total} or interval {interval}"
        ));
    }
    let on_schedule = replay.checkpoints().len() as u64 == total.checked_div(interval).unwrap_or(0)
        && replay
            .checkpoints()
            .iter()
            .zip(1u64..)
            .all(|(c, n)| c.tick == n * interval);
    if !on_schedule {
        return Err(format!(
            "checkpoints do not follow the header interval {interval} over {total} ticks"
        ));
    }
    if let Some(i) = replay
        .inputs()
        .iter()
        .find(|i| i.actor != ACTOR_HISTORY_SCRIPT)
    {
        return Err(format!(
            "input at tick {} has actor {:#04x}, not 0xfd",
            i.tick, i.actor
        ));
    }
    let script = apply_variant(&script, variant);
    let inputs: Vec<ScriptInput> = replay
        .inputs()
        .iter()
        .map(|i| ScriptInput {
            tick: i.tick,
            seq: i.seq,
            payload: i.payload.clone(),
        })
        .collect();
    let battle =
        stage_battle(&units, &script, &tables, replay.seed()).map_err(|e| e.to_string())?;
    let mut driver = ReplayDriver::new(battle, inputs, &script).map_err(|e| e.to_string())?;
    let mut checkpoints = replay.checkpoints().iter().peekable();
    for tick in 1..=total {
        driver.step().map_err(|e| e.to_string())?;
        if let Some(c) = checkpoints.next_if(|c| c.tick == tick) {
            let actual = driver.history_hash();
            if actual != c.hash {
                return Ok(Verdict::Mismatch {
                    tick,
                    expected: c.hash,
                    actual,
                });
            }
        }
    }
    let actual = driver.history_hash();
    Ok(if actual == replay.final_hash() {
        Verdict::Match
    } else {
        Verdict::Mismatch {
            tick: total,
            expected: replay.final_hash(),
            actual,
        }
    })
}
