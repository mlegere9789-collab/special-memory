//! T8-17 (SPEC-008 sections 4, 5, 11): a unit with `at_s = 3600` is absent (no agents, in no
//! gap) before tick 72000 and present at it; a departure removes it and adds to `departed`; the
//! soldier identity `arrived == present + killed + departed` holds at every tick.

mod t084_support;

use battle_rules::{Battle, RegimentId, RegimentStatus, F0, F2};
use t084_support::{battle, departure, scenario_text, U};

fn identity(b: &Battle) {
    for team in 0..2u8 {
        let l = b.ledger(team);
        assert_eq!(
            l.arrived,
            l.ledger.present + l.ledger.killed + l.departed,
            "team {team} at tick {}: {l:?}",
            b.state.world.tick
        );
        let c = l.ledger;
        assert_eq!(c.held + c.spent + c.lost, c.initial + c.resupplied);
    }
}

fn agents_of(b: &Battle, reg: u16) -> usize {
    b.state
        .world
        .agents
        .iter_alive()
        .filter(|&a| b.state.world.agents.unit(a) == Some(u32::from(reg)))
        .count()
}

#[test]
fn a_unit_at_3600_seconds_is_absent_until_tick_72000() {
    let units = [U::blue("early", 50, 0), U::grey("late", 110, 3600)];
    let mut b = battle(&scenario_text(0, 3700, &units, ""), 1);
    let (early, late) = (RegimentId(0), RegimentId(1));
    while b.state.world.tick < 72_000 {
        let tick = b.state.world.tick;
        if tick.is_multiple_of(6000) || tick == 71_999 {
            let r = &b.state.regiments[1];
            assert_eq!(r.status, RegimentStatus::NotArrived, "tick {tick}");
            assert_eq!(agents_of(&b, late.0), 0);
            assert_eq!(b.ledger(1).arrived, 0);
            // Alone on the field the early unit has no gap: it is an aggregate.
            if tick > 0 {
                assert_eq!(b.state.regiments[0].class, F2, "tick {tick}");
            }
        }
        b.step();
        identity(&b);
    }
    // The boundary at tick 72000 runs in the next step.
    assert_eq!(b.state.regiments[1].status, RegimentStatus::NotArrived);
    b.step();
    identity(&b);
    let r = &b.state.regiments[usize::from(late.0)];
    assert_eq!(r.status, RegimentStatus::Present);
    assert_eq!(b.ledger(1).arrived, 120);
    assert_eq!(b.ledger(1).ledger.present, 120);
    // Now a gap exists and both are agent-backed.
    assert_eq!(b.state.regiments[usize::from(early.0)].class, F0);
    assert_eq!(r.class, F0);
    assert_eq!(agents_of(&b, late.0), 120);
    assert_eq!(agents_of(&b, early.0), 120);
}

#[test]
fn a_departure_removes_the_unit_and_counts_it() {
    let units = [U::blue("b", 50, 0), U::grey("g", 110, 0)];
    let extra = departure("g", 4);
    let mut b = battle(&scenario_text(0, 60, &units, &extra), 2);
    for _ in 0..80 {
        b.step();
        identity(&b);
    }
    assert_eq!(b.state.world.tick, 80);
    assert_eq!(b.state.regiments[1].status, RegimentStatus::Present);
    assert_eq!(agents_of(&b, 1), 120);
    b.step();
    identity(&b);
    assert_eq!(b.state.regiments[1].status, RegimentStatus::Departed);
    assert_eq!(agents_of(&b, 1), 0);
    let l = b.ledger(1);
    assert_eq!((l.arrived, l.departed, l.ledger.present), (120, 120, 0));
    assert_eq!(l.ledger.held, 0);
    // The remaining unit has no enemy: it condenses once the dwell has passed.
    for _ in 0..400 {
        b.step();
        identity(&b);
    }
    assert_eq!(b.state.regiments[0].class, F2);
    assert_eq!(b.ledger(0).ledger.present, 120);
    assert_eq!(b.ledger(0).departed, 0);
}

#[test]
fn a_departure_from_an_aggregate_counts_its_men() {
    let units = [U::blue("b", 50, 0), U::grey("g", 110, 0)];
    let extra = departure("b", 30);
    let mut b = battle(&scenario_text(2, 60, &units, &extra), 2);
    for _ in 0..700 {
        b.step();
        identity(&b);
    }
    assert_eq!(b.state.regiments[0].status, RegimentStatus::Departed);
    assert_eq!(b.ledger(0).departed, 120);
    assert_eq!(b.state.regiments[1].status, RegimentStatus::Present);
    assert_eq!(b.state.regiments[1].class, F2);
}
