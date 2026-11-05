//! T-103: the setters and read accessors of SPEC-004 amendment A8 and SPEC-005 amendment A13.

mod t022_support;

use battle_rules::{FireMode, RegimentId, Training, FATIGUE_MAX};
use t022_support::{field, line, metres};

use core_math::Angle;

fn equipped(men: usize) -> battle_rules::BattleState {
    let mut b = field(3);
    let a = b.spawn_regiment(line(
        0,
        men,
        Training::Trained,
        metres(100, 100),
        Angle::ZERO,
    ));
    let e = b.spawn_regiment(line(
        1,
        men,
        Training::Trained,
        metres(300, 100),
        Angle::HALF,
    ));
    for id in [a, e] {
        b.equip_regiment(id, "rifle_58", 0).expect("weapon exists");
    }
    b
}

fn identity(b: &battle_rules::BattleState) {
    for team in [0u8, 1] {
        let l = b.ledger(team);
        assert_eq!(l.held + l.spent + l.lost, l.initial + l.resupplied);
    }
}

#[test]
fn fatigue_setter_and_reader() {
    let mut b = equipped(100);
    let id = RegimentId(0);
    assert_eq!(b.fatigue_pm(id), Some(0));
    assert!(b.set_fatigue(id, 250_000));
    assert_eq!(b.fatigue_pm(id), Some(250));
    assert!(b.set_fatigue(id, u32::MAX));
    assert_eq!(b.fatigue_pm(id), Some((FATIGUE_MAX / 1000) as u16));
    assert_eq!(
        b.fatigue_pm(RegimentId(1)),
        Some(0),
        "other regiments are untouched"
    );
    assert!(!b.set_fatigue(RegimentId(9), 1));
    assert_eq!(b.fatigue_pm(RegimentId(9)), None);
}

#[test]
fn strength_setter_keeps_the_first_soldiers_and_the_identity() {
    let mut b = equipped(100);
    let id = RegimentId(0);
    let first: Vec<_> = b.regiments[0].roster[..70].to_vec();
    assert_eq!(b.strength(id), Some(100));
    assert!(!b.set_strength(id, 101), "a strength is never raised");
    assert!(b.set_strength(id, 70));
    assert_eq!(b.strength(id), Some(70));
    assert_eq!(b.regiments[0].roster, first);
    assert!(b.regiments[0].dirty);
    assert_eq!(b.world.agents.len(), 170);
    identity(&b);
    assert_eq!(b.strength(RegimentId(1)), Some(100));
    assert!(!b.set_strength(RegimentId(9), 0));
    assert_eq!(b.strength(RegimentId(9)), None);
    b.step();
    assert_eq!(b.strength(id), Some(70));
}

#[test]
fn rounds_setter_moves_the_ledger_both_ways() {
    let mut b = equipped(100);
    let id = RegimentId(0);
    assert_eq!(b.rounds_pm(id), Some(1000));
    assert!(b.set_rounds_pm(id, 400));
    assert_eq!(b.rounds_pm(id), Some(400));
    let slot = b.regiments[0].roster[0].index as usize;
    assert_eq!(b.fire_ext(slot).map(|f| f.rounds), Some(24));
    identity(&b);
    assert!(b.set_rounds_pm(id, 5000), "clamped to a full load");
    assert_eq!(b.rounds_pm(id), Some(1000));
    identity(&b);
    assert!(b.ledger(0).resupplied > 0);
}

#[test]
fn rounds_accessors_refuse_unequipped_and_unknown() {
    let mut b = field(1);
    let id = b.spawn_regiment(line(0, 10, Training::Raw, metres(10, 10), Angle::ZERO));
    assert_eq!(b.rounds_pm(id), None);
    assert!(!b.set_rounds_pm(id, 500));
    assert_eq!(b.fire_ext(0), None);
    assert_eq!(b.fire_mode(id), Some(FireMode::HoldFire));
    assert_eq!(b.fire_mode(RegimentId(4)), None);
    assert_eq!(b.rounds_pm(RegimentId(4)), None);
}

#[test]
fn fire_mode_reads_back() {
    let mut b = equipped(10);
    let target = RegimentId(1);
    assert!(b.set_fire_mode(RegimentId(0), FireMode::AtWill { target }));
    assert_eq!(
        b.fire_mode(RegimentId(0)),
        Some(FireMode::AtWill { target })
    );
}

#[test]
fn setters_leave_the_hash_of_an_untouched_world_alone() {
    let mut a = equipped(50);
    let b = equipped(50);
    let before = a.hash();
    assert_eq!(before, b.hash());
    assert!(a.set_fatigue(RegimentId(0), 0));
    assert!(a.set_rounds_pm(RegimentId(0), 1000));
    assert_eq!(a.hash(), before, "setting the present value is a no-op");
}
