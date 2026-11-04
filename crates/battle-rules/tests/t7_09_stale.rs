//! T7-09 (SPEC-007 sections 5 and 8): order A by a slow courier and a countermand B by telegraph
//! arriving first: A is discarded, the regiment follows B, and A's status is `Stale`.

mod t73_support;

use battle_rules::command::{
    CommandTable, Latitude, Link, Medium, ModKind, OfficerId, OrderStatus, Truth,
};
use battle_rules::{Gait, Order};
use t73_support::*;

/// No loss or garbling, and officers stay where `place` put them.
fn quiet() -> CommandTable {
    let mut t = calm_table();
    t.globals.pos_interval_ticks = 1_000_000;
    t
}

#[test]
fn a_countermand_by_telegraph_makes_the_earlier_courier_order_stale() {
    let t = quiet();
    let mut b = field(9, 1, metres(5000, 5000));
    place(&mut b, 4, 0, 0);
    place(&mut b, 5, 3000, 0);
    b.cmd.as_mut().unwrap().links.push(Link {
        medium: Medium::Telegraph,
        a: OfficerId(4),
        b: OfficerId(5),
        cut: false,
    });
    b.world.advance_tick();
    let a = b
        .dispatch(
            &t,
            OfficerId(4),
            OfficerId(5),
            Some(Medium::Courier),
            Order::Move {
                to: metres(50, 50),
                gait: Gait::Quick,
            },
            Latitude::Strict,
        )
        .unwrap();
    b.step_command(&t);
    b.world.advance_tick();
    let countermand = Order::Move {
        to: metres(7, 7),
        gait: Gait::Walk,
    };
    let c = b
        .dispatch(
            &t,
            OfficerId(4),
            OfficerId(5),
            Some(Medium::Telegraph),
            countermand,
            Latitude::Strict,
        )
        .unwrap();
    while b.world.tick < 80_000 {
        b.step_command(&t);
        b.world.advance_tick();
    }
    assert_eq!(b.regiments[0].order, countermand);
    let cmd = b.cmd.as_ref().unwrap();
    let ra = cmd.records.iter().find(|r| r.id == a).unwrap();
    assert_eq!(ra.truth, Truth::Discarded);
    assert_eq!(ra.ack.unwrap().kind, ModKind::Stale);
    let views = b.order_views(&t, OfficerId(4));
    let status = |id| views.iter().find(|v| v.id == id).unwrap().status;
    assert_eq!(status(a), OrderStatus::Stale);
    assert_eq!(status(c), OrderStatus::Delivered);
}

#[test]
fn a_stale_order_does_not_reach_the_subordinates() {
    // Two orders for the brigade officer's formation: the later one wins at every level.
    let t = quiet();
    let mut b = field(9, 1, metres(5000, 5000));
    place(&mut b, 3, 0, 0);
    place(&mut b, 4, 3000, 0);
    b.cmd.as_mut().unwrap().links.push(Link {
        medium: Medium::Telegraph,
        a: OfficerId(3),
        b: OfficerId(4),
        cut: false,
    });
    b.world.advance_tick();
    let slow = Order::Move {
        to: metres(500, 0),
        gait: Gait::Quick,
    };
    let fast = Order::Move {
        to: metres(900, 0),
        gait: Gait::Quick,
    };
    for (m, o) in [(Medium::Courier, slow), (Medium::Telegraph, fast)] {
        b.dispatch(&t, OfficerId(3), OfficerId(4), Some(m), o, Latitude::Strict)
            .unwrap();
        b.step_command(&t);
        b.world.advance_tick();
    }
    while b.world.tick < 80_000 {
        b.step_command(&t);
        b.world.advance_tick();
    }
    let Order::Move { to, .. } = b.regiments[0].order else {
        panic!("regiment 0 should be moving")
    };
    // The fast order's target shifted by the regiment's offset from the brigade centroid.
    assert!(to.x.raw() > metres(800, 0).x.raw());
}
