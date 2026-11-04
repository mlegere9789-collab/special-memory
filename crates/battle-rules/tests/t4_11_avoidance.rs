//! T4-11: avoidance. Coincident soldiers separate to personal space within 40 ticks; two
//! 100-man files marching through each other end at least 0.8 of personal space apart after
//! settling; `desired_velocity` is pure (SPEC-004 section 13 and Amendment A2).

mod t018_support;

use battle_rules::{BattleState, Formation, Gait, Order, RegimentSpec, Training};
use core_math::{Angle, Fx, Vec2Fx};
use sim_state::AgentId;
use t018_support::empty_battle;

fn pt(x: i64, y: i64) -> Vec2Fx {
    Vec2Fx::new(
        battle_rules::fx_ratio(x, 1000),
        battle_rules::fx_ratio(y, 1000),
    )
}

fn space(b: &BattleState) -> Fx {
    battle_rules::fx_ratio(
        i64::from(b.tables.locomotion.globals().personal_space_mm),
        1000,
    )
}

fn lone(anchor: Vec2Fx) -> RegimentSpec {
    RegimentSpec {
        team: 1,
        count: 1,
        training: Training::Trained,
        formation: Formation::Line,
        anchor,
        heading: Angle::ZERO,
    }
}

/// Two soldiers whose slots are 600 mm apart, started on the same spot.
fn coincident_pair() -> (BattleState, [AgentId; 2]) {
    let mut b = empty_battle(1);
    let a = b.spawn_regiment(lone(pt(10_000, 10_000)));
    let c = b.spawn_regiment(lone(pt(10_000, 10_600)));
    let ids = [a, c].map(|r| b.regiments[usize::from(r.0)].roster[0]);
    b.world
        .agents
        .set_pos(ids[1], b.world.agents.pos(ids[0]).unwrap());
    (b, ids)
}

fn gap(b: &BattleState, ids: [AgentId; 2]) -> Fx {
    (b.world.agents.pos(ids[0]).unwrap() - b.world.agents.pos(ids[1]).unwrap()).length()
}

#[test]
fn coincident_soldiers_separate_within_40_ticks() {
    let (mut b, ids) = coincident_pair();
    b.grid.rebuild(&b.world.agents);
    let push = [0, 1].map(|i| b.desired_velocity(ids[i]));
    assert!(push[0] != Vec2Fx::ZERO && push[1] != Vec2Fx::ZERO);
    let mut first = None;
    for tick in 1..=40 {
        b.step_movement();
        if first.is_none() && gap(&b, ids) >= space(&b) {
            first = Some(tick);
        }
    }
    println!("coincident pair reached personal space at tick {first:?}");
    assert!(first.is_some());
    assert!(gap(&b, ids) >= space(&b));
}

/// Two single files of 100 men on one line, marching in opposite directions and ending
/// interleaved 450 mm apart (half a rank gap).
fn files() -> BattleState {
    let mut b = empty_battle(1);
    b.tables.drill.formations[Formation::Column.tag() as usize].cols = Some(1);
    let spec = |team, anchor, heading| RegimentSpec {
        team,
        count: 100,
        training: Training::Trained,
        formation: Formation::Column,
        anchor,
        heading,
    };
    let east = b.spawn_regiment(spec(0, pt(100_000, 200_000), Angle::ZERO));
    let west = b.spawn_regiment(spec(1, pt(250_000, 200_000), Angle::HALF));
    let order = |x| Order::Move {
        to: pt(x, 200_000),
        gait: Gait::Quick,
    };
    b.issue_order(east, order(160_000));
    b.issue_order(west, order(100_150));
    b
}

fn closest_pair(b: &BattleState, a: usize, c: usize) -> Fx {
    let pos = |r: usize| -> Vec<Vec2Fx> {
        b.regiments[r]
            .roster
            .iter()
            .map(|i| b.world.agents.pos(*i).unwrap())
            .collect()
    };
    let (pa, pc) = (pos(a), pos(c));
    pa.iter()
        .flat_map(|p| pc.iter().map(move |q| (*p - *q).length()))
        .min()
        .unwrap()
}

#[test]
fn files_marching_through_each_other_end_apart() {
    let mut b = files();
    let (mut during, mut purity_checked) = (Fx::MAX, false);
    let mut held_for = 0;
    while held_for < 400 {
        assert!(b.world.tick < 20_000, "files never arrive");
        b.step_movement();
        during = during.min(closest_pair(&b, 0, 1));
        let holding = b.regiments.iter().all(|r| r.order == Order::Hold);
        held_for = if holding { held_for + 1 } else { 0 };
        if !purity_checked && closest_pair(&b, 0, 1) < space(&b) {
            purity_checked = true;
            b.grid.rebuild(&b.world.agents);
            let ids = b.world.agents.ids_alive();
            let up: Vec<Vec2Fx> = ids.iter().map(|i| b.desired_velocity(*i)).collect();
            let mut down: Vec<Vec2Fx> = ids.iter().rev().map(|i| b.desired_velocity(*i)).collect();
            down.reverse();
            assert_eq!(up, down);
        }
    }
    let after = closest_pair(&b, 0, 1);
    println!("closest cross pair: {during:?} during the pass, {after:?} after settling");
    assert!(purity_checked, "the files never came within personal space");
    let floor = battle_rules::fx_ratio(
        i64::from(b.tables.locomotion.globals().personal_space_mm) * 8,
        10_000,
    );
    assert!(after >= floor, "{after:?} below 0.8 of personal space");
}

#[test]
fn desired_velocity_is_zero_for_strangers() {
    let (b, _) = coincident_pair();
    let stranger = AgentId {
        index: 9999,
        generation: 0,
    };
    assert_eq!(b.desired_velocity(stranger), Vec2Fx::ZERO);
}
