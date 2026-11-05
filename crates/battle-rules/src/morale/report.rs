//! The interface of a regiment's morale (SPEC-006 section 5.7 and ADR 0010 D-04): the coarse
//! indicator with its dwell, the ranked causes, the quantised margin, the trend and the nine
//! cause terms. The hidden cohesion and loss are never exposed.

use super::inputs::MoraleInputs;
use super::state::{officers_total, Indicator, RegimentMorale};
use super::table::MoraleKind;
use super::{symptoms::symptom_values, F_LOOKBACK};
use crate::{BattleState, RegimentId};

/// Steps of the margin band, permille.
pub const MARGIN_STEP_PM: u32 = 50;
/// The smoothed change of cohesion per second, ppm, from which the trend is not flat.
pub const TREND_BAND_PPM: i32 = 1000;
/// Cohesion at which a wavering regiment stops looking uneasy and looks shaken, ppm.
const UNEASY_FLOOR: u32 = 500_000;

/// What weighs on a regiment (SPEC-006 section 5.7), in tie-break order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Cause {
    /// Casualties, recent and lasting.
    Losses,
    /// Officers lost.
    Officers,
    /// Tired men.
    Fatigue,
    /// Short of ammunition.
    Ammunition,
    /// Short of supply.
    Supply,
    /// Enemies on the flank, along the line or behind.
    Flank,
    /// An enemy charging home.
    Charge,
    /// Neighbours falling back or breaking.
    Neighbours,
    /// Broken ground.
    Terrain,
}

impl Cause {
    /// Every cause in tie-break order; the index of a cause is its position here.
    pub const ALL: [Cause; 9] = [
        Cause::Losses,
        Cause::Officers,
        Cause::Fatigue,
        Cause::Ammunition,
        Cause::Supply,
        Cause::Flank,
        Cause::Charge,
        Cause::Neighbours,
        Cause::Terrain,
    ];
}

/// What the player may know of a regiment's morale. It has no cohesion and no loss.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MoraleReport {
    /// The coarse state.
    pub indicator: Indicator,
    /// The two largest causes at least `cause_min`, largest first.
    pub causes: [Option<Cause>; 2],
    /// Loading rate now, permille.
    pub fire_pm: u16,
    /// Aim-error factor now, permille.
    pub shake_pm: u16,
    /// Soldiers in the roster looking back.
    pub looking: u16,
    /// Soldiers who have left the ranks.
    pub absent: u16,
    /// Permille of the starting roster that is dead, wounded, disabled or absent.
    pub thinning_pm: u16,
    /// All nine cause magnitudes in permille of the training base ceiling, in [`Cause::ALL`]
    /// order.
    pub terms: [u16; 9],
    /// Distance above the next worse indicator level, permille, in steps of 50.
    pub margin_pm: u16,
    /// `+1` regaining cohesion, `-1` losing it, `0` steady.
    pub trend: i8,
}

/// Memory kept only for the report: nothing in the rules reads it and it is not hashed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReportMemo {
    /// Cohesion lost to neighbours lately, ppm, decaying by an eighth each second.
    pub nbr: u32,
    /// Cohesion at the previous slow period, ppm.
    pub c_prev: u32,
    /// Smoothed change of cohesion per second, ppm.
    pub slope: i32,
}

/// The two largest of nine magnitudes (ppm) that reach `cause_min`, largest first, ties by
/// [`Cause::ALL`] order.
pub fn rank_causes(mag: &[u32; 9], cause_min: u32) -> [Option<Cause>; 2] {
    let mut order: Vec<usize> = (0..9).filter(|&i| mag[i] >= cause_min).collect();
    order.sort_by(|&a, &b| mag[b].cmp(&mag[a]).then(a.cmp(&b)));
    let pick = |k: usize| order.get(k).map(|&i| Cause::ALL[i]);
    [pick(0), pick(1)]
}

/// The indicator a state and cohesion call for.
fn indicator_for(kind: MoraleKind, c: u32) -> Indicator {
    match kind {
        MoraleKind::Steady => Indicator::Firm,
        MoraleKind::Wavering if c >= UNEASY_FLOOR => Indicator::Uneasy,
        MoraleKind::Wavering => Indicator::Shaken,
        MoraleKind::FallingBack => Indicator::Retiring,
        MoraleKind::Broken => Indicator::Routed,
        MoraleKind::Rallied => Indicator::Rallying,
    }
}

/// Severity of an indicator: higher is worse. Rallying lies between Uneasy and Shaken.
const fn severity(i: Indicator) -> u8 {
    match i {
        Indicator::Firm => 0,
        Indicator::Uneasy => 1,
        Indicator::Rallying => 2,
        Indicator::Shaken => 3,
        Indicator::Retiring => 4,
        Indicator::Routed => 5,
    }
}

/// Nine magnitudes in ppm in [`Cause::ALL`] order.
fn magnitudes(rm: &RegimentMorale, inputs: &MoraleInputs, w_officer_pm: u32) -> [u32; 9] {
    let lost = officers_total(usize::from(rm.s0)).saturating_sub(rm.officers.len()) as u64;
    let officers = lost * u64::from(w_officer_pm) * u64::from(rm.shock_unit) / 1000;
    [
        rm.window.saturating_add(rm.loss),
        u32::try_from(officers).unwrap_or(u32::MAX),
        inputs.fatigue,
        inputs.ammo,
        inputs.supply,
        inputs.flank,
        inputs.charge,
        rm.memo.nbr,
        inputs.disorder,
    ]
}

impl BattleState {
    /// The report of regiment `id`; `None` while morale is off or for an unknown regiment.
    pub fn morale_report(&self, id: RegimentId) -> Option<MoraleReport> {
        let ms = self.morale.as_ref()?;
        let rm = ms.regs.get(usize::from(id.0))?;
        let reg = self.regiments.get(usize::from(id.0))?;
        let inputs = self.morale_inputs(id)?;
        let g = &self.tables.morale.globals;
        let base = u64::from(self.tables.morale.training(reg.training).base_ppm).max(1);
        let mag = magnitudes(rm, &inputs, g.w_officer_pm);
        let mut terms = [0u16; 9];
        for (t, m) in terms.iter_mut().zip(mag) {
            *t = u16::try_from(u64::from(m) * 1000 / base).unwrap_or(u16::MAX);
        }
        let sym = symptom_values(&self.tables.morale, rm.kind, rm.c);
        let looking = reg
            .roster
            .iter()
            .filter(|a| self.world.agents.flags(**a).unwrap_or(0) & F_LOOKBACK != 0)
            .count();
        let s0 = u64::from(rm.s0).max(1);
        let off_roster = s0.saturating_sub(reg.roster.len() as u64);
        let at = |x: u32| i64::from(x) + i64::from(rm.nerve_offset);
        let line = match rm.indicator {
            Indicator::Firm => Some(at(g.waver)),
            Indicator::Uneasy => Some(i64::from(UNEASY_FLOOR)),
            Indicator::Shaken | Indicator::Rallying => Some(at(g.fall)),
            Indicator::Retiring => Some(at(g.break_)),
            Indicator::Routed => None,
        };
        let margin = line.map_or(0, |l| (i64::from(rm.c) - l).max(0) / 1000);
        let step = i64::from(MARGIN_STEP_PM);
        let trend = match rm.memo.slope {
            s if s >= TREND_BAND_PPM => 1,
            s if s <= -TREND_BAND_PPM => -1,
            _ => 0,
        };
        let short = |v: u64| u16::try_from(v).unwrap_or(u16::MAX);
        Some(MoraleReport {
            indicator: rm.indicator,
            causes: rank_causes(&mag, g.cause_min),
            fire_pm: short(u64::from(sym.fire_pm)),
            shake_pm: short(u64::from(sym.shake_pm)),
            looking: short(looking as u64),
            absent: short(rm.absent.len() as u64),
            thinning_pm: short((off_roster * 1000 / s0).min(1000)),
            terms,
            margin_pm: short(u64::try_from(margin / step * step).unwrap_or(0)),
            trend,
        })
    }

    /// Recomputes every regiment's indicator after a tick: a worse one is shown at once, a better
    /// one once `dwell` ticks have passed since the last change.
    pub(crate) fn update_indicators(&mut self) {
        let dwell = self.tables.morale.globals.dwell;
        let Some(ms) = self.morale.as_mut() else {
            return;
        };
        for rm in &mut ms.regs {
            let want = indicator_for(rm.kind, rm.c);
            let (now, next) = (severity(rm.indicator), severity(want));
            if next > now {
                rm.indicator = want;
                rm.dwell = 0;
                continue;
            }
            rm.dwell = rm.dwell.saturating_add(1);
            if next < now && u32::from(rm.dwell) >= dwell {
                rm.indicator = want;
                rm.dwell = 0;
            }
        }
    }

    /// Slow-period bookkeeping of the report memory: decays the neighbour shock by an eighth and
    /// adds what the neighbour step took from each regiment (`before` is the cohesion ahead of
    /// it, by regiment).
    pub(crate) fn note_neighbour_loss(&mut self, before: &[u32]) {
        let Some(ms) = self.morale.as_mut() else {
            return;
        };
        for (rm, c0) in ms.regs.iter_mut().zip(before) {
            let lost = c0.saturating_sub(rm.c);
            rm.memo.nbr = (rm.memo.nbr - rm.memo.nbr / 8).saturating_add(lost);
        }
    }

    /// Slow-period bookkeeping of the trend: `s = s - s / 4 + (c - c_prev)`.
    pub(crate) fn note_trend(&mut self) {
        let Some(ms) = self.morale.as_mut() else {
            return;
        };
        for rm in &mut ms.regs {
            let step = i64::from(rm.c) - i64::from(rm.memo.c_prev);
            let s = i64::from(rm.memo.slope);
            rm.memo.slope =
                i32::try_from((s - s / 4 + step).clamp(-1_000_000, 1_000_000)).unwrap_or(0);
            rm.memo.c_prev = rm.c;
        }
    }
}
