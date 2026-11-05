//! T6-08, neighbour events (SPEC-006 section 5.5): a hop takes one period, shocks add, and the
//! order in which the list is applied cannot matter.

mod t6_support;

use battle_rules::morale::{
    ceiling_target, effective_ceiling, MoraleEventKind, MoraleKind, SLOW_PERIOD_TICKS,
};
use battle_rules::{BattleState, Formation, Training};
use core_math::Angle;
use t6_support::{field, m, spawn};

/// Regiments 0 and 1 (team 0) are about to break; 2 (team 0) and 3 (team 1) stand in reach.
/// Every distance is 30 or 50 m, so the expected values are exact.
fn scene() -> BattleState {
    let mut b = field(11, '.');
    for (team, at) in [
        (0, m(100, 100)),
        (0, m(160, 100)),
        (0, m(130, 100)),
        (1, m(130, 140)),
    ] {
        spawn(
            &mut b,
            team,
            40,
            Training::Trained,
            Formation::Line,
            at,
            Angle::ZERO,
        );
    }
    b.enable_morale();
    b
}

fn ceiling(b: &BattleState, i: usize) -> u32 {
    let g = &b.tables.morale.globals;
    let rm = &b.morale.as_ref().unwrap().regs[i];
    let row = b.tables.morale.training(b.regiments[i].training);
    let inputs = b.morale_inputs(b.regiments[i].id).unwrap();
    let t = ceiling_target(g, row.base_ppm, rm.broken_count, &inputs);
    effective_ceiling(g, t, rm.loss, rm.kind)
}

fn pin_to_ceiling(b: &mut BattleState, i: usize) {
    let tc = ceiling(b, i);
    b.morale.as_mut().unwrap().regs[i].c = tc;
}

fn run_period(b: &mut BattleState, reverse: bool, hit: Option<bool>) {
    for _ in 0..SLOW_PERIOD_TICKS {
        b.step_morale_hooked(reverse, hit);
        b.world.advance_tick();
    }
}

/// Plays two periods: the first breaks regiments 0 and 1, the second applies their events.
fn play(reverse: bool) -> (BattleState, u32, u32) {
    let mut b = scene();
    for i in 0..2 {
        let rm = &mut b.morale.as_mut().unwrap().regs[i];
        rm.kind = MoraleKind::FallingBack;
        rm.c = 0;
    }
    pin_to_ceiling(&mut b, 2);
    pin_to_ceiling(&mut b, 3);
    run_period(&mut b, reverse, Some(true));
    let ms = b.morale.as_ref().unwrap();
    assert_eq!(ms.regs[0].kind, MoraleKind::Broken);
    assert_eq!(ms.regs[1].kind, MoraleKind::Broken);
    assert_eq!(ms.events.len(), 2);
    assert!(ms.events.iter().all(|e| e.kind == MoraleEventKind::Broke));
    assert_eq!(ms.events[0].regiment.0, 0);
    assert_eq!(ms.events[1].regiment.0, 1);
    // One hop takes a whole period: nothing has reached the neighbours yet.
    let (c2, c3) = (ms.regs[2].c, ms.regs[3].c);
    assert_eq!((c2, c3), (ceiling(&b, 2), ceiling(&b, 3)));
    run_period(&mut b, reverse, Some(false));
    (b, c2, c3)
}

#[test]
fn two_breaks_shock_a_third_by_the_sum() {
    let (b, c2, c3) = play(false);
    let g = &b.tables.morale.globals;
    // Each break is 30 m from regiment 2: (150 - 30) / 150 of nbr_shock, trained nbr_pm = 1000.
    let each = g.nbr_shock * 120 / 150;
    let ms = b.morale.as_ref().unwrap();
    assert_eq!(ms.regs[2].c, c2 - 2 * each);
    // The enemy at 50 m from each broken line gains (150 - 50) / 150 of the boost per event.
    let boost = g.enemy_boost * 100 / 150;
    assert_eq!(ms.regs[3].c, c3 + 2 * boost);
    // The events have been consumed.
    assert!(ms.events.is_empty());
}

#[test]
fn application_order_does_not_matter() {
    let (a, ..) = play(false);
    let (b, ..) = play(true);
    assert_eq!(a.hash(), b.hash());
    assert_eq!(a.morale, b.morale);
}

#[test]
fn a_fall_shocks_less_and_does_not_help_the_enemy() {
    let mut b = scene();
    let g = b.tables.morale.globals.clone();
    {
        let rm = &mut b.morale.as_mut().unwrap().regs[0];
        rm.kind = MoraleKind::Wavering;
        rm.c = 0;
    }
    pin_to_ceiling(&mut b, 2);
    pin_to_ceiling(&mut b, 3);
    let c2 = b.morale.as_ref().unwrap().regs[2].c;
    run_period(&mut b, false, Some(true));
    let ms = b.morale.as_ref().unwrap();
    assert_eq!(ms.events.len(), 1);
    assert_eq!(ms.events[0].kind, MoraleEventKind::Fell);
    // The same battle with the event dropped: the enemy's ceiling moves because the regiment
    // that fell back no longer fires (SPEC-006 section 5.2), so its cohesion is compared with
    // the run without the event rather than with its value before the period.
    let mut quiet = b.clone();
    quiet.morale.as_mut().unwrap().events.clear();
    run_period(&mut quiet, false, Some(false));
    run_period(&mut b, false, Some(false));
    let ms = b.morale.as_ref().unwrap();
    let full = g.nbr_shock * 120 / 150;
    assert_eq!(ms.regs[2].c, c2 - full * g.nbr_fall_pm / 1000);
    // The enemy receives nothing from the event.
    assert_eq!(ms.regs[3].c, quiet.morale.as_ref().unwrap().regs[3].c);
}

#[test]
fn shocks_and_boosts_stay_within_bounds() {
    let (mut b, ..) = play(false);
    // Hand the third regiment almost nothing and the enemy almost everything, then repeat.
    {
        let ms = b.morale.as_mut().unwrap();
        ms.regs[2].c = 10;
        ms.regs[3].c = 999_990;
        for i in 0..2 {
            ms.events.push(battle_rules::morale::MoraleEvent {
                kind: MoraleEventKind::Broke,
                regiment: battle_rules::RegimentId(i),
                pos: b.regiments[usize::from(i)].anchor,
            });
        }
    }
    run_period(&mut b, false, Some(false));
    let ms = b.morale.as_ref().unwrap();
    assert_eq!(ms.regs[2].c, 0);
    assert!(ms.regs[3].c <= 1_000_000);
}
