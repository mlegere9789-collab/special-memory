//! What the men show and what the regiment does about it (SPEC-006 sections 5.5, 5.6 and 5.8):
//! the symptom values of a state, the per-soldier draws that make men look back or leave the
//! ranks, the return of absentees, and the flight orders of a regiment that falls back.

use super::state::RegimentMorale;
use super::table::{MoraleKind, MoraleTable};
use super::{lerp, F_LOOKBACK, F_SKULK};
use crate::{fx_ratio, BattleState, Gait, LocoExt, Order, Regiment, RegimentId, RegimentState};
use crate::{SlotLocal, F_DISABLED, F_WOUNDED};
use core_math::Vec2Fx;
use sim_state::{AgentId, AgentStore};

/// Draws are compared as `x % 10^6`.
const ROLL_MOD: u32 = 1_000_000;

/// The symptoms of one state at one cohesion (SPEC-006 section 3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SymptomValues {
    /// Loading progress per tick, permille.
    pub fire_pm: u32,
    /// Factor on the shooter's aim error, permille.
    pub shake_pm: u32,
    /// Chance per second that a soldier looks back, permille.
    pub look_pm: u32,
    /// Chance per second that a soldier leaves the ranks, ppm.
    pub skulk_ppm: u32,
}

/// The values of `kind` at cohesion `c`. Only `Wavering` depends on `c`: its fire rate, shake,
/// look-back and skulk rates ramp from the row's values at the `steady` threshold to the ramp's
/// end values at the `fall` threshold (skulking from `skulk_from_ppm`), by [`lerp`] on the raw
/// thresholds.
pub fn symptom_values(table: &MoraleTable, kind: MoraleKind, c: u32) -> SymptomValues {
    let row = table.symptom(kind);
    let (mut fire_pm, mut shake_pm) = (row.fire_pm, row.shake_pm);
    let (mut look_pm, mut skulk_ppm) = (row.look_pm, row.skulk_ppm);
    if let (MoraleKind::Wavering, Some(r)) = (kind, row.ramp) {
        let g = &table.globals;
        let (c, hi, lo) = (i64::from(c), i64::from(g.steady), i64::from(g.fall));
        let ramp = |v_hi: u32, v_lo: u32| {
            u32::try_from(lerp(c, hi, lo, i64::from(v_hi), i64::from(v_lo))).unwrap_or(v_hi)
        };
        fire_pm = ramp(row.fire_pm, r.fire_end_pm);
        shake_pm = ramp(row.shake_pm, r.shake_end_pm);
        look_pm = ramp(row.look_pm, r.look_end_pm);
        let from = i64::from(r.skulk_from_ppm);
        skulk_ppm = u32::try_from(lerp(
            c,
            from,
            lo,
            i64::from(row.skulk_ppm),
            i64::from(r.skulk_end_ppm),
        ))
        .unwrap_or(row.skulk_ppm);
    }
    SymptomValues {
        fire_pm,
        shake_pm,
        look_pm,
        skulk_ppm,
    }
}

/// Where a regiment of `regiments` retires to: its team's rally point if one is set, else the
/// anchor moved `flight_mm` directly away from the nearest enemy anchor (the first of equals),
/// or straight back along the heading when no enemy is on the field.
pub(crate) fn retreat_point(
    regiments: &[Regiment],
    rally: &[Option<Vec2Fx>],
    i: usize,
    flight_mm: i64,
) -> Vec2Fx {
    let reg = &regiments[i];
    if let Some(Some(p)) = rally.get(usize::from(reg.team)) {
        return *p;
    }
    let behind = || -Vec2Fx::from_angle(reg.heading);
    let nearest = regiments
        .iter()
        .filter(|e| e.team != reg.team)
        .min_by_key(|e| (e.anchor - reg.anchor).length_sq());
    let away = match nearest {
        Some(e) => {
            let d = (reg.anchor - e.anchor).normalize();
            if d == Vec2Fx::ZERO {
                behind()
            } else {
                d
            }
        }
        None => behind(),
    };
    reg.anchor + away.scale(fx_ratio(flight_mm, 1000))
}

/// The entry action of FallingBack and Broken (SPEC-006 section 5.5), and its recomputation:
/// the pending order is cancelled, a `Forming` countdown is capped at 20 ticks and the standing
/// order becomes a move to `retreat` at a quick step (falling back) or at a run (broken).
pub(crate) fn order_flight(reg: &mut Regiment, kind: MoraleKind, retreat: Vec2Fx) {
    reg.pending = None;
    if let RegimentState::Forming { ticks_left, .. } = &mut reg.state {
        *ticks_left = (*ticks_left).min(FORMING_CAP_TICKS);
    }
    let gait = if kind == MoraleKind::Broken {
        Gait::Run
    } else {
        Gait::Quick
    };
    reg.order = Order::Move { to: retreat, gait };
}

/// Longest a regiment keeps forming once it has started to fall back.
const FORMING_CAP_TICKS: u32 = 20;

/// Section 5.6 for one regiment: one `u64` draw per roster soldier in ascending order, using the
/// symptom values `sym`. The look-back flag is cleared on every one and set where the draw says
/// so (not on one who leaves); a soldier whose skulk draw hits, officers excepted, takes
/// `F_SKULK`, joins `absent`, is steered `skulk_rear_mm` behind the anchor and leaves the
/// roster, which becomes `dirty`.
pub(crate) fn draw_symptoms(
    agents: &mut AgentStore,
    loco: &mut [LocoExt],
    reg: &mut Regiment,
    rm: &mut RegimentMorale,
    sym: SymptomValues,
    skulk_rear_mm: i64,
) {
    let look_below = sym.look_pm.saturating_mul(1000);
    let mut gone: Vec<AgentId> = Vec::new();
    for &id in &reg.roster {
        let x = rm.rng.next_u64();
        let look = (x as u32) % ROLL_MOD < look_below;
        let skulk = ((x >> 32) as u32) % ROLL_MOD < sym.skulk_ppm;
        let leaves = skulk && rm.officers.binary_search(&id).is_err();
        let mut flags = agents.flags(id).unwrap_or(0) & !F_LOOKBACK;
        if leaves {
            flags |= F_SKULK;
            gone.push(id);
        } else if look {
            flags |= F_LOOKBACK;
        }
        agents.set_flags(id, flags);
    }
    if gone.is_empty() {
        return;
    }
    reg.roster.retain(|a| gone.binary_search(a).is_err());
    reg.dirty = true;
    for id in gone {
        if let Some(l) = loco.get_mut(id.index as usize) {
            l.slot = SlotLocal {
                u: l.slot.u,
                v: fx_ratio(-skulk_rear_mm, 1000),
            };
        }
        if let Err(at) = rm.absent.binary_search(&id) {
            rm.absent.insert(at, id);
        }
    }
}

/// Drops absentees that are dead or gone (SPEC-006 section 5.6).
pub(crate) fn drop_dead_absentees(agents: &AgentStore, rm: &mut RegimentMorale) {
    rm.absent.retain(|a| agents.is_alive(*a));
}

/// Entry action of Rallied (SPEC-006 section 5.5): every absentee, ascending, draws one `u32`
/// and returns if `roll % 1000 < return_pm`; a returner loses `F_SKULK` and, if fit, takes its
/// place in the ascending roster, which becomes `dirty`.
pub(crate) fn return_absentees(
    agents: &mut AgentStore,
    reg: &mut Regiment,
    rm: &mut RegimentMorale,
    return_pm: u32,
) {
    let waiting = std::mem::take(&mut rm.absent);
    for id in waiting {
        if rm.rng.next_u32() % 1000 >= return_pm {
            rm.absent.push(id);
            continue;
        }
        let flags = agents.flags(id).unwrap_or(0);
        agents.set_flags(id, flags & !F_SKULK);
        if flags & (F_DISABLED | F_WOUNDED) == 0 {
            if let Err(at) = reg.roster.binary_search(&id) {
                reg.roster.insert(at, id);
                reg.dirty = true;
            }
        }
    }
}

impl BattleState {
    /// Loading rate of a regiment's soldiers in permille (0 to 1000) from its morale state and
    /// cohesion; 1000 with morale off or for an unknown regiment.
    pub fn fire_pm(&self, id: RegimentId) -> u16 {
        self.symptoms_of(id).map_or(1000, |v| v.fire_pm as u16)
    }

    /// Factor on the aim error of a regiment's soldiers in permille; 1000 with morale off or for
    /// an unknown regiment.
    pub fn shake_pm(&self, id: RegimentId) -> u16 {
        self.symptoms_of(id).map_or(1000, |v| v.shake_pm as u16)
    }

    /// The symptom values of a regiment now; `None` with morale off or for an unknown regiment.
    pub(crate) fn symptoms_of(&self, id: RegimentId) -> Option<SymptomValues> {
        let rm = self.morale.as_ref()?.regs.get(usize::from(id.0))?;
        Some(symptom_values(&self.tables.morale, rm.kind, rm.c))
    }

    /// Sets where a team's regiments retire to when they fall back or break, or clears it
    /// (`None`); the retreat is then computed away from the nearest enemy. `false` while morale
    /// is off.
    pub fn set_rally_point(&mut self, team: u8, point: Option<Vec2Fx>) -> bool {
        let Some(ms) = self.morale.as_mut() else {
            return false;
        };
        let t = usize::from(team);
        if ms.rally_points.len() <= t {
            ms.rally_points.resize(t + 1, None);
        }
        ms.rally_points[t] = point;
        true
    }
}
