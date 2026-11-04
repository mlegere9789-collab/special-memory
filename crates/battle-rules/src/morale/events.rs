//! Neighbour events of SPEC-006 section 5.5: a regiment that falls back or breaks shocks the
//! friendly regiments around it and heartens the enemy ones, one period later. Every effect is
//! additive and summed before it is applied, so the order of the list cannot matter.

use super::inputs::raw_mm;
use super::state::{MoraleEvent, MoraleEventKind};
use super::table::MoraleGlobals;
use crate::BattleState;

const PPM: i64 = 1_000_000;

/// Floor of the square root of `n`.
fn isqrt(n: u128) -> u128 {
    if n < 2 {
        return n;
    }
    let mut x = 1u128 << (128 - n.leading_zeros()).div_ceil(2);
    loop {
        let y = (x + n / x) / 2;
        if y >= x {
            return x;
        }
        x = y;
    }
}

/// Cohesion a friendly neighbour at raw distance `d` loses to an event within radius `r`:
/// `nbr_shock * (r - d) / r * nbr_pm / 1000`, times `nbr_fall_pm / 1000` for a fall.
pub fn neighbour_shock(
    g: &MoraleGlobals,
    nbr_pm: u32,
    kind: MoraleEventKind,
    d: i128,
    r: i128,
) -> i64 {
    if r <= 0 || d >= r {
        return 0;
    }
    let near = i128::from(g.nbr_shock) * (r - d.max(0)) / r;
    let mut shock = i64::try_from(near).unwrap_or(0) * i64::from(nbr_pm) / 1000;
    if kind == MoraleEventKind::Fell {
        shock = shock * i64::from(g.nbr_fall_pm) / 1000;
    }
    shock
}

/// Cohesion an enemy neighbour gains from a Broke event: `enemy_boost * (r - d) / r`.
pub fn enemy_boost(g: &MoraleGlobals, d: i128, r: i128) -> i64 {
    if r <= 0 || d >= r {
        return 0;
    }
    i64::try_from(i128::from(g.enemy_boost) * (r - d.max(0)) / r).unwrap_or(0)
}

impl BattleState {
    /// Applies the events of the previous period (`reverse` walks the list backwards; the result
    /// is the same) and clears the list. Per regiment the effects are summed in `i64` and applied
    /// once, then `c` is clamped to `[0, 10^6]`.
    pub(crate) fn apply_morale_events(&mut self, reverse: bool) {
        let g = &self.tables.morale.globals;
        let Some(ms) = self.morale.as_mut() else {
            return;
        };
        if ms.events.is_empty() {
            return;
        }
        let events = std::mem::take(&mut ms.events);
        let r = raw_mm(g.nbr_radius);
        let mut delta = vec![0i64; ms.regs.len()];
        let mut apply = |ev: &MoraleEvent| {
            let Some(src) = self.regiments.get(usize::from(ev.regiment.0)) else {
                return;
            };
            for (j, reg) in self.regiments.iter().enumerate().take(delta.len()) {
                if j == usize::from(ev.regiment.0) {
                    continue;
                }
                let dx = i128::from(reg.anchor.x.raw()) - i128::from(ev.pos.x.raw());
                let dy = i128::from(reg.anchor.y.raw()) - i128::from(ev.pos.y.raw());
                let d = isqrt((dx * dx + dy * dy) as u128) as i128;
                if reg.team == src.team {
                    let row = self.tables.morale.training(reg.training);
                    delta[j] -= neighbour_shock(g, row.nbr_pm, ev.kind, d, r);
                } else if ev.kind == MoraleEventKind::Broke {
                    delta[j] += enemy_boost(g, d, r);
                }
            }
        };
        if reverse {
            events.iter().rev().for_each(&mut apply);
        } else {
            events.iter().for_each(&mut apply);
        }
        for (rm, d) in ms.regs.iter_mut().zip(delta) {
            rm.c = u32::try_from((i64::from(rm.c) + d).clamp(0, PPM)).unwrap_or(0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::isqrt;

    #[test]
    fn isqrt_is_the_floor() {
        for n in [0u128, 1, 2, 3, 4, 15, 16, 17, 1 << 100, (1 << 100) - 1] {
            let s = isqrt(n);
            assert!(s * s <= n && (s + 1) * (s + 1) > n, "{n}");
        }
    }
}
