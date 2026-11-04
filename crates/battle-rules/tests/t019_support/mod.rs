//! Shared helpers for the T-019 movement tests.
#![allow(dead_code)]

use battle_rules::{BattleState, Tables};
use core_math::{Angle, Fx, Vec2Fx};

pub const RULES_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/rules");

pub fn tables() -> Tables {
    Tables::load_dir(RULES_DIR).unwrap()
}

/// Millimetres as `Fx`.
pub fn mm(v: i64) -> Fx {
    battle_rules::fx_ratio(v, 1000)
}

/// Distance of every rostered soldier of regiment `r` from its slot, in ascending roster order.
pub fn slot_errors(b: &BattleState, r: usize) -> Vec<Fx> {
    let reg = &b.regiments[r];
    let forward = Vec2Fx::from_angle(reg.heading);
    let right = Vec2Fx::from_angle(reg.heading - Angle::QUARTER);
    reg.roster
        .iter()
        .map(|id| {
            let s = b.loco[id.index as usize].slot;
            let at = reg.anchor + right.scale(s.u) + forward.scale(s.v);
            (at - b.world.agents.pos(*id).unwrap()).length()
        })
        .collect()
}

pub fn max_slot_error(b: &BattleState, r: usize) -> Fx {
    slot_errors(b, r).into_iter().max().unwrap_or(Fx::ZERO)
}

/// One row of `qc/fixtures/ref_march.csv`: `(case, gait, distance_mm, ideal_ticks, model_ticks)`.
pub fn ref_rows() -> Vec<(String, String, i64, u64, u64)> {
    include_str!("../../../../qc/fixtures/ref_march.csv")
        .lines()
        .skip(1)
        .map(|l| {
            let f: Vec<&str> = l.split(',').collect();
            (
                f[0].to_string(),
                f[1].to_string(),
                f[2].parse().unwrap(),
                f[3].parse().unwrap(),
                f[4].parse().unwrap(),
            )
        })
        .collect()
}

/// `|a - b| * 100 <= pct * b`.
pub fn within_percent(a: u64, b: u64, pct: u64) -> bool {
    a.abs_diff(b) * 100 <= pct * b
}
