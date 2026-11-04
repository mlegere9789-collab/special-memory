//! T8-09: materialise, condense, materialise without fire loses nothing; soldiers stand on
//! their slots; remainders are distributed exactly; a regiment with a projectile in flight is
//! not condensed (SPEC-008 section 5, SPEC-004 amendment A4).

mod t083_support;

use battle_rules::{
    condense, hand_over, materialise, BattleState, CasualtyKind, HitOutcome, Projectile,
    RegimentId, F0, F1, F2, F_DISABLED, F_WOUNDED,
};
use core_math::{Fx, Vec2Fx};
use sim_state::AgentId;
use t083_support::{hit, member, six};

fn r0() -> RegimentId {
    RegimentId(0)
}

/// A battle after 60 ticks of fire, with one soldier dead, one wounded and two disabled in
/// regiment 0 (disabled at different ticks).
fn worn() -> BattleState {
    let mut b = six(11);
    for _ in 0..60 {
        b.step();
    }
    // Fatigue and fouling that do not divide evenly.
    for (k, a) in b.regiments[0].roster.clone().into_iter().enumerate() {
        b.loco[a.index as usize].fatigue = 12_345 + 7 * k as u32;
        b.fire[a.index as usize].fouling = 3 * (k % 13) as u16;
    }
    let (k, w, d1, d2) = (
        member(&b, 0, 0),
        member(&b, 0, 1),
        member(&b, 0, 2),
        member(&b, 0, 3),
    );
    hit(&mut b, k, HitOutcome::Killed);
    hit(&mut b, w, HitOutcome::Wounded);
    hit(&mut b, d1, HitOutcome::Disabled);
    b.step_casualties();
    b.world.advance_tick();
    b.world.advance_tick();
    hit(&mut b, d2, HitOutcome::Disabled);
    b.step_casualties();
    b
}

/// The live soldiers of regiment `r` in slot order of a layout for all of them.
fn in_slot_order(b: &BattleState, r: u16) -> Vec<AgentId> {
    let reg = &b.regiments[usize::from(r)];
    let all: Vec<AgentId> = b
        .world
        .agents
        .iter_alive()
        .filter(|&a| b.world.agents.unit(a) == Some(u32::from(r)))
        .collect();
    let layout =
        battle_rules::slot_layout(reg.state.target_formation(), all.len(), &b.tables.drill);
    layout
        .iter()
        .map(|s| {
            *all.iter()
                .find(|a| b.loco[a.index as usize].slot == *s)
                .unwrap()
        })
        .collect()
}

#[test]
fn condense_materialise_condense_is_the_identity_on_the_aggregate() {
    let mut b = worn();
    let dead = b
        .casualties()
        .iter()
        .filter(|c| c.unit == 0 && c.kind == CasualtyKind::Killed)
        .count() as u32;
    assert!(condense(&mut b, r0()));
    let first = b.regiments[0].aggregate.clone();
    assert_eq!(first.fit + first.wounded + 2 + dead, 60);
    assert!(first.wounded >= 1 && dead >= 1);
    assert_eq!(first.disabled.len(), 2);
    assert!(first.disabled[0].0 < first.disabled[1].0, "ascending until");
    assert!(first.rounds > 0 && first.fatigue_sum > 0 && first.fouling_sum > 0);
    assert!(b.regiments[0].roster.is_empty());
    for _ in 0..3 {
        assert!(materialise(&mut b, r0(), F0));
        let a = &b.regiments[0].aggregate;
        assert_eq!(
            (a.fit, a.rounds, a.fatigue_sum, a.fouling_sum),
            (0, 0, 0, 0)
        );
        assert!(a.disabled.is_empty());
        assert!(condense(&mut b, r0()));
        let again = &b.regiments[0].aggregate;
        assert_eq!(
            (
                again.fit,
                again.rounds,
                again.fatigue_sum,
                again.fouling_sum
            ),
            (
                first.fit,
                first.rounds,
                first.fatigue_sum,
                first.fouling_sum
            )
        );
        assert_eq!(again.disabled, first.disabled);
        assert_eq!((again.wounded, again.killed), (first.wounded, first.killed));
    }
}

#[test]
fn materialised_soldiers_stand_on_their_slots_and_keep_the_disabled_apart() {
    let mut b = worn();
    let tick = b.world.tick;
    assert!(condense(&mut b, r0()));
    let aggregate = b.regiments[0].aggregate.clone();
    assert!(materialise(&mut b, r0(), F1));
    let reg = &b.regiments[0];
    assert_eq!(reg.class, F1);
    assert_eq!(reg.since, tick);
    let fit = aggregate.fit as usize;
    assert_eq!(reg.roster.len(), fit);
    assert_eq!(
        reg.aggregate.wounded, aggregate.wounded,
        "the wounded stay counted"
    );
    let forward = Vec2Fx::from_angle(reg.heading);
    let right = Vec2Fx::from_angle(reg.heading - core_math::Angle::QUARTER);
    let all = in_slot_order(&b, 0);
    assert_eq!(all.len(), fit + 2);
    let layout = battle_rules::slot_layout(reg.state.target_formation(), fit + 2, &b.tables.drill);
    for (k, &a) in all.iter().enumerate() {
        let want = reg.anchor + right.scale(layout[k].u) + forward.scale(layout[k].v);
        assert_eq!(b.world.agents.pos(a), Some(want), "soldier {k} on its slot");
        assert_eq!(b.loco[a.index as usize].slot, layout[k]);
    }
    // The last two soldiers are the disabled ones, with their release ticks.
    for (a, &(until, n)) in all[fit..].iter().zip(&aggregate.disabled) {
        assert_eq!(n, 1);
        assert_ne!(b.world.agents.flags(*a).unwrap() & F_DISABLED, 0);
        assert_eq!(b.fire[a.index as usize].disabled_until, until);
        assert!(!reg.roster.contains(a));
    }
    for a in &reg.roster {
        assert_eq!(
            b.world.agents.flags(*a).unwrap() & (F_DISABLED | F_WOUNDED),
            0
        );
    }
    assert!(reg.dirty, "the close-up lays the roster out again");
}

#[test]
fn remainders_are_distributed_exactly_in_slot_order() {
    let mut b = six(2);
    for _ in 0..20 {
        b.step();
    }
    assert!(condense(&mut b, r0()));
    // 60 men: these sums do not divide evenly over them.
    let a = &mut b.regiments[0].aggregate;
    a.rounds = 60 * 10 + 7;
    a.fatigue_sum = 60 * 1000 + 59;
    a.fouling_sum = 60 * 20 + 3;
    assert!(materialise(&mut b, r0(), F0));
    let roster = in_slot_order(&b, 0);
    assert_eq!(roster.len(), 60);
    let rounds: Vec<u16> = roster
        .iter()
        .map(|a| b.fire[a.index as usize].rounds)
        .collect();
    let fatigue: Vec<u32> = roster
        .iter()
        .map(|a| b.loco[a.index as usize].fatigue)
        .collect();
    let fouling: Vec<u16> = roster
        .iter()
        .map(|a| b.fire[a.index as usize].fouling)
        .collect();
    for k in 0..60 {
        assert_eq!(rounds[k], 10 + u16::from(k < 7));
        assert_eq!(fatigue[k], 1000 + u32::from(k < 59));
        assert_eq!(fouling[k], 20 + u16::from(k < 3));
    }
    assert_eq!(rounds.iter().map(|&x| u64::from(x)).sum::<u64>(), 607);
}

#[test]
fn a_projectile_in_flight_blocks_the_condense() {
    let mut b = six(4);
    b.pool.push(Projectile {
        seq: 0,
        owner: member(&b, 0, 0).index,
        unit: 0,
        team: 0,
        ammo: 0,
        pos: Vec2Fx::ZERO,
        z: Fx::ZERO,
        vel: Vec2Fx::ZERO,
        vz: Fx::ZERO,
        travelled_mm: 0,
        bounces: 0,
        hits: 0,
        fuse: -1,
        seed: 1,
    });
    let before = b.regiments[0].clone();
    assert!(!condense(&mut b, r0()));
    assert!(!hand_over(&mut b, r0(), F2));
    assert_eq!(b.regiments[0], before);
    assert_eq!(b.world.agents.len(), 360);
    // A regiment with nothing of its own in flight is condensed.
    assert!(condense(&mut b, RegimentId(1)));
    b.pool.clear();
    assert!(condense(&mut b, r0()));
}

#[test]
fn refused_requests_change_nothing() {
    let mut b = six(6);
    assert!(!materialise(&mut b, r0(), F0), "not an aggregate");
    assert!(!condense(&mut b, RegimentId(40)));
    assert!(condense(&mut b, r0()));
    assert!(!condense(&mut b, r0()), "already class 2");
    assert!(
        !materialise(&mut b, r0(), F2),
        "class 2 is not a materialised class"
    );
    assert!(materialise(&mut b, r0(), F0));
    assert_eq!(b.soldier_serial, 60);
}
