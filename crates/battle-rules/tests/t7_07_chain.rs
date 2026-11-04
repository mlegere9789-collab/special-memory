//! T7-07 (SPEC-007 section 6): with loss, garble and judgement noise off, an army Move to a corps
//! of 3 x 3 x 3 regiments reaches all 27 with offsets preserved exactly, at the ticks the
//! staff and courier arithmetic gives, and every order is acknowledged.

mod t74_support;

use battle_rules::command::{Latitude, Level, ModKind, OfficerId, OrderStatus, Truth};
use battle_rules::{Gait, Order};
use core_math::Vec2Fx;
use t74_support::*;

fn target() -> Vec2Fx {
    metres(3000, 2500)
}

/// Expected tick at which each regiment `k` gets its order: courier hop and staff time per level.
fn expected_ticks(t: &battle_rules::command::CommandTable) -> Vec<u64> {
    let corps = ready_tick(t, ARMY, CORPS, Level::Corps, 0);
    (0..27)
        .map(|k| {
            let div = ready_tick(t, CORPS, division(k / 9), Level::Division, corps);
            let bde = ready_tick(t, division(k / 9), brigade(k / 3), Level::Brigade, div);
            ready_tick(t, brigade(k / 3), regiment_officer(k), Level::Regiment, bde)
        })
        .collect()
}

#[test]
fn a_move_reaches_all_27_regiments_with_offsets_preserved() {
    let t = exact_table();
    let mut b = tree(7);
    let order = Order::Move {
        to: target(),
        gait: Gait::Quick,
    };
    b.dispatch(
        &t,
        OfficerId(ARMY),
        OfficerId(CORPS),
        None,
        order,
        Latitude::Strict,
    )
    .unwrap();
    let mut got: Vec<Option<(u64, Order)>> = vec![None; 27];
    while b.world.tick < 40_000 {
        b.step_command(&t);
        for (k, slot) in got.iter_mut().enumerate() {
            if slot.is_none() && matches!(b.regiments[k].order, Order::Move { .. }) {
                *slot = Some((b.world.tick, b.regiments[k].order));
            }
        }
        b.world.advance_tick();
    }
    let want = expected_ticks(&t);
    let corps_at = centroid(CORPS);
    for k in 0..27 {
        let (tick, order) = got[k].unwrap_or_else(|| panic!("regiment {k} never got the order"));
        assert_eq!(tick, want[k], "regiment {k} tick");
        let offset = anchor(k) - corps_at;
        let expect = Order::Move {
            to: target() + offset,
            gait: Gait::Quick,
        };
        assert_eq!(order, expect, "regiment {k} target");
    }
}

#[test]
fn every_order_is_acknowledged_as_delivered() {
    let t = exact_table();
    let mut b = tree(7);
    let root = b
        .dispatch(
            &t,
            OfficerId(ARMY),
            OfficerId(CORPS),
            None,
            Order::Hold,
            Latitude::Strict,
        )
        .unwrap();
    while b.world.tick < 40_000 {
        b.step_command(&t);
        b.world.advance_tick();
    }
    let cmd = b.cmd.as_ref().unwrap();
    // One order for each officer below the army: 1 corps, 3 divisions, 9 brigades, 27 regiments.
    assert_eq!(cmd.records.len(), 40);
    assert!(cmd.records.iter().all(|r| r.truth == Truth::Delivered));
    assert!(cmd.records.iter().all(|r| {
        let a = r.ack.expect("acknowledged");
        a.kind == ModKind::None && a.received == r.sent && a.executing == r.sent
    }));
    assert!(cmd.staff.is_empty());
    for issuer in [ARMY, CORPS, division(1), brigade(4)] {
        let views = b.order_views(&t, OfficerId(issuer));
        assert!(!views.is_empty());
        assert!(views.iter().all(|v| v.status == OrderStatus::Delivered));
    }
    // Only the issuer's own orders show: the army sent exactly the first one.
    let army = b.order_views(&t, OfficerId(ARMY));
    assert_eq!(army.len(), 1);
    assert_eq!(army[0].id, root);
}

#[test]
fn the_ack_reaches_the_issuer_after_the_order_was_executed() {
    let t = exact_table();
    let mut b = tree(7);
    b.dispatch(
        &t,
        OfficerId(ARMY),
        OfficerId(CORPS),
        None,
        Order::Hold,
        Latitude::Strict,
    )
    .unwrap();
    let ready = ready_tick(&t, ARMY, CORPS, Level::Corps, 0);
    let mut ack_tick = None;
    while b.world.tick < 10_000 && ack_tick.is_none() {
        b.step_command(&t);
        ack_tick = b.cmd.as_ref().unwrap().records[0].ack.map(|a| a.tick);
        b.world.advance_tick();
    }
    // The ack leaves at `ready` and takes the courier time back (write, base and the leg).
    let m = &t.media[0];
    let back = u64::from(m.write_ticks)
        + u64::from(m.base_ticks)
        + courier_leg(&t, centroid(CORPS), centroid(ARMY));
    assert_eq!(ack_tick, Some(ready + back));
}
