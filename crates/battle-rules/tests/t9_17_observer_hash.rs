//! T9-17 (observers): every field of the observer list and of each team's sighting log changes the
//! hash when the environment is on, and none is hashed when it is off.

mod t022_support;
mod t9_support;

use battle_rules::{BattleState, Observer, RegimentId, Sighting, Training};
use core_math::{Angle, Fx, Vec2Fx};
use t022_support::{field, line, metres};
use t9_support::{env_table, weather};

fn base(on: bool) -> BattleState {
    let mut b = field(4);
    b.spawn_regiment(line(0, 6, Training::Trained, metres(100, 200), Angle::ZERO));
    b.spawn_regiment(line(1, 6, Training::Trained, metres(300, 200), Angle::HALF));
    if on {
        b.set_environment(env_table(), weather(&[(0, 700, 800, 0, 0, 0)]));
    }
    b
}

const OBS: Observer = Observer {
    id: 0x8001,
    team: 0,
    pos: Vec2Fx::ZERO,
    eye_mm: 1700,
    vision_mm: 3_000_000,
};

const SIGHTING: Sighting = Sighting {
    observer: 0x8001,
    enemy: RegimentId(1),
    tick: 40,
    men_seen: 6,
    quality_pm: 700,
    centroid: Vec2Fx::ZERO,
};

fn assert_all_distinct(mut seen: Vec<u64>, variants: Vec<BattleState>) {
    for c in variants {
        let h = c.hash();
        assert!(!seen.contains(&h), "a field is not hashed");
        seen.push(h);
    }
}

#[test]
fn each_observer_field_changes_the_hash() {
    let b = base(true);
    let mut with = b.clone();
    assert!(with.add_observer(OBS));
    let variants: Vec<Observer> = vec![
        Observer { id: 0x8002, ..OBS },
        Observer { team: 1, ..OBS },
        Observer {
            pos: Vec2Fx::new(Fx::from_raw(1), Fx::ZERO),
            ..OBS
        },
        Observer {
            pos: Vec2Fx::new(Fx::ZERO, Fx::from_raw(1)),
            ..OBS
        },
        Observer {
            eye_mm: 1701,
            ..OBS
        },
        Observer {
            vision_mm: 3_000_001,
            ..OBS
        },
    ];
    let states = variants
        .into_iter()
        .map(|o| {
            let mut c = b.clone();
            assert!(c.add_observer(o));
            c
        })
        .collect();
    assert_all_distinct(vec![b.hash(), with.hash()], states);
}

#[test]
fn a_second_observer_changes_the_hash() {
    let mut a = base(true);
    assert!(a.add_observer(OBS));
    let h = a.hash();
    assert!(a.add_observer(Observer { id: 0x8002, ..OBS }));
    assert_ne!(a.hash(), h);
}

#[test]
fn each_sighting_field_changes_the_hash() {
    let b = base(true);
    let mut with = b.clone();
    with.record_sighting(0, SIGHTING);
    let one = Fx::from_raw(1);
    let variants = vec![
        Sighting {
            observer: 0x8002,
            ..SIGHTING
        },
        Sighting {
            enemy: RegimentId(0),
            ..SIGHTING
        },
        Sighting {
            tick: 41,
            ..SIGHTING
        },
        Sighting {
            men_seen: 7,
            ..SIGHTING
        },
        Sighting {
            quality_pm: 701,
            ..SIGHTING
        },
        Sighting {
            centroid: Vec2Fx::new(one, Fx::ZERO),
            ..SIGHTING
        },
        Sighting {
            centroid: Vec2Fx::new(Fx::ZERO, one),
            ..SIGHTING
        },
    ];
    let mut states: Vec<BattleState> = variants
        .into_iter()
        .map(|s| {
            let mut c = b.clone();
            c.record_sighting(0, s);
            c
        })
        .collect();
    // The log of the other team is a different field.
    let mut other = b.clone();
    other.record_sighting(1, SIGHTING);
    states.push(other);
    assert_all_distinct(vec![b.hash(), with.hash()], states);
}

#[test]
fn the_log_keeps_one_slot_per_enemy_in_ascending_order() {
    let mut b = base(true);
    b.record_sighting(
        0,
        Sighting {
            enemy: RegimentId(5),
            ..SIGHTING
        },
    );
    b.record_sighting(
        0,
        Sighting {
            enemy: RegimentId(2),
            ..SIGHTING
        },
    );
    b.record_sighting(
        0,
        Sighting {
            enemy: RegimentId(5),
            tick: 99,
            ..SIGHTING
        },
    );
    let log = b.sightings(0);
    let ids: Vec<u16> = log.iter().map(|s| s.enemy.0).collect();
    assert_eq!(ids, [2, 5]);
    assert_eq!(log[1].tick, 99);
}

#[test]
fn with_the_environment_off_nothing_of_it_is_hashed() {
    let b = base(false);
    let h0 = b.hash();
    let mut c = b.clone();
    c.add_observer(OBS);
    c.record_sighting(0, SIGHTING);
    assert_eq!(c.hash(), h0);
}
