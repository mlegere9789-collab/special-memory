//! `step_morale` (SPEC-006 section 5, REGISTRY step TK-11): casualty ingestion every tick and,
//! on slow ticks, relaxation, neighbour events, the fixed rolls and the state transitions.

use super::inputs::within;
use super::state::{MoraleEvent, MoraleEventKind};
use super::symptoms::{
    draw_symptoms, drop_dead_absentees, order_flight, retreat_point, return_absentees,
    symptom_values,
};
use super::table::MoraleKind;
use super::transition::{next_kind, TransitionIn};
use super::SLOW_PERIOD_TICKS;
use crate::{BattleState, CasualtyKind, Order, RegimentId};

/// Roll granularity: a roll decides `hit` when `roll % 10^6 < delay`.
const ROLL_MOD: u32 = 1_000_000;

/// Ticks between recomputations of a flight order while a regiment stays in FallingBack or
/// Broken.
const FLIGHT_REFRESH: u32 = 100;

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
        if self.world.tick.is_multiple_of(SLOW_PERIOD_TICKS) {
            self.relax_morale();
            let before = self.morale_cohesion();
            self.apply_morale_events(reverse_events);
            self.note_neighbour_loss(&before);
            self.roll_and_transition(force_hit);
            self.note_trend();
        }
        self.update_indicators();
    }

    /// Every regiment's cohesion, by regiment.
    fn morale_cohesion(&self) -> Vec<u32> {
        self.morale
            .as_ref()
            .map_or_else(Vec::new, |ms| ms.regs.iter().map(|r| r.c).collect())
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

    /// Sections 5.4 to 5.6 for every regiment ascending: one `u32` roll, then one `u64` per roster
    /// member (the soldier symptoms of the state the period starts in), then at most one
    /// transition with its entry action (flight orders, or the return of absentees with one more
    /// `u32` each). New events wait for the next period.
    fn roll_and_transition(&mut self, force_hit: Option<bool>) {
        let BattleState {
            world,
            regiments,
            loco,
            tables,
            morale,
            ..
        } = self;
        let g = &tables.morale.globals;
        let pressure: Vec<bool> = regiments
            .iter()
            .map(|r| {
                regiments
                    .iter()
                    .any(|e| e.team != r.team && within(e.anchor, r.anchor, g.pressure_radius))
            })
            .collect();
        let Some(ms) = morale.as_mut() else {
            return;
        };
        let mut raised = Vec::new();
        for i in 0..ms.regs.len().min(regiments.len()) {
            let rm = &mut ms.regs[i];
            drop_dead_absentees(&world.agents, rm);
            let roll = rm.rng.next_u32();
            let sym = symptom_values(&tables.morale, rm.kind, rm.c);
            draw_symptoms(
                &mut world.agents,
                loco,
                &mut regiments[i],
                rm,
                sym,
                g.skulk_rear,
            );
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
            // The state the regiment is in after this period, and whether it has just entered it.
            let (now, entered) = match to {
                Some(k) => (k, true),
                None => (rm.kind, false),
            };
            let routing = matches!(now, MoraleKind::FallingBack | MoraleKind::Broken);
            let again =
                !entered && routing && rm.state_ticks > 0 && rm.state_ticks % FLIGHT_REFRESH == 0;
            if routing && (entered || again) {
                let retreat = retreat_point(regiments, &ms.rally_points, i, g.flight_distance);
                order_flight(&mut regiments[i], now, retreat);
            }
            let rm = &mut ms.regs[i];
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
                MoraleKind::Rallied => {
                    let row = tables.morale.training(regiments[i].training);
                    regiments[i].order = Order::Hold;
                    return_absentees(&mut world.agents, &mut regiments[i], rm, row.return_pm);
                    None
                }
                _ => None,
            };
            if let Some(kind) = kind {
                raised.push(MoraleEvent {
                    kind,
                    regiment: RegimentId(regiments[i].id.0),
                    pos: regiments[i].anchor,
                });
            }
        }
        ms.events.extend(raised);
    }
}
