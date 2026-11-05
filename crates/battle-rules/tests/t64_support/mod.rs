//! Shared helpers of the SPEC-006 symptom, flight, report and fire tests (T-064). The test file
//! declares `mod t6_support;` first.
#![allow(dead_code)]

use crate::t6_support::{field, line, m};
use battle_rules::morale::{MoraleKind, RegimentMorale, SLOW_PERIOD_TICKS};
use battle_rules::{BattleState, Casualty, CasualtyKind};

/// One tick of the morale step alone, then the clock.
pub fn tick(b: &mut BattleState, hit: Option<bool>) {
    b.step_morale_hooked(false, hit);
    b.world.advance_tick();
}

/// One slow period (20 ticks); the slow work runs on its first tick.
pub fn period(b: &mut BattleState, hit: Option<bool>) {
    for _ in 0..SLOW_PERIOD_TICKS {
        tick(b, hit);
    }
}

pub fn rm(b: &BattleState, i: usize) -> &RegimentMorale {
    &b.morale.as_ref().unwrap().regs[i]
}

pub fn rm_mut(b: &mut BattleState, i: usize) -> &mut RegimentMorale {
    &mut b.morale.as_mut().unwrap().regs[i]
}

/// Pins regiment `i` to cohesion `c` and state `k`.
pub fn set(b: &mut BattleState, i: usize, c: u32, k: MoraleKind) {
    let r = rm_mut(b, i);
    r.c = c;
    r.kind = k;
}

pub fn kind(b: &BattleState, i: usize) -> MoraleKind {
    rm(b, i).kind
}

/// A casualty record for soldier slot `index` of regiment 0.
pub fn cas(b: &BattleState, index: u32, kind: CasualtyKind) -> Casualty {
    Casualty {
        tick: b.world.tick,
        target_index: index,
        unit: 0,
        team: 0,
        kind,
        shooter_unit: 1,
        ammo: 0,
    }
}

/// Regiment 0: `n` trained men (team 0) at (100, 100) facing east; with `gap_m` a second
/// regiment of 100 (team 1) that many metres east of it; both carry rifles at full rounds (so that
/// ammunition weighs nothing) and morale is enabled.
pub fn duel(seed: u64, n: usize, gap_m: Option<i32>) -> BattleState {
    let mut b = field(seed, '.');
    let a = line(&mut b, 0, n, m(100, 100));
    b.equip_regiment(a, "rifle_58", 0).unwrap();
    if let Some(g) = gap_m {
        let e = line(&mut b, 1, 100, m(100 + g, 100));
        b.equip_regiment(e, "rifle_58", 0).unwrap();
    }
    b.enable_morale();
    b
}
