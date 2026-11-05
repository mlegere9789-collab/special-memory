//! The belief builder: which units are kept, noise on contact strengths, tracks of contacts.

mod scene;

use battle_ai::{build_belief, Contact, ContactId, Frame, OwnReport, UnitRef, BELIEF_MAX_SOLDIERS};
use battle_rules::{Formation, RegimentId};
use core_math::{Angle, Rng};
use scene::*;

fn rep(unit: u16, x: i32, y: i32, strength: u16) -> OwnReport {
    OwnReport {
        unit: UnitRef(unit),
        pos: m(x, y),
        heading: Angle::ZERO,
        formation: Formation::Line,
        strength,
        strength_ref: strength,
        rounds_pm: 1000,
        fatigue_pm: 0,
        cohesion_pm: None,
        age_ticks: 0,
    }
}

fn con(id: u32, x: i32, y: i32, strength: u16, seen: u64) -> Contact {
    Contact {
        id: ContactId(id),
        pos: m(x, y),
        strength_est: strength,
        formation_est: None,
        weapon_est: None,
        last_seen: seen,
        quality_pm: 800,
    }
}

#[test]
fn units_beyond_the_radius_are_left_out() {
    let t = tables();
    let mp = map(&t);
    let own = [rep(0, 100, 100, 100), rep(1, 100, 1900, 100)];
    let foes = [con(5, 300, 100, 100, 0), con(6, 2000, 100, 100, 0)];
    let f = Frame::of(&view(0, 0, (&own, &foes), &[], &mp, &t));
    let units: Vec<u16> = f.own.iter().map(|r| r.unit.0).collect();
    let ids: Vec<u32> = f.foes.iter().map(|x| x.contact.id.0).collect();
    assert_eq!((units, ids), (vec![0], vec![5]));
}

#[test]
fn the_farthest_go_first_and_ties_drop_the_higher_id() {
    let t = tables();
    let mp = map(&t);
    // Eight regiments of 500 at distances 10, 20, ... from the centre (200, 120); two share one.
    let own: Vec<OwnReport> = (0..8u16)
        .map(|i| rep(i, 200 + 10 * (i32::from(i.min(6)) + 1), 120, 500))
        .collect();
    let f = Frame::of(&view(0, 0, (&own, &[]), &[], &mp, &t));
    let kept: Vec<u16> = f.own.iter().map(|r| r.unit.0).collect();
    // Units 6 and 7 are equally far (70 m); both exceed the cap together, 7 goes first and 6 next
    // only if still needed: six units of 500 fit exactly.
    assert_eq!(kept, vec![0, 1, 2, 3, 4, 5]);
    let total: u32 = f.own.iter().map(|r| u32::from(r.strength)).sum();
    assert!(total <= BELIEF_MAX_SOLDIERS);
}

#[test]
fn contacts_and_own_units_compete_for_the_cap() {
    let t = tables();
    let mp = map(&t);
    let own = [rep(0, 205, 120, 2000)];
    let foes = [con(1, 230, 120, 900, 0), con(2, 260, 120, 900, 0)];
    let f = Frame::of(&view(0, 0, (&own, &foes), &[], &mp, &t));
    assert_eq!(f.own.len(), 1);
    assert_eq!(f.foes.len(), 1);
    assert_eq!(f.foes[0].contact.id, ContactId(1));
    assert_eq!(f.foe_index(ContactId(1)), Some(1));
    assert_eq!(f.foe_index(ContactId(2)), None);
    assert_eq!(f.own_index(UnitRef(0)), Some(0));
}

#[test]
fn a_track_comes_from_the_last_two_sightings() {
    let t = tables();
    let mp = map(&t);
    let own = [rep(0, 100, 100, 100)];
    let now = [con(7, 260, 100, 100, 200)];
    let before = [con(7, 280, 100, 100, 100)];
    let f = Frame::of(&view(0, 200, (&own, &now), &before, &mp, &t));
    // 20 m west in 100 ticks.
    assert_eq!(f.foes[0].vel.x.raw(), -(20i64 << 32) / 100);
    assert_eq!(f.foes[0].vel.y.raw(), 0);
    let none = Frame::of(&view(0, 200, (&own, &now), &[], &mp, &t));
    assert_eq!(none.foes[0].vel, core_math::Vec2Fx::ZERO);
}

#[test]
fn noise_perturbs_contact_strengths_only_and_repeats() {
    let t = tables();
    let mp = map(&t);
    let own = [rep(0, 100, 100, 100)];
    let foes = [con(1, 250, 100, 200, 0), con(2, 250, 140, 200, 0)];
    let v = view(0, 0, (&own, &foes), &[], &mp, &t);
    let strengths = |seed, noise| {
        let b = build_belief(&v, &mut Rng::from_seed(seed), noise);
        (0..3)
            .map(|i| b.strength(RegimentId(i)).unwrap())
            .collect::<Vec<_>>()
    };
    assert_eq!(strengths(1, 0), vec![100, 200, 200]);
    assert_eq!(strengths(1, 300), strengths(1, 300));
    let varied: Vec<_> = (1..=10).map(|s| strengths(s, 300)).collect();
    assert!(varied.iter().any(|s| s[1] != 200 || s[2] != 200));
    for s in &varied {
        assert_eq!(s[0], 100, "own strength is reported, not estimated");
        assert!(s[1] >= 1 && s[1] <= 200 * 28 / 10, "{s:?}");
    }
    assert_ne!(varied[0], varied[1]);
}

#[test]
fn reports_set_fatigue_rounds_and_strength() {
    let t = tables();
    let mp = map(&t);
    let mut r = rep(0, 100, 100, 70);
    r.fatigue_pm = 300;
    r.rounds_pm = 250;
    let own = [r];
    let b = build_belief(
        &view(0, 42, (&own, &[]), &[], &mp, &t),
        &mut Rng::from_seed(1),
        0,
    );
    let id = RegimentId(0);
    assert_eq!(b.world.tick, 42);
    assert_eq!(b.strength(id), Some(70));
    assert_eq!(b.fatigue_pm(id), Some(300));
    assert_eq!(b.rounds_pm(id), Some(250));
}
