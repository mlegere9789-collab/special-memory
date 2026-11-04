//! `step_morale` (SPEC-006 section 5, REGISTRY step TK-11): casualty ingestion every tick and,
//! on slow ticks, relaxation, neighbour events, the fixed rolls and the state transitions.

use super::inputs::within;
use super::state::{MoraleEvent, MoraleEventKind};
use super::table::MoraleKind;
use super::transition::{next_kind, TransitionIn};
use super::SLOW_PERIOD_TICKS;
use crate::{BattleState, CasualtyKind, RegimentId};

/// Roll granularity: a roll decides `hit` when `roll % 10^6 < delay`.
const ROLL_MOD: u32 = 1_000_000;

impl BattleState {
    /// Step TK-11. Does nothing while morale is off. Casualties from the log are ingested every
    /// tick; on ticks with `tick % 20 == 0` the slow period follows: inputs and relaxation, the
    /// events of the previous period, the rolls and the transitions, regiments ascending.
    pub fn step_morale(&mut self) {
        self.step_morale_hooked(false, None);
    }

    /// [`BattleState::step_morale`] with the neighbour events applied in reverse (the result is
    /// the same) and `hit` forced for every regiment (the rolls are still drawn). Test hook.
    #[doc(hidden)]
    pub fn step_morale_hooked(&mut self, reverse_events: bool, force_hit: Option<bool>) {
        if self.morale.is_none() {
            return;
        }
        self.ingest_casualties();
        if !self.world.tick.is_multiple_of(SLOW_PERIOD_TICKS) {
            return;
        }
        self.relax_morale();
        self.apply_morale_events(reverse_events);
        self.roll_and_transition(force_hit);
    }

    /// Section 5.1: every log entry from `cursor` on shocks its regiment; then every regiment's
    /// `state_ticks` advances by one.
    fn ingest_casualties(&mut self) {
        let g = &self.tables.morale.globals;
        let Some(ms) = self.morale.as_mut() else {
            return;
        };
        for c in self.casualties.iter().skip(ms.cursor) {
            let Some(rm) = ms.regs.get_mut(c.unit as usize) else {
                continue;
            };
            // An aggregate outcome (ADR 0010 D-07) names no soldier: kind weight only.
            let soldier = c.target_index != u32::MAX;
            if soldier && rm.absent.iter().any(|a| a.index == c.target_index) {
                continue;
            }
            let post = rm.officers.iter().position(|a| a.index == c.target_index);
            let w = match post {
                Some(_) if soldier && rm.colonel.index == c.target_index => g.w_colonel_pm,
                Some(_) if soldier => g.w_officer_pm,
                _ => match c.kind {
                    CasualtyKind::Killed => g.w_killed_pm,
                    CasualtyKind::Wounded => g.w_wounded_pm,
                    CasualtyKind::Disabled => g.w_disabled_pm,
                },
            };
            if let (true, Some(at)) = (soldier, post) {
                rm.officers.remove(at);
            }
            let mut shock = u64::from(rm.shock_unit) * u64::from(w) / 1000;
            if rm.leader_pm > 0 {
                let calm = u64::from(g.calm_pm) * u64::from(rm.leader_pm) / 1000;
                shock = shock * 1000u64.saturating_sub(calm) / 1000;
            }
            let shock = u32::try_from(shock).unwrap_or(u32::MAX);
            rm.c = rm.c.saturating_sub(shock);
            let perm = u64::from(shock) * u64::from(g.perm_pm) / 1000;
            rm.loss = u32::try_from((u64::from(rm.loss) + perm).min(1_000_000)).unwrap_or(0);
            rm.window = rm.window.saturating_add(shock);
        }
        ms.cursor = self.casualties.len();
        for rm in &mut ms.regs {
            rm.state_ticks = rm.state_ticks.saturating_add(1);
        }
    }

    /// Sections 5.4 and 5.5 for every regiment ascending: one `u32` roll, then one `u64` per
    /// roster member (consumed by the soldier symptoms), then at most one transition. New events
    /// wait for the next period.
    fn roll_and_transition(&mut self, force_hit: Option<bool>) {
        let g = &self.tables.morale.globals;
        let pressure: Vec<bool> = self
            .regiments
            .iter()
            .map(|r| {
                self.regiments
                    .iter()
                    .any(|e| e.team != r.team && within(e.anchor, r.anchor, g.pressure_radius))
            })
            .collect();
        let Some(ms) = self.morale.as_mut() else {
            return;
        };
        let mut raised = Vec::new();
        for (i, rm) in ms.regs.iter_mut().enumerate() {
            let Some(reg) = self.regiments.get(i) else {
                continue;
            };
            let roll = rm.rng.next_u32();
            for _ in 0..reg.roster.len() {
                rm.rng.next_u64();
            }
            let hit = force_hit.unwrap_or(roll % ROLL_MOD < g.delay);
            let to = next_kind(
                g,
                &TransitionIn {
                    kind: rm.kind,
                    c: rm.c,
                    state_ticks: rm.state_ticks,
                    nerve_offset: rm.nerve_offset,
                    hit,
                    pressure: pressure.get(i).copied().unwrap_or(false),
                    officers: !rm.officers.is_empty(),
                    leader_pm: rm.leader_pm,
                },
            );
            let Some(to) = to else {
                continue;
            };
            rm.kind = to;
            rm.state_ticks = 0;
            let kind = match to {
                MoraleKind::FallingBack => Some(MoraleEventKind::Fell),
                MoraleKind::Broken => {
                    rm.broken_count = rm.broken_count.saturating_add(1);
                    Some(MoraleEventKind::Broke)
                }
                _ => None,
            };
            if let Some(kind) = kind {
                raised.push(MoraleEvent {
                    kind,
                    regiment: RegimentId(reg.id.0),
                    pos: reg.anchor,
                });
            }
        }
        ms.events.extend(raised);
    }
}
