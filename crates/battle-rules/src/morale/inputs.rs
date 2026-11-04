//! Slow-period inputs of a regiment's ceiling (SPEC-006 section 5.2), sampled from the frozen
//! state of the battle.

use super::helpers::{classify_exposure, Exposure};
use crate::{fx_ratio, BattleState, Formation, Order, Regiment, RegimentId, RegimentState};
use core_math::Vec2Fx;

/// The terms of one regiment's ceiling, ppm unless named. Penalties lower the ceiling and bonuses
/// raise it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MoraleInputs {
    /// Penalty from mean fatigue above the knee.
    pub fatigue: u32,
    /// Penalty from the ammunition share below the knee.
    pub ammo: u32,
    /// Penalty from short supply.
    pub supply: u32,
    /// Penalty from enemies on the flank, in enfilade or in the rear.
    pub flank: u32,
    /// Penalty from the strongest charge.
    pub charge: u32,
    /// Penalty from line disorder of the terrain.
    pub disorder: u32,
    /// Bonus from the cover of the terrain.
    pub cover: u32,
    /// Bonus from a wall ahead.
    pub wall: u32,
    /// Bonus from the leader in reach.
    pub leader: u32,
    /// An enemy anchor is within the pressure radius.
    pub pressure: bool,
    /// Prestige of the best living friendly leader in reach, permille.
    pub leader_pm: u16,
    /// The exposure class carrying the most enemy weight; `None` with no enemy in range.
    pub dominant: Option<Exposure>,
}

impl MoraleInputs {
    /// Sum of the penalties.
    pub fn penalties(&self) -> u32 {
        [
            self.fatigue,
            self.ammo,
            self.supply,
            self.flank,
            self.charge,
            self.disorder,
        ]
        .iter()
        .fold(0u32, |a, v| a.saturating_add(*v))
    }

    /// Sum of the bonuses.
    pub fn bonuses(&self) -> u32 {
        [self.cover, self.wall, self.leader]
            .iter()
            .fold(0u32, |a, v| a.saturating_add(*v))
    }
}

/// Raw `Fx` value of a length in millimetres.
pub(crate) fn raw_mm(mm: i64) -> i128 {
    i128::from(fx_ratio(mm, 1000).raw())
}

fn sub_raw(a: Vec2Fx, b: Vec2Fx) -> (i128, i128) {
    (
        i128::from(a.x.raw()) - i128::from(b.x.raw()),
        i128::from(a.y.raw()) - i128::from(b.y.raw()),
    )
}

/// Whether two points are within `mm` millimetres of each other.
pub(crate) fn within(a: Vec2Fx, b: Vec2Fx, mm: i64) -> bool {
    let (dx, dy) = sub_raw(a, b);
    let r = raw_mm(mm);
    dx * dx + dy * dy <= r * r
}

/// Whether a segment has a point within `reach_mm` of `p` that lies ahead of `p` along `fwd`.
fn wall_ahead(p: Vec2Fx, fwd: Vec2Fx, a: Vec2Fx, b: Vec2Fx, reach_mm: i64) -> bool {
    let (abx, aby) = sub_raw(b, a);
    let (apx, apy) = sub_raw(p, a);
    let mut den = abx * abx + aby * aby;
    let mut num = (apx * abx + apy * aby).clamp(0, den);
    // Scale the ratio down so the products below cannot overflow, whatever the segment length.
    let shift = (128 - den.leading_zeros()).saturating_sub(60);
    den >>= shift;
    num >>= shift;
    // Nearest point relative to `p`, in raw units.
    let (qx, qy) = if den == 0 {
        (-apx, -apy)
    } else {
        (abx * num / den - apx, aby * num / den - apy)
    };
    let r = raw_mm(reach_mm);
    let ahead = qx * i128::from(fwd.x.raw()) + qy * i128::from(fwd.y.raw()) >= 0;
    ahead && qx * qx + qy * qy <= r * r
}

fn unit_vectors(reg: &Regiment) -> (Vec2Fx, Vec2Fx) {
    (
        Vec2Fx::from_angle(reg.heading),
        Vec2Fx::from_angle(reg.heading - core_math::Angle::QUARTER),
    )
}

impl BattleState {
    /// The inputs of regiment `id` from the current state, or `None` for an unknown regiment or
    /// while morale is off. Read-only: a slow period samples every regiment before any write.
    pub fn morale_inputs(&self, id: RegimentId) -> Option<MoraleInputs> {
        let ms = self.morale.as_ref()?;
        let rm = ms.regs.get(usize::from(id.0))?;
        let reg = self.regiments.get(usize::from(id.0))?;
        let g = &self.tables.morale.globals;
        let mut out = MoraleInputs::default();
        let n = reg.roster.len() as u64;

        // Mean fatigue, floored.
        let fat_sum: u64 = reg
            .roster
            .iter()
            .filter_map(|a| self.loco.get(a.index as usize))
            .map(|l| u64::from(l.fatigue))
            .sum();
        let mean = fat_sum.checked_div(n).unwrap_or(0);
        let knee = u64::from(g.fatigue_knee);
        out.fatigue = narrow(
            u64::from(g.fatigue_pen) * mean.saturating_sub(knee) / (1_000_000 - knee).max(1),
        );

        // Ammunition share and supply.
        let rounds: u64 = reg
            .roster
            .iter()
            .filter_map(|a| self.fire.get(a.index as usize))
            .map(|f| u64::from(f.rounds))
            .sum();
        let share = if rm.rounds_full == 0 {
            0
        } else {
            1000 * rounds / u64::from(rm.rounds_full)
        };
        let ammo_knee = u64::from(g.ammo_knee_pm);
        out.ammo = narrow(u64::from(g.ammo_pen) * ammo_knee.saturating_sub(share) / ammo_knee);
        out.supply =
            narrow(u64::from(g.supply_pen) * u64::from(1000 - rm.supply_pm.min(1000)) / 1000);

        // Terrain at the anchor, and a wall ahead of it.
        if let Some(class) = self.tables.terrain.class(self.terrain.class_at(reg.anchor)) {
            out.cover = narrow(u64::from(g.cover_bonus) * u64::from(class.cover_pm) / 1000);
            out.disorder =
                narrow(u64::from(g.disorder_pen) * u64::from(class.line_disorder_pm) / 1000);
        }
        let (fwd, right) = unit_vectors(reg);
        if self
            .obstacles
            .segments
            .iter()
            .any(|o| o.height_mm >= 600 && wall_ahead(reg.anchor, fwd, o.a, o.b, g.wall_reach))
        {
            out.wall = g.wall_bonus;
        }

        // Enemies: exposure, pressure and charge.
        let (mut weight, mut weighted_f) = (0u64, 0u64);
        let mut by_class = [0u64; 4];
        let mut any = false;
        for enemy in self.regiments.iter().filter(|e| e.team != reg.team) {
            if within(enemy.anchor, reg.anchor, g.pressure_radius) {
                out.pressure = true;
            }
            if within(enemy.anchor, reg.anchor, g.threat_range) {
                let own = enemy.roster.len() as u64;
                let wt = if self.can_fire(enemy.id) {
                    own
                } else {
                    own / 4
                };
                let (dx, dy) = sub_raw(enemy.anchor, reg.anchor);
                let f = dx * i128::from(fwd.x.raw()) + dy * i128::from(fwd.y.raw());
                let r = dx * i128::from(right.x.raw()) + dy * i128::from(right.y.raw());
                let class = classify_exposure(f, r);
                let f_pm = match class {
                    Exposure::Front => 0,
                    Exposure::Flank => g.flank_f_pm,
                    Exposure::Enfilade => g.enfilade_f_pm,
                    Exposure::Rear => g.rear_f_pm,
                };
                weight += wt;
                weighted_f += wt * u64::from(f_pm);
                by_class[class as usize] += wt;
                any = true;
            }
            if within(enemy.anchor, reg.anchor, g.charge_range) && charging(enemy, reg) {
                let ratio = (1000 * enemy.roster.len() as u64 / n.max(1)).min(1000);
                out.charge = out
                    .charge
                    .max(narrow(u64::from(g.charge_pen) * ratio / 1000));
            }
        }
        if let Some(share_pm) = weighted_f.checked_div(weight) {
            out.flank = narrow(u64::from(g.flank_pen) * share_pm / 1000);
        }
        if any {
            // First class in enum order among those with the most weight.
            let best = by_class.iter().copied().max().unwrap_or(0);
            let at = by_class.iter().position(|w| *w == best).unwrap_or(0);
            out.dominant = Some(
                [
                    Exposure::Front,
                    Exposure::Flank,
                    Exposure::Enfilade,
                    Exposure::Rear,
                ][at],
            );
        }

        // The best living friendly leader in reach.
        out.leader_pm = ms
            .leaders
            .iter()
            .filter(|l| l.alive && l.team == reg.team && within(l.pos, reg.anchor, g.leader_radius))
            .map(|l| l.prestige_pm)
            .max()
            .unwrap_or(0);
        out.leader = narrow(u64::from(g.leader_bonus) * u64::from(out.leader_pm) / 1000);
        Some(out)
    }
}

/// An enemy in column or line under a move order, heading towards `reg`'s anchor.
fn charging(enemy: &Regiment, reg: &Regiment) -> bool {
    let steady = matches!(
        enemy.state,
        RegimentState::Steady(Formation::Line | Formation::Column)
    );
    let (dx, dy) = sub_raw(reg.anchor, enemy.anchor);
    let f = Vec2Fx::from_angle(enemy.heading);
    steady
        && matches!(enemy.order, Order::Move { .. })
        && dx * i128::from(f.x.raw()) + dy * i128::from(f.y.raw()) > 0
}

fn narrow(v: u64) -> u32 {
    u32::try_from(v).unwrap_or(u32::MAX)
}
