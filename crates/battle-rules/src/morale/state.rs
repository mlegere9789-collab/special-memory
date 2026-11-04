//! Morale state (SPEC-006 section 4): per-regiment cohesion, leaders and events, and the calls
//! that enable and feed it.

use super::table::MoraleKind;
use crate::registry::morale_stream;
use crate::{BattleState, RegimentId};
use core_math::{Rng, Vec2Fx};
use sim_state::AgentId;

/// The coarse state the player sees (SPEC-006 section 5.7).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Indicator {
    /// Holding well.
    Firm,
    /// Showing strain.
    Uneasy,
    /// Close to falling back.
    Shaken,
    /// Falling back in order.
    Retiring,
    /// Running.
    Routed,
    /// Recovering.
    Rallying,
}

impl Indicator {
    /// Stable numeric tag (hash format): firm 0, uneasy 1, shaken 2, retiring 3, routed 4,
    /// rallying 5.
    pub const fn tag(self) -> u8 {
        self as u8
    }
}

/// What a neighbour hears of a regiment's change of state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MoraleEventKind {
    /// A regiment broke.
    Broke,
    /// A regiment fell back.
    Fell,
}

/// A change of state that shocks the regiments around it one period later.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MoraleEvent {
    /// What happened.
    pub kind: MoraleEventKind,
    /// Who it happened to.
    pub regiment: RegimentId,
    /// Where, at the regiment's anchor.
    pub pos: Vec2Fx,
}

/// A commander on the field, set by the scenario until a command spec supplies them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Leader {
    /// Identity; the list is kept in ascending id order.
    pub id: u16,
    /// Side.
    pub team: u8,
    /// Position in metres.
    pub pos: Vec2Fx,
    /// Standing with the men, permille, `0..=1000`.
    pub prestige_pm: u16,
    /// Still on the field.
    pub alive: bool,
}

/// The morale of one regiment. Cohesion and loss are hidden from the player.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegimentMorale {
    /// Current state.
    pub kind: MoraleKind,
    /// Ticks since the state was entered (counted by casualty ingestion).
    pub state_ticks: u32,
    /// Cohesion, ppm.
    pub c: u32,
    /// Cumulative loss, ppm of ceiling permanently removed.
    pub loss: u32,
    /// Recent shock, ppm, decaying by an eighth each second.
    pub window: u32,
    /// Per-regiment offset added to every threshold, ppm.
    pub nerve_offset: i32,
    /// Times the regiment has broken, at most 255.
    pub broken_count: u8,
    /// Roster size when morale was enabled.
    pub s0: u16,
    /// Rounds the roster carried when morale was enabled.
    pub rounds_full: u32,
    /// Supply, permille, default 1000.
    pub supply_pm: u16,
    /// Strength of the leader in reach, permille, set by the slow period.
    pub leader_pm: u16,
    /// Officers in ascending id order, the colonel among them while alive.
    pub officers: Vec<AgentId>,
    /// The colonel; an index of `u32::MAX` marks a regiment that had no soldiers.
    pub colonel: AgentId,
    /// Soldiers that left the ranks, ascending.
    pub absent: Vec<AgentId>,
    /// The regiment's named stream.
    pub rng: Rng,
    /// What the player sees.
    pub indicator: Indicator,
    /// Ticks the indicator has been held.
    pub dwell: u16,
    /// Cohesion removed by one casualty of weight 1000 permille, ppm.
    pub shock_unit: u32,
}

/// Morale of a battle: one entry per regiment that existed when it was enabled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MoraleState {
    /// By regiment id.
    pub regs: Vec<RegimentMorale>,
    /// Leaders in ascending id.
    pub leaders: Vec<Leader>,
    /// Next entry of the casualty log to ingest.
    pub cursor: usize,
    /// Events recorded this period, applied at the next.
    pub events: Vec<MoraleEvent>,
    /// Where each team's broken regiments retire to, by team.
    pub rally_points: Vec<Option<Vec2Fx>>,
}

/// Officers in a regiment of `s0` soldiers: `max(2, s0 / 40)`, at most one per soldier.
pub fn officers_total(s0: usize) -> usize {
    (s0 / 40).max(2).min(s0)
}

impl BattleState {
    /// Turns morale on, once, after the regiments are spawned and equipped; a second call
    /// changes nothing. Per regiment: `s0` is the roster size; `shock_unit` is
    /// `ceil(10^9 / (s0 * cap_pm))`; cohesion starts at the training base; the officers are the
    /// roster entries at index `(2k + 1) * s0 / (2 * n_off)` for `k` in `0..n_off`
    /// (`n_off = max(2, s0 / 40)`), the first being the colonel; `rounds_full` is the roster's
    /// rounds; and `nerve_offset` is the one draw from the regiment's named stream.
    pub fn enable_morale(&mut self) {
        if self.morale.is_some() {
            return;
        }
        let g = &self.tables.morale.globals;
        let spread = i32::try_from(g.nerve_spread).unwrap_or(i32::MAX);
        let mut regs = Vec::with_capacity(self.regiments.len());
        for reg in &self.regiments {
            let row = self.tables.morale.training(reg.training);
            let s0 = reg.roster.len();
            let n_off = (s0 / 40).max(2);
            let mut officers: Vec<AgentId> = (0..n_off)
                .filter_map(|k| reg.roster.get((2 * k + 1) * s0 / (2 * n_off)).copied())
                .collect();
            let colonel = officers.first().copied().unwrap_or(AgentId {
                index: u32::MAX,
                generation: 0,
            });
            officers.sort_unstable();
            officers.dedup();
            let rounds_full = reg
                .roster
                .iter()
                .filter_map(|a| self.fire.get(a.index as usize))
                .map(|f| u32::from(f.rounds))
                .sum();
            let mut rng = self.world.rng.stream(morale_stream(reg.id.0));
            let nerve_offset = rng.range_i32(-spread, spread);
            regs.push(RegimentMorale {
                kind: MoraleKind::Steady,
                state_ticks: 0,
                c: row.base_ppm,
                loss: 0,
                window: 0,
                nerve_offset,
                broken_count: 0,
                s0: u16::try_from(s0).unwrap_or(u16::MAX),
                rounds_full,
                supply_pm: 1000,
                leader_pm: 0,
                officers,
                colonel,
                absent: Vec::new(),
                rng,
                indicator: Indicator::Firm,
                dwell: 0,
                shock_unit: super::shock_unit(u32::try_from(s0).unwrap_or(u32::MAX), row.cap_pm),
            });
        }
        self.morale = Some(MoraleState {
            regs,
            leaders: Vec::new(),
            cursor: 0,
            events: Vec::new(),
            rally_points: Vec::new(),
        });
    }

    /// Places or replaces a leader (by id, keeping the list ascending). `false` while morale is
    /// off.
    pub fn set_leader(&mut self, leader: Leader) -> bool {
        let Some(ms) = self.morale.as_mut() else {
            return false;
        };
        let mut leader = leader;
        leader.prestige_pm = leader.prestige_pm.min(1000);
        match ms.leaders.binary_search_by_key(&leader.id, |l| l.id) {
            Ok(i) => ms.leaders[i] = leader,
            Err(i) => ms.leaders.insert(i, leader),
        }
        true
    }

    /// Sets a regiment's supply in permille (at most 1000). `false` for an unknown regiment or
    /// while morale is off.
    pub fn set_supply(&mut self, id: RegimentId, supply_pm: u16) -> bool {
        match self
            .morale
            .as_mut()
            .and_then(|ms| ms.regs.get_mut(usize::from(id.0)))
        {
            Some(r) => {
                r.supply_pm = supply_pm.min(1000);
                true
            }
            None => false,
        }
    }
}
