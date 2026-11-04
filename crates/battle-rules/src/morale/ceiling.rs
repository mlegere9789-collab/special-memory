//! The ceiling a regiment's cohesion relaxes towards and the relaxation itself (SPEC-006
//! section 5.3), as pure functions plus the slow-period pass over a battle.

use super::inputs::MoraleInputs;
use super::state::officers_total;
use super::table::{MoraleGlobals, MoraleKind, MoraleTrainingRow};
use crate::BattleState;

const PPM: i64 = 1_000_000;

/// The ceiling `T`: `base - repeat_pen * min(broken_count, 3) - penalties + bonuses`, clamped to
/// `[0, 10^6]`.
pub fn ceiling_target(
    g: &MoraleGlobals,
    base_ppm: u32,
    broken_count: u8,
    inputs: &MoraleInputs,
) -> u32 {
    let t = i64::from(base_ppm)
        - i64::from(g.repeat_pen) * i64::from(broken_count.min(3))
        - i64::from(inputs.penalties())
        + i64::from(inputs.bonuses());
    u32::try_from(t.clamp(0, PPM)).unwrap_or(0)
}

/// The level cohesion relaxes towards: `max(0, T - loss)`, at most `broken_ceiling` while Broken.
pub fn effective_ceiling(g: &MoraleGlobals, target: u32, loss: u32, kind: MoraleKind) -> u32 {
    let tc = target.saturating_sub(loss);
    if kind == MoraleKind::Broken {
        tc.min(g.broken_ceiling)
    } else {
        tc
    }
}

/// The officer factor `500 + 500 * alive / total` in permille; 500 with no officers at all.
pub fn officer_factor_pm(alive: usize, total: usize) -> u32 {
    if total == 0 {
        return 500;
    }
    let alive = alive.min(total) as u64;
    u32::try_from(500 + 500 * alive / total as u64).unwrap_or(1000)
}

/// What drives recovery besides the training row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Drive {
    /// Officer factor in permille, see [`officer_factor_pm`].
    pub officer_pm: u32,
    /// Strength of the leader in reach, permille.
    pub leader_pm: u16,
    /// An enemy is within the pressure radius.
    pub pressure: bool,
}

/// One second of relaxation of cohesion `c` towards the ceiling `tc`. Above it, `down_pm` of the
/// gap is lost. Below it, `up_pm` of the gap (Steady, Wavering, Rallied) or `rally_pm` with the
/// leader gain (FallingBack, Broken) is recovered, scaled by the officer factor; under
/// pressure a FallingBack or Broken regiment recovers nothing. Divisions truncate toward zero.
pub fn relax_step(
    g: &MoraleGlobals,
    row: &MoraleTrainingRow,
    kind: MoraleKind,
    c: u32,
    tc: u32,
    drive: Drive,
) -> u32 {
    let gap = i64::from(tc) - i64::from(c);
    let routing = matches!(kind, MoraleKind::FallingBack | MoraleKind::Broken);
    let next = if gap < 0 {
        i64::from(c) + gap * i64::from(g.down_pm) / 1000
    } else if gap > 0 && !(drive.pressure && routing) {
        let o = i64::from(drive.officer_pm);
        let rate = if routing {
            let gain = 1000 + i64::from(g.rally_leader_pm) * i64::from(drive.leader_pm) / 1000;
            i64::from(row.rally_pm) * o / 1000 * gain / 1000
        } else {
            i64::from(row.up_pm) * o / 1000
        };
        i64::from(c) + gap * rate / 1000
    } else {
        i64::from(c)
    };
    u32::try_from(next.clamp(0, PPM)).unwrap_or(0)
}

/// One second of decay of the recent-shock window: `window - window / 8`.
pub fn decay_window(window: u32) -> u32 {
    window - window / 8
}

impl BattleState {
    /// One slow period of the input and ceiling work (SPEC-006 sections 5.2 and 5.3) for every
    /// regiment in ascending order: all inputs are sampled from the unchanged state first, then
    /// each regiment stores its leader strength, relaxes `c` and decays `window`. State changes
    /// and rolls belong to the later part of the period. Does nothing while morale is off.
    pub fn relax_morale(&mut self) {
        let Some(ms) = self.morale.as_ref() else {
            return;
        };
        let sampled: Vec<Option<MoraleInputs>> = (0..ms.regs.len())
            .map(|i| {
                u16::try_from(i)
                    .ok()
                    .and_then(|i| self.morale_inputs(crate::RegimentId(i)))
            })
            .collect();
        let g = &self.tables.morale.globals;
        let (regiments, morale) = (&self.regiments, self.morale.as_mut());
        let Some(ms) = morale else {
            return;
        };
        for (i, (rm, inputs)) in ms.regs.iter_mut().zip(sampled).enumerate() {
            let (Some(inputs), Some(reg)) = (inputs, regiments.get(i)) else {
                continue;
            };
            let row = self.tables.morale.training(reg.training);
            rm.leader_pm = inputs.leader_pm;
            let target = ceiling_target(g, row.base_ppm, rm.broken_count, &inputs);
            let tc = effective_ceiling(g, target, rm.loss, rm.kind);
            let o = officer_factor_pm(rm.officers.len(), officers_total(usize::from(rm.s0)));
            rm.c = relax_step(
                g,
                row,
                rm.kind,
                rm.c,
                tc,
                Drive {
                    officer_pm: o,
                    leader_pm: inputs.leader_pm,
                    pressure: inputs.pressure,
                },
            );
            rm.window = decay_window(rm.window);
        }
    }
}
