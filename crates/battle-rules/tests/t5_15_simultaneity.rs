//! T5-15: resolution is independent of projectile order; one record per target and tick.

mod t023_support;

use battle_rules::hit::QueuedHit;
use battle_rules::{
    battle_hash, BattleState, CasualtyKind, FireMode, HitOutcome, RegimentId, Training, F_DISABLED,
    F_WOUNDED,
};
use core_math::Angle;
use t023_support::{field_of, m, man};

fn hit(target: sim_state::AgentId, outcome: HitOutcome, ticks: u32, seq: u64) -> QueuedHit {
    QueuedHit {
        target,
        outcome,
        disabled_ticks: ticks,
        seq,
        shooter_unit: seq as u32,
        ammo: seq as u8,
    }
}

#[test]
fn two_shooters_killing_one_target_give_one_record_with_the_lowest_seq() {
    let mut b = field_of(1, '.');
    let t = man(&mut b, 1, m(100, 200), 0);
    for h in [
        hit(t, HitOutcome::Killed, 0, 9),
        hit(t, HitOutcome::Wounded, 0, 1),
        hit(t, HitOutcome::Killed, 0, 4),
    ] {
        b.push_hit(h);
    }
    b.step_casualties();
    assert_eq!(b.casualties().len(), 1);
    let c = b.casualties()[0];
    assert_eq!(
        (c.kind, c.shooter_unit, c.ammo),
        (CasualtyKind::Killed, 4, 4)
    );
    assert_eq!((c.target_index, c.tick), (t.index, 0));
    assert!(!b.world.agents.is_alive(t));
}

#[test]
fn casualties_are_logged_in_ascending_target_index_and_applied_by_kind() {
    let mut b = field_of(1, '.');
    let ids: Vec<_> = (0..4)
        .map(|i| man(&mut b, 1, m(100 + 10 * i, 200), 0))
        .collect();
    b.world.tick = 50;
    b.push_hit(hit(ids[3], HitOutcome::Disabled, 700, 3));
    b.push_hit(hit(ids[1], HitOutcome::Wounded, 0, 2));
    b.push_hit(hit(ids[3], HitOutcome::Disabled, 900, 8));
    b.push_hit(hit(ids[2], HitOutcome::Killed, 0, 1));
    b.step_casualties();
    let kinds: Vec<_> = b
        .casualties()
        .iter()
        .map(|c| (c.target_index, c.kind))
        .collect();
    assert_eq!(
        kinds,
        vec![
            (ids[1].index, CasualtyKind::Wounded),
            (ids[2].index, CasualtyKind::Killed),
            (ids[3].index, CasualtyKind::Disabled)
        ]
    );
    let a = &b.world.agents;
    assert_eq!((a.flags(ids[1]), a.hp(ids[1])), (Some(F_WOUNDED), Some(25)));
    assert_eq!(
        (a.flags(ids[3]), a.hp(ids[3])),
        (Some(F_DISABLED), Some(60))
    );
    assert_eq!(a.flags(ids[0]), Some(0));
    assert_eq!(b.fire[ids[3].index as usize].disabled_until, 950);
}

#[test]
fn a_disabled_soldier_is_released_and_rejoins_the_roster() {
    let mut b = field_of(1, '.');
    let t = man(&mut b, 1, m(100, 200), 0);
    b.world.tick = 10;
    b.push_hit(hit(t, HitOutcome::Disabled, 600, 1));
    b.step_casualties();
    b.step_rosters();
    assert!(b.regiments[0].roster.is_empty());
    b.world.tick = 609;
    b.release_disables();
    assert_eq!(b.world.agents.flags(t), Some(F_DISABLED));
    b.world.tick = 610;
    b.release_disables();
    assert_eq!(b.world.agents.flags(t), Some(0));
    assert_eq!(b.regiments[0].roster, vec![t]);
    assert!(b.regiments[0].dirty);
}

#[test]
fn a_casualty_record_changes_the_hash_and_an_empty_log_does_not() {
    let mut b = field_of(1, '.');
    let t = man(&mut b, 1, m(100, 200), 0);
    let before = battle_hash(&b);
    b.step_casualties();
    assert_eq!(battle_hash(&b), before);
    b.push_hit(hit(t, HitOutcome::Wounded, 0, 1));
    b.step_casualties();
    assert_ne!(battle_hash(&b), before);
}

fn firefight(reverse: bool) -> (Vec<u64>, BattleState) {
    let mut b = field_of(7, '.');
    let spec = |team, at, heading| t023_support::line_spec(team, 60, at, heading);
    let a = b.spawn_regiment(spec(0, m(100, 200), Angle::ZERO));
    let c = b.spawn_regiment(spec(1, m(140, 200), Angle::HALF));
    for (r, o) in [(a, c), (c, a)] {
        b.equip_regiment(r, "rifle_58", 0).unwrap();
        assert!(b.set_fire_mode(r, FireMode::AtWill { target: o }));
    }
    let mut hashes = Vec::new();
    for _ in 0..900 {
        b.step_with(reverse);
        hashes.push(b.hash());
    }
    (hashes, b)
}

#[test]
fn resolving_the_pool_in_reverse_gives_identical_casualties_and_hash() {
    let (fwd, a) = firefight(false);
    let (rev, b) = firefight(true);
    assert!(!a.casualties().is_empty(), "the fight must cost men");
    assert_eq!(fwd, rev);
    assert_eq!(a.casualties(), b.casualties());
}

fn gun_duel(reverse: bool) -> (Vec<u64>, BattleState) {
    let (mut b, _, _) = t023_support::gun_duel(3, Training::Trained);
    let mut hashes = Vec::new();
    for _ in 0..1500 {
        b.step_with(reverse);
        hashes.push(b.hash());
    }
    (hashes, b)
}

#[test]
fn bursts_do_not_depend_on_resolution_order() {
    let (fwd, a) = gun_duel(false);
    let (rev, b) = gun_duel(true);
    assert!(a.counters().projectiles_spawned > a.counters().shots_fired);
    assert_eq!(fwd, rev);
    assert_eq!(a.casualties(), b.casualties());
    let _ = RegimentId(0);
}
