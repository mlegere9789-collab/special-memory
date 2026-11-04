//! T7-11 (picture and omniscient parts): with no sighting the picture has no enemy contact;
//! `omniscient_view` lists every living enemy regiment at confidence 1000; visibility is read
//! through a trait (ADR 0010 D-03), so an oracle that blocks or dims changes what is seen.

mod t73_support;

use battle_rules::command::{CommandOptions, OfficerId, Visibility};
use core_math::Vec2Fx;
use t73_support::*;

struct Dim(u32);

impl Visibility for Dim {
    fn transmit_pm(&self, _observer: Vec2Fx, _target: Vec2Fx) -> u32 {
        self.0
    }
    fn visible(&self, _observer: Vec2Fx, _target: Vec2Fx) -> bool {
        self.0 > 0
    }
}

#[test]
fn no_sighting_means_no_enemy_contact() {
    let t = calm_table();
    let mut b = field(1, 20, metres(5000, 5000));
    for _ in 0..500 {
        b.step_command(&t);
        b.world.advance_tick();
    }
    for o in [1, 4, 5, 6] {
        let v = b.picture_view(&t, OfficerId(o)).unwrap();
        assert!(v.contacts.is_empty());
    }
}

#[test]
fn omniscient_view_lists_every_living_enemy_at_full_confidence() {
    let t = calm_table();
    let mut b = field(1, 20, metres(5000, 5000));
    b.cmd.as_mut().unwrap().opt[0] = CommandOptions {
        omniscient_view: true,
        instant_orders: false,
    };
    let v = b.picture_view(&t, OfficerId(1)).unwrap();
    assert_eq!(v.contacts.len(), 1);
    let c = &v.contacts[0];
    assert_eq!((c.confidence_pm, c.radius_mm, c.age_ticks), (1000, 0, 0));
    assert_eq!((c.strength, c.pos), (20, metres(5000, 5000)));
    // The other side did not ask for it, and the enemy cannot see the first side's regiments.
    assert!(b
        .picture_view(&t, OfficerId(10))
        .unwrap()
        .contacts
        .is_empty());
    // A regiment with no men left is not listed.
    b.regiments[2].roster.clear();
    assert!(b
        .picture_view(&t, OfficerId(1))
        .unwrap()
        .contacts
        .is_empty());
    assert!(b.picture_view(&t, OfficerId(99)).is_none());
}

#[test]
fn an_oracle_can_block_or_dim_what_is_seen() {
    let t = calm_table();
    let quality = |vis: &dyn Visibility| {
        let mut b = field(1, 20, metres(1010, 20));
        b.observe_with(&t, vis);
        let cmd = b.cmd.as_ref().unwrap();
        let pic = &cmd.pics[cmd.index_of(OfficerId(5)).unwrap()];
        pic.contacts.first().map(|c| c.quality_pm)
    };
    assert_eq!(quality(&Dim(1000)), Some(445));
    assert_eq!(quality(&Dim(500)), Some(222));
    assert_eq!(quality(&Dim(0)), None);
    // Dimmed below `min_quality_pm` nothing is seen.
    assert_eq!(quality(&Dim(300)), None);
}
