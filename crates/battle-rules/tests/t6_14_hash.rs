//! T6-14, hash (SPEC-006 section 6): with morale off nothing is written, with it on every listed
//! field moves the hash.

mod t6_support;

use battle_rules::morale::{Indicator, Leader, MoraleEvent, MoraleEventKind, MoraleKind};
use battle_rules::{BattleState, RegimentId};
use core_math::Fx;
use sim_state::AgentId;
use t6_support::{field, line, m};

fn base() -> BattleState {
    let mut b = field(5, '.');
    line(&mut b, 0, 40, m(100, 100));
    line(&mut b, 1, 40, m(500, 100));
    b.enable_morale();
    b.set_leader(Leader {
        id: 3,
        team: 0,
        pos: m(100, 100),
        prestige_pm: 500,
        alive: true,
    });
    let ms = b.morale.as_mut().unwrap();
    ms.events.push(MoraleEvent {
        kind: MoraleEventKind::Fell,
        regiment: RegimentId(0),
        pos: m(100, 100),
    });
    ms.rally_points = vec![Some(m(10, 10)), None];
    b
}

fn changed(f: impl FnOnce(&mut BattleState)) -> bool {
    let a = base();
    let mut b = base();
    assert_eq!(a.hash(), b.hash());
    f(&mut b);
    a.hash() != b.hash()
}

fn reg(b: &mut BattleState) -> &mut battle_rules::morale::RegimentMorale {
    &mut b.morale.as_mut().unwrap().regs[1]
}

#[test]
fn morale_off_writes_nothing_and_on_writes_a_block() {
    let mut off = field(5, '.');
    line(&mut off, 0, 40, m(100, 100));
    line(&mut off, 1, 40, m(500, 100));
    let before = off.hash();
    off.step_morale();
    assert_eq!(off.hash(), before);
    off.enable_morale();
    assert_ne!(off.hash(), before);
    off.morale = None;
    assert_eq!(off.hash(), before);
}

#[test]
fn every_regiment_field_changes_the_hash() {
    assert!(changed(|b| reg(b).kind = MoraleKind::Rallied));
    assert!(changed(|b| reg(b).state_ticks += 1));
    assert!(changed(|b| reg(b).c += 1));
    assert!(changed(|b| reg(b).loss += 1));
    assert!(changed(|b| reg(b).window += 1));
    assert!(changed(|b| reg(b).nerve_offset += 1));
    assert!(changed(|b| reg(b).broken_count += 1));
    assert!(changed(|b| reg(b).s0 += 1));
    assert!(changed(|b| reg(b).rounds_full += 1));
    assert!(changed(|b| reg(b).supply_pm -= 1));
    assert!(changed(|b| reg(b).leader_pm += 1));
    assert!(changed(|b| reg(b).colonel.index += 1));
    assert!(changed(|b| {
        reg(b).officers.pop();
    }));
    assert!(changed(|b| reg(b).officers[0].index += 1));
    assert!(changed(|b| reg(b).absent.push(AgentId {
        index: 7,
        generation: 0
    })));
    assert!(changed(|b| {
        reg(b).rng.next_u64();
    }));
    assert!(changed(|b| reg(b).indicator = Indicator::Routed));
    assert!(changed(|b| reg(b).dwell += 1));
    // A change in the first regiment is seen as well.
    assert!(changed(|b| b.morale.as_mut().unwrap().regs[0].c -= 1));
}

#[test]
fn leader_cursor_event_and_rally_fields_change_the_hash() {
    assert!(changed(|b| b.morale.as_mut().unwrap().cursor += 1));
    let one = |f: fn(&mut Leader)| {
        changed(|b| {
            f(&mut b.morale.as_mut().unwrap().leaders[0]);
        })
    };
    assert!(one(|l| l.id += 1));
    assert!(one(|l| l.team += 1));
    assert!(one(|l| l.alive = false));
    assert!(one(|l| l.pos.x += Fx::from_int(1)));
    assert!(one(|l| l.pos.y += Fx::from_int(1)));
    assert!(one(|l| l.prestige_pm += 1));
    assert!(changed(|b| {
        b.set_leader(Leader {
            id: 4,
            team: 1,
            pos: m(0, 0),
            prestige_pm: 0,
            alive: true,
        });
    }));
    let ev = |f: fn(&mut MoraleEvent)| {
        changed(|b| {
            f(&mut b.morale.as_mut().unwrap().events[0]);
        })
    };
    assert!(ev(|e| e.kind = MoraleEventKind::Broke));
    assert!(ev(|e| e.regiment = RegimentId(1)));
    assert!(ev(|e| e.pos.x += Fx::from_int(1)));
    assert!(ev(|e| e.pos.y += Fx::from_int(1)));
    assert!(changed(|b| b.morale.as_mut().unwrap().events.clear()));
    assert!(changed(
        |b| b.morale.as_mut().unwrap().rally_points[0] = None
    ));
    assert!(changed(
        |b| b.morale.as_mut().unwrap().rally_points[1] = Some(m(0, 0))
    ));
    assert!(changed(
        |b| b.morale.as_mut().unwrap().rally_points[0] = Some(m(11, 10))
    ));
    assert!(changed(
        |b| b.morale.as_mut().unwrap().rally_points[0] = Some(m(10, 11))
    ));
    assert!(changed(|b| b
        .morale
        .as_mut()
        .unwrap()
        .rally_points
        .push(None)));
}

#[test]
fn the_hash_is_reproducible_across_identical_runs() {
    let run = || {
        let mut b = base();
        for _ in 0..200 {
            b.step();
        }
        b.hash()
    };
    assert_eq!(run(), run());
}

#[test]
fn the_step_runs_morale_between_casualties_and_smoke() {
    let mut b = base();
    b.world.advance_tick();
    let who = b.regiments[0].roster[1];
    b.casualties.push(battle_rules::Casualty {
        tick: 1,
        target_index: who.index,
        unit: 0,
        team: 0,
        kind: battle_rules::CasualtyKind::Killed,
        shooter_unit: 1,
        ammo: 0,
    });
    let c = b.morale.as_ref().unwrap().regs[0].c;
    b.step();
    let ms = b.morale.as_ref().unwrap();
    assert_eq!(ms.cursor, 1);
    assert!(ms.regs[0].c < c);
}
