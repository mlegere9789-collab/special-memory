//! Aggregate regiments, hand-overs and the ledger (SPEC-008 section 5).
//!
//! A class 2 regiment keeps no soldiers in the agent store; its men are the counts and sums of
//! an [`Aggregate`]. [`condense`] folds a regiment's soldiers into one, [`materialise`] stands
//! them up again on the slots of the regiment's formation, and [`BattleState::ledger`] counts
//! soldiers and rounds so that a test can see that no hand-over loses either.

use crate::battle::{sync_roster, SPAWN_HP};
use crate::casualty::DISABLED_HP;
use crate::registry::STREAM_MATERIALISED_BASE;
use crate::{
    slot_layout, step_ticks, BattleState, CasualtyKind, FidelityClass, FireExt, RegimentId,
    WeaponClass, F2, F_DISABLED, F_GUN, F_WOUNDED,
};
use crate::{LocoExt, FATIGUE_MAX};
use core_math::{Angle, StreamId, Vec2Fx};
use sim_state::{AgentId, AgentInit};

/// Stream indices of materialised soldiers wrap inside the registry block `0x8000_0000..=0x8FFF_FFFF`.
const SERIAL_MASK: u32 = 0x0FFF_FFFF;

/// The soldiers of a regiment as numbers (SPEC-008 section 5).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Aggregate {
    /// Soldiers able to fight.
    pub fit: u32,
    /// `(until tick, count)` of soldiers down, ascending and distinct in `until`.
    pub disabled: Vec<(u64, u32)>,
    /// Soldiers out for the battle.
    pub wounded: u32,
    /// Soldiers killed while the regiment was an aggregate.
    pub killed: u32,
    /// Rounds held by all of them.
    pub rounds: u64,
    /// Sum of the soldiers' fatigue, ppm.
    pub fatigue_sum: u64,
    /// Sum of the soldiers' barrel fouling, permille.
    pub fouling_sum: u64,
    /// Fractional hits carried between exchanges, ppm.
    pub hit_carry_ppm: u32,
    /// Fractional shots carried between exchanges, ppm.
    pub shot_carry_ppm: u32,
}

impl Aggregate {
    /// Soldiers down but not out.
    pub fn disabled_count(&self) -> u64 {
        self.disabled.iter().map(|&(_, n)| u64::from(n)).sum()
    }

    /// Adds `n` soldiers down until `until`, keeping the list ascending and distinct.
    fn add_disabled(&mut self, until: u64, n: u32) {
        match self.disabled.binary_search_by_key(&until, |&(u, _)| u) {
            Ok(i) => self.disabled[i].1 += n,
            Err(i) => self.disabled.insert(i, (until, n)),
        }
    }
}

/// Soldiers and rounds of one team (SPEC-008 section 5, SPEC-005 amendment A7).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Ledger {
    /// Live agents plus the fit, disabled and wounded men of its aggregates.
    pub present: u64,
    /// `Killed` records of the casualty log.
    pub killed: u64,
    /// Rounds in the hands of live soldiers of equipped regiments and in aggregates.
    pub held: u64,
    /// Shots taken, misfires included.
    pub spent: u64,
    /// Rounds that left with killed soldiers or were replaced by a re-equipment.
    pub lost: u64,
    /// Rounds given at equipment.
    pub initial: u64,
    /// Rounds added by resupply.
    pub resupplied: u64,
}

/// The stream of the materialised soldier with serial `serial` (SPEC-005 amendment A7).
pub const fn materialised_stream(serial: u32) -> StreamId {
    StreamId(STREAM_MATERIALISED_BASE + (serial & SERIAL_MASK))
}

/// `sum / n`, plus one when `k < sum % n`.
fn share(sum: u64, n: u64, k: u64) -> u64 {
    sum / n + u64::from(k < sum % n)
}

impl BattleState {
    /// Soldiers and rounds of `team` (see [`Ledger`]).
    pub fn ledger(&self, team: u8) -> Ledger {
        let agents = &self.world.agents;
        let mut l = Ledger::default();
        for id in agents.iter_alive() {
            if agents.team(id) != Some(team) {
                continue;
            }
            l.present += 1;
            let equipped = agents
                .unit(id)
                .and_then(|u| self.regfire.get(u as usize))
                .is_some_and(|r| r.weapon.is_some());
            if let (true, Some(f)) = (equipped, self.fire.get(id.index as usize)) {
                l.held += u64::from(f.rounds);
            }
        }
        for reg in self.regiments.iter().filter(|r| r.team == team) {
            let a = &reg.aggregate;
            l.present += u64::from(a.fit) + a.disabled_count() + u64::from(a.wounded);
            l.held += a.rounds;
            l.spent += reg.acct.spent;
            l.lost += reg.acct.lost;
            l.initial += reg.acct.initial;
            l.resupplied += reg.acct.resupplied;
        }
        l.killed = self
            .casualties
            .iter()
            .filter(|c| c.team == team && c.kind == CasualtyKind::Killed)
            .count() as u64;
        l
    }
}

/// Folds the soldiers of a regiment into its [`Aggregate`] and despawns them in ascending slot
/// order; the regiment becomes class 2 with `since` the current tick. Returns `false` and
/// changes nothing for an unknown regiment, one already in class 2, or one that still owns a
/// pooled projectile (it stays as it is and the caller retries at the next boundary).
pub fn condense(state: &mut BattleState, id: RegimentId) -> bool {
    let ri = usize::from(id.0);
    let Some(reg) = state.regiments.get(ri) else {
        return false;
    };
    if reg.class == F2 || state.pool.iter().any(|p| p.unit == u32::from(id.0)) {
        return false;
    }
    sync_roster(&state.world.agents, &mut state.regiments[ri]);
    let equipped = state.regfire[ri].weapon.is_some();
    let tick = state.world.tick;
    let members: Vec<AgentId> = state
        .world
        .agents
        .iter_alive()
        .filter(|&a| state.world.agents.unit(a) == Some(u32::from(id.0)))
        .collect();
    let mut agg = std::mem::take(&mut state.regiments[ri].aggregate);
    for a in members {
        let flags = state.world.agents.flags(a).unwrap_or(0);
        let fire = if equipped {
            state.fire.get(a.index as usize).copied()
        } else {
            None
        };
        if flags & F_WOUNDED != 0 {
            agg.wounded += 1;
        } else if flags & F_DISABLED != 0 {
            agg.add_disabled(fire.map_or(tick, |f| f.disabled_until), 1);
        } else {
            agg.fit += 1;
        }
        if let Some(f) = fire {
            agg.rounds += u64::from(f.rounds);
            agg.fouling_sum += u64::from(f.fouling);
        }
        agg.fatigue_sum += state
            .loco
            .get(a.index as usize)
            .map_or(0, |l| u64::from(l.fatigue));
        state.world.agents.despawn(a);
    }
    let reg = &mut state.regiments[ri];
    reg.aggregate = agg;
    reg.roster.clear();
    reg.slots.clear();
    reg.dirty = false;
    reg.class = F2;
    reg.since = tick;
    true
}

/// Stands the men of an aggregate up on the slots of the regiment's formation as class `class`
/// (0 or 1) and empties the aggregate except for its wounded and killed counts. Sums are shared
/// exactly (`S / n`, one more for the first `S % n` soldiers in slot order); the last soldiers
/// in slot order are the disabled ones. Returns `false` and changes nothing for an unknown
/// regiment, one that is not class 2, or a `class` above 1.
pub fn materialise(state: &mut BattleState, id: RegimentId, class: FidelityClass) -> bool {
    let ri = usize::from(id.0);
    if class > FidelityClass(1) || state.regiments.get(ri).is_none_or(|r| r.class != F2) {
        return false;
    }
    let tick = state.world.tick;
    let (anchor, heading, team, training, formation) = {
        let r = &state.regiments[ri];
        (
            r.anchor,
            r.heading,
            r.team,
            r.training,
            r.state.target_formation(),
        )
    };
    let agg = std::mem::take(&mut state.regiments[ri].aggregate);
    let fit = agg.fit as usize;
    let down: Vec<u64> = agg
        .disabled
        .iter()
        .flat_map(|&(until, n)| std::iter::repeat_n(until, n as usize))
        .collect();
    let n = fit + down.len();
    let layout = slot_layout(formation, n, &state.tables.drill);
    let forward = Vec2Fx::from_angle(heading);
    let right = Vec2Fx::from_angle(heading - Angle::QUARTER);
    // Weapon facts, fixed for the whole regiment.
    let rf = state.regfire[ri];
    let weapon = rf.weapon.map(|w| {
        let row = &state.tables.weapons.rows()[usize::from(w)];
        let sel = row
            .ammo
            .iter()
            .position(|l| state.tables.ammo.index_of(&l.id) == Some(usize::from(rf.ammo_row)))
            .unwrap_or(0);
        (
            row.class == WeaponClass::FieldGun,
            sel as u8,
            step_ticks(row, 0, training, 0),
        )
    });
    let gun = weapon.is_some_and(|w| w.0);
    let mut roster = Vec::with_capacity(fit);
    for (k, slot) in layout.iter().enumerate() {
        let is_down = k >= fit;
        let mut flags = if gun { F_GUN } else { 0 };
        if is_down {
            flags |= F_DISABLED;
        }
        let agent = state.world.agents.spawn(AgentInit {
            pos: anchor + right.scale(slot.u) + forward.scale(slot.v),
            vel: Vec2Fx::ZERO,
            facing: heading,
            hp: if is_down { DISABLED_HP } else { SPAWN_HP },
            flags,
            team,
            unit: u32::from(id.0),
        });
        let slots = state.world.agents.slot_count();
        if state.loco.len() < slots {
            state.loco.resize(slots, LocoExt::default());
        }
        let k64 = k as u64;
        let fatigue = share(agg.fatigue_sum, n as u64, k64).min(u64::from(FATIGUE_MAX));
        state.loco[agent.index as usize] = LocoExt {
            fatigue: fatigue as u32,
            slot: *slot,
            ..LocoExt::default()
        };
        let serial = state.soldier_serial;
        state.soldier_serial = serial.wrapping_add(1);
        if weapon.is_some() || !state.fire.is_empty() {
            if state.fire.len() < slots {
                state.fire.resize(slots, FireExt::UNEQUIPPED);
            }
            state.fire[agent.index as usize] = match weapon {
                Some((_, ammo_sel, first)) => {
                    let mut rng = state.world.rng.stream(materialised_stream(serial));
                    let left = rng.range_u32(0, u32::from(first)) as u16;
                    FireExt {
                        rounds: share(agg.rounds, n as u64, k64).min(u64::from(u16::MAX)) as u16,
                        ammo_sel,
                        step: 0,
                        left,
                        fouling: share(agg.fouling_sum, n as u64, k64).min(1000) as u16,
                        mag: 0,
                        disabled_until: if is_down { down[k - fit] } else { 0 },
                        rng: rng.state(),
                    }
                }
                None => FireExt::UNEQUIPPED,
            };
        }
        if !is_down {
            roster.push(agent);
        }
    }
    roster.sort_unstable();
    let reg = &mut state.regiments[ri];
    reg.aggregate = Aggregate {
        wounded: agg.wounded,
        killed: agg.killed,
        ..Aggregate::default()
    };
    reg.dirty = n > fit;
    reg.slots = layout[..fit].to_vec();
    reg.roster = roster;
    reg.class = class;
    reg.since = tick;
    true
}

/// Moves a regiment to class `to`: [`condense`] to class 2, [`materialise`] from class 2, and for
/// a change between class 0 and class 1 only the `class` and `since` fields. Returns `false`
/// and changes nothing for an unknown regiment, no change of class, a class above 2, or a
/// refused condense.
pub fn hand_over(state: &mut BattleState, id: RegimentId, to: FidelityClass) -> bool {
    let tick = state.world.tick;
    let Some(reg) = state.regiments.get_mut(usize::from(id.0)) else {
        return false;
    };
    let from = reg.class;
    if from == to || to > F2 {
        return false;
    }
    match (from == F2, to == F2) {
        (false, true) => condense(state, id),
        (true, _) => materialise(state, id, to),
        _ => {
            reg.class = to;
            reg.since = tick;
            true
        }
    }
}
