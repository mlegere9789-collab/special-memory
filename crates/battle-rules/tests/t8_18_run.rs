//! T8-18 (SPEC-008 sections 8, 11): `Battle::run` and manual stepping give equal final hashes.
//! Class 0 only for now: the class 1 and class 2 cases join when F1 and F2 fire land (T-085,
//! T-086); the scheduler cases below (hand-overs by classification) already run both ways.

mod t084_support;

use battle_rules::{Battle, F0, F1, F2};
use t084_support::{battle, four, scenario_text, trigger, U};

fn scenario(fc: u8) -> String {
    let extra = [
        trigger("fire_b", 1, "b1", "fire_mode", "target = \"g1\""),
        trigger("fire_g", 1, "g1", "fire_mode", "target = \"b1\""),
        trigger("ammo", 6, "g2", "resupply", "rounds = 5"),
    ]
    .concat();
    scenario_text(fc, 60, &four(), &extra)
}

fn manual(mut b: Battle, ticks: u64) -> Battle {
    for _ in 0..ticks {
        b.step();
    }
    b
}

fn run(mut b: Battle, ticks: u64) -> Battle {
    b.run(ticks);
    b
}

#[test]
fn run_equals_manual_stepping_in_class_0() {
    for seed in [1, 2, 3] {
        let a = manual(battle(&scenario(0), seed), 400);
        let b = run(battle(&scenario(0), seed), 400);
        assert_eq!(a.hash(), b.hash(), "seed {seed}");
        assert_eq!(a.state.world.tick, 400);
        assert_eq!(b.state.world.tick, 400);
        assert!(a.state.counters().shots_fired > 0, "the fight happened");
        assert_eq!(a.state.counters().unit_level_rolls, 0);
    }
}

#[test]
fn run_in_pieces_equals_one_run() {
    let mut a = battle(&scenario(0), 9);
    a.run(150);
    a.run(150); // no change: already there
    a.run(399);
    let b = run(battle(&scenario(0), 9), 399);
    assert_eq!(a.hash(), b.hash());
    // Running to a tick in the past does nothing.
    let mut c = run(battle(&scenario(0), 9), 399);
    c.run(10);
    assert_eq!(c.hash(), b.hash());
}

#[test]
fn run_equals_manual_stepping_across_hand_overs() {
    // A unit that leaves the fight and is far away is condensed, and one that arrives late
    // is materialised: the same path both ways.
    let units = [
        U::blue("b", 20, 0),
        U::grey("g", 140, 0),
        U::grey("late", 100, 3),
    ];
    for fc in [0, 2] {
        let text = scenario_text(fc, 60, &units, "");
        let a = manual(battle(&text, 4), 600);
        let b = run(battle(&text, 4), 600);
        assert_eq!(a.hash(), b.hash(), "fc {fc}");
    }
}

#[test]
fn the_scenario_class_is_a_floor_for_every_regiment() {
    let b = run(battle(&scenario(2), 1), 100);
    assert!(b.state.regiments.iter().all(|r| r.class == F2));
    assert_eq!(b.state.world.agents.iter_alive().count(), 0);
    let b = run(battle(&scenario(1), 1), 100);
    assert!(b
        .state
        .regiments
        .iter()
        .all(|r| r.class == F1 || r.class == F2));
    assert!(b.state.regiments.iter().all(|r| r.class != F0));
}

#[test]
fn soldiers_and_rounds_balance_after_a_fight() {
    let b = run(battle(&scenario(0), 5), 600);
    for team in 0..2u8 {
        let l = b.ledger(team);
        assert_eq!(l.arrived, l.ledger.present + l.ledger.killed + l.departed);
        let c = l.ledger;
        assert_eq!(c.held + c.spent + c.lost, c.initial + c.resupplied);
    }
    assert!(b.ledger(0).ledger.spent > 0);
}
