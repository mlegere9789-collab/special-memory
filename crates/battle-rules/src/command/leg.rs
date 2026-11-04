//! Courier legs (SPEC-007 section 5): distance, the eight-point terrain mean, travel time and the
//! per-leg draws for jitter, loss and capture.

use super::state::Fate;
use super::{CommandTable, Medium};
use crate::{fx_ratio, BattleState};
use core_math::{Fx, Rng, Vec2Fx};

/// `to_mm(Fx) = ((raw * 1000) >> 32)`.
pub(crate) fn to_mm(f: Fx) -> i64 {
    ((i128::from(f.raw()) * 1000) >> 32) as i64
}

/// Distance between two points in millimetres.
pub(crate) fn dist_mm(a: Vec2Fx, b: Vec2Fx) -> i64 {
    to_mm((b - a).length())
}

/// `scale32(w, n) = (w * n) >> 32`.
pub(crate) fn scale32(w: u32, n: u32) -> u64 {
    (u64::from(w) * u64::from(n)) >> 32
}

/// `hit(w, ppm) = w % 1_000_000 < ppm`.
pub(crate) fn hit(w: u32, ppm: u32) -> bool {
    w % 1_000_000 < ppm
}

fn ceil_div(n: i128, d: i128) -> i128 {
    (n + d - 1).div_euclid(d)
}

impl BattleState {
    /// Terrain speed along a leg, permille: the floor of the mean `speed_pm` at the eight points
    /// `from + (to - from) * (2i + 1) / 16`, never below `min_terrain_pm`.
    pub(crate) fn terrain_mean_pm(&self, table: &CommandTable, from: Vec2Fx, to: Vec2Fx) -> u32 {
        let d = to - from;
        let along = |v: Fx, k: i128| Fx::from_raw(((i128::from(v.raw()) * k) >> 4) as i64);
        let sum: u32 = (0..8)
            .map(|i| {
                let k = 2 * i + 1;
                let p = from + Vec2Fx::new(along(d.x, k), along(d.y, k));
                let class = self.tables.terrain.class(self.terrain.class_at(p));
                class.map_or(1000, |c| c.speed_pm)
            })
            .sum();
        (sum / 8).max(table.globals.min_terrain_pm)
    }

    /// Ticks a courier needs for one leg: `ceil(D * path_factor_pm * 20 / (speed_mm_s * m))`.
    pub(crate) fn leg_ticks(&self, table: &CommandTable, from: Vec2Fx, to: Vec2Fx) -> u64 {
        let speed = i128::from(table.media[usize::from(Medium::Courier.tag()) - 1].speed_mm_s);
        let m = i128::from(self.terrain_mean_pm(table, from, to));
        let d = i128::from(dist_mm(from, to));
        ceil_div(d * i128::from(table.globals.path_factor_pm) * 20, speed * m) as u64
    }

    /// Enemy agents of `side` within the exposure corridor of the segment `from..aim`.
    fn enemies_near(&self, table: &CommandTable, side: u8, from: Vec2Fx, aim: Vec2Fx) -> i128 {
        let half = fx_ratio(table.globals.exposure_halfwidth_mm, 1000);
        let mut found = Vec::new();
        self.grid.query_segment(from, aim, half, &mut found);
        let agents = &self.world.agents;
        found
            .iter()
            .filter(|&&a| agents.is_alive(a) && agents.team(a).is_some_and(|t| t != side))
            .count() as i128
    }

    /// The three draws of one leg, in fixed order (`w1` jitter, `w2` loss, `w3` capture split):
    /// returns the jitter in ticks and what became of the message. Captured needs an enemy
    /// agent near a courier's route.
    pub(crate) fn draw_leg(
        &self,
        table: &CommandTable,
        rng: &mut Rng,
        medium: Medium,
        side: u8,
        from: Vec2Fx,
        aim: Vec2Fx,
    ) -> (u64, Fate) {
        let (w1, w2, w3) = (rng.next_u32(), rng.next_u32(), rng.next_u32());
        let g = &table.globals;
        let row = &table.media[usize::from(medium.tag()) - 1];
        let jitter = scale32(w1, g.jitter_ticks + 1);
        let mut n = 0;
        let mut p = i128::from(row.loss_base_ppm);
        if medium == Medium::Courier {
            n = self.enemies_near(table, side, from, aim);
            let path_mm = i128::from(dist_mm(from, aim)) * i128::from(g.path_factor_pm) / 1000;
            p += i128::from(g.loss_ppm_per_km) * path_mm / 1_000_000;
            p += i128::from(g.exposure_ppm_per_enemy) * n.min(i128::from(g.exposure_cap));
            p = p.min(i128::from(g.loss_cap_ppm));
        }
        let fate = if !hit(w2, p as u32) {
            Fate::InFlight
        } else if n > 0 && w3 % 1000 < g.capture_share_pm {
            Fate::Captured
        } else {
            Fate::Lost
        };
        (jitter, fate)
    }
}
