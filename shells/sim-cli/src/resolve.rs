//! `hd-sim resolve`: quick-resolve a data scenario and write its replay (SPEC-008 section 8,
//! SPEC-003 amendment A2).
//!
//! A bound replay has the scenario id and the seed in its header, the fidelity class `fc` of the
//! scenario, and one input: `(tick 0, actor 0xFF, seq 0)` with payload `u8 0x20, u64
//! scenario_hash` (little-endian). A checkpoint hash of [`Battle::hash`] follows every
//! [`CHECKPOINT_INTERVAL`] ticks.

use crate::scenario::{check_limits, MAX_TOTAL_TICKS};
use battle_rules::registry::{ACTOR_SYSTEM, SEQ_SCENARIO_BIND, TAG_SCENARIO_BIND};
use battle_rules::{
    scenario_hash, Arm, Battle, CasualtyKind, FidelityTable, Scenario, Side, Tables, TICK_HZ,
};
use sim_state::{Replay, ReplayWriter};
use std::fmt::Write as _;
use std::path::Path;

/// Ticks between checkpoint hashes of a resolve replay.
pub const CHECKPOINT_INTERVAL: u32 = 100;

/// A scenario with the rules it runs under.
pub struct Loaded {
    /// The validated scenario.
    pub scenario: Scenario,
    /// `data/rules` tables.
    pub tables: Tables,
    /// `data/rules/fidelity.toml`.
    pub fidelity: FidelityTable,
    /// `scenario_hash` of the scenario.
    pub hash: u64,
}

/// A scenario id is `[a-z0-9_-]{1,32}`; anything else never names a file.
fn valid_id(id: &str) -> bool {
    (1..=32).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}

fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))
}

/// Loads `<data>/scenarios/<id>.toml` (and its terrain rows file) with the rules of
/// `<data>/rules`.
pub fn load_scenario(data: &Path, id: &str) -> Result<Loaded, String> {
    if !valid_id(id) {
        return Err(format!("{id:?} is not a scenario id"));
    }
    let rules = data.join("rules");
    let rules_str = rules.to_str().ok_or("data directory is not valid UTF-8")?;
    let tables = Tables::load_dir(rules_str).map_err(|e| format!("rules: {e}"))?;
    let fidelity = FidelityTable::from_toml_str(&read(&rules.join("fidelity.toml"))?)
        .map_err(|e| format!("fidelity.toml: {e}"))?;
    let dir = data.join("scenarios");
    let text = read(&dir.join(format!("{id}.toml")))?;
    let rows = match Scenario::terrain_source(&text).map_err(|e| e.to_string())? {
        Some(src) => {
            let plain = src.starts_with("terrain/")
                && !src.contains("..")
                && !src.contains('\\')
                && !src.starts_with('/');
            if !plain {
                return Err(format!("terrain source {src:?} is outside data/scenarios"));
            }
            Some(read(&dir.join(&src))?)
        }
        None => None,
    };
    let scenario = Scenario::from_toml_str(&text, &tables, rows.as_deref())
        .map_err(|e| format!("{id}: {e}"))?;
    if scenario.id != id {
        return Err(format!("{id}.toml names the scenario {:?}", scenario.id));
    }
    let hash = scenario_hash(&scenario);
    Ok(Loaded {
        scenario,
        tables,
        fidelity,
        hash,
    })
}

/// Soldiers the scenario can have in the agent store at once: all of its men (guns for a
/// battery), at most `agent_cap`. The work limit of amendment A1 uses this count.
pub fn agent_bound(s: &Scenario) -> u32 {
    let all: u64 = s
        .units
        .iter()
        .filter(|u| u.arm != Arm::Hq)
        .map(|u| u64::from(if u.arm == Arm::Battery { u.guns } else { u.men }))
        .sum();
    u32::try_from(all.min(u64::from(s.agent_cap))).unwrap_or(u32::MAX)
}

/// The ScenarioBind payload: `u8 0x20, u64 hash`.
pub fn bind_payload(hash: u64) -> Vec<u8> {
    let mut p = vec![TAG_SCENARIO_BIND];
    p.extend_from_slice(&hash.to_le_bytes());
    p
}

/// Builds the battle of a loaded scenario.
pub fn make_battle(l: &Loaded, seed: u64) -> Result<Battle, String> {
    Battle::from_scenario_with(&l.scenario, seed, l.tables.clone(), l.fidelity.clone())
        .map_err(|e| e.to_string())
}

/// Default length of a resolve: the whole scenario.
pub fn default_ticks(s: &Scenario) -> u64 {
    u64::from(s.duration_s) * u64::from(TICK_HZ)
}

/// Runs the scenario for `ticks` ticks and records the replay.
pub fn resolve(l: &Loaded, seed: u64, ticks: u64) -> Result<(Replay, Battle), String> {
    check_limits(agent_bound(&l.scenario), ticks).map_err(|e| e.to_string())?;
    if ticks > MAX_TOTAL_TICKS {
        return Err(format!("ticks {ticks} exceeds the limit {MAX_TOTAL_TICKS}"));
    }
    let mut battle = make_battle(l, seed)?;
    let mut w = ReplayWriter::new(
        l.scenario.id.clone(),
        seed,
        u16::try_from(l.scenario.tick_hz).unwrap_or(0),
        l.scenario.fidelity_class,
        CHECKPOINT_INTERVAL,
    );
    w.push_input(0, ACTOR_SYSTEM, SEQ_SCENARIO_BIND, bind_payload(l.hash));
    for tick in 1..=ticks {
        battle.step();
        if tick % u64::from(CHECKPOINT_INTERVAL) == 0 {
            w.push_checkpoint(tick, battle.hash());
        }
    }
    let replay = w.finish(ticks, battle.hash()).map_err(|e| e.to_string())?;
    Ok((replay, battle))
}

fn side_name(s: Side) -> &'static str {
    match s {
        Side::Union => "union",
        Side::Confederate => "confederate",
    }
}

/// The `key=value` lines `resolve` prints: header, final hash, ledger and casualties by side,
/// objective holders.
pub fn report(b: &Battle, seed: u64, ticks: u64) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "scenario={}", b.scenario.id);
    let _ = writeln!(s, "seed={seed}");
    let _ = writeln!(s, "ticks={ticks}");
    let _ = writeln!(s, "fidelity_class={}", b.scenario.fidelity_class);
    let _ = writeln!(s, "final_hash={:016x}", b.hash());
    for side in [Side::Union, Side::Confederate] {
        let team = side.team();
        let name = side_name(side);
        let l = b.ledger(team);
        let c = l.ledger;
        let _ = writeln!(
            s,
            "ledger.{name} present={} killed={} arrived={} departed={} held={} spent={} lost={} initial={} resupplied={}",
            c.present, c.killed, l.arrived, l.departed, c.held, c.spent, c.lost, c.initial, c.resupplied
        );
        let count = |k: CasualtyKind| {
            b.state
                .casualties()
                .iter()
                .filter(|x| x.team == team && x.kind == k)
                .count()
        };
        let _ = writeln!(
            s,
            "casualties.{name} killed={} wounded={} disabled={}",
            count(CasualtyKind::Killed),
            count(CasualtyKind::Wounded),
            count(CasualtyKind::Disabled)
        );
    }
    for o in &b.scenario.objectives {
        let holder = b.objective_holder(o).map_or("none", side_name);
        let _ = writeln!(s, "objective.{}={holder}", o.id);
    }
    s
}
