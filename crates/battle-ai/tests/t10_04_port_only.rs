//! T10-04: the AI sees the world through `HqPort` only. No `BattleState` outside `lookahead.rs`,
//! and every submitted message decodes through the SPEC-004, SPEC-005 or SPEC-007 codecs.

mod support;

use battle_ai::{
    Ai, ContactId, Decision, Difficulty, FireOrder, ObjectiveBody, ObjectiveKind, OrderBody,
    OutMessage, PlanKind, ReasonCode, SeatId, SeatLevel, UnitRef,
};
use battle_rules::command::{decode_dispatch, Latitude};
use battle_rules::{decode_fire_mode, FireMode, Gait, Order, RegimentId};
use core_math::{Fx, Vec2Fx};
use support::{ai, run, seat, Once, ScriptPort};

#[test]
fn no_battle_state_outside_lookahead() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
    let mut seen = 0;
    for entry in std::fs::read_dir(dir).expect("src dir") {
        let path = entry.expect("entry").path();
        if path.extension().is_none_or(|e| e != "rs")
            || path.file_name().is_some_and(|n| n == "lookahead.rs")
        {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("read");
        assert!(
            !text.contains("BattleState"),
            "{} names the world state",
            path.display()
        );
        seen += 1;
    }
    assert!(seen >= 7, "scanned {seen} files");
}

fn msg(body: OrderBody) -> OutMessage {
    OutMessage {
        from: SeatId(3),
        to: UnitRef(11),
        body,
    }
}

#[test]
fn regiment_orders_encode_as_strict_dispatches() {
    let orders = [
        Order::Hold,
        Order::Move {
            to: Vec2Fx::new(Fx::from_int(5), Fx::from_int(-6)),
            gait: Gait::DoubleQuick,
        },
        Order::Form(battle_rules::Formation::Column),
    ];
    for order in orders {
        let bytes = msg(OrderBody::Reg(order)).wire().expect("wire form");
        let d = decode_dispatch(&bytes).expect("decodes");
        assert_eq!((d.from.0, d.to.0), (3, 11));
        assert_eq!(d.latitude, Latitude::Strict);
        assert_eq!(d.medium, None);
        assert_eq!(d.order, order);
    }
}

#[test]
fn objectives_use_the_move_fallback() {
    let center = Vec2Fx::new(Fx::from_int(100), Fx::from_int(200));
    for kind in [
        ObjectiveKind::Seize,
        ObjectiveKind::Hold,
        ObjectiveKind::Screen,
        ObjectiveKind::Support,
        ObjectiveKind::Delay,
        ObjectiveKind::Withdraw,
    ] {
        let body = OrderBody::Objective(ObjectiveBody {
            kind,
            center,
            radius_mm: 30_000,
            deadline_tick: 900,
            priority: 2,
            discretionary: true,
        });
        let d = decode_dispatch(&msg(body).wire().expect("wire")).expect("decodes");
        assert_eq!(
            d.order,
            Order::Move {
                to: center,
                gait: Gait::Quick
            },
            "{kind:?}"
        );
        assert_eq!(
            d.latitude,
            Latitude::Strict,
            "the AI's own gate has approved it"
        );
    }
}

#[test]
fn fire_orders_never_carry_an_enemy_regiment() {
    let (r, mode) = decode_fire_mode(
        &msg(OrderBody::Fire(FireOrder::Cease))
            .wire()
            .expect("cease has a wire form"),
    )
    .expect("decodes");
    assert_eq!((r, mode), (RegimentId(11), FireMode::HoldFire));
    // The target of `AtContact` is resolved inside the port, so the crate has no bytes for it.
    assert_eq!(
        msg(OrderBody::Fire(FireOrder::AtContact(ContactId(2)))).wire(),
        None
    );
}

#[test]
fn every_submitted_message_decodes() {
    let d = Decision {
        to: UnitRef(5),
        body: OrderBody::Reg(Order::Hold),
        plan: PlanKind::Place,
        reason: ReasonCode::Assigned,
        v0: 0,
        v1: 0,
    };
    let mut a = ai(
        0,
        Difficulty::Standard,
        &[seat(1, SeatLevel::Tactical, None, "")],
        1,
    );
    a.set_planners(Box::new(Once(None)), Box::new(Once(Some(d))));
    let mut p = ScriptPort::new(10);
    run(&mut a, &mut p, 500);
    assert_eq!(p.log.len(), 1);
    for s in &p.log {
        decode_dispatch(&s.msg.wire().expect("wire")).expect("decodes");
        assert_eq!(s.msg.from, SeatId(1));
    }
}

#[test]
fn empty_planners_touch_only_the_clock_and_send_nothing() {
    let mut a: Ai = ai(
        0,
        Difficulty::Hard,
        &[
            seat(0, SeatLevel::Operational, None, ""),
            seat(1, SeatLevel::Tactical, Some(0), ""),
        ],
        3,
    );
    let mut p = ScriptPort::new(10);
    run(&mut a, &mut p, 2000);
    assert!(p.log.is_empty());
    assert!(a.notes().is_empty());
    let calls = p.calls.borrow();
    assert!(calls
        .iter()
        .all(|c| matches!(*c, "now" | "own_reports" | "contacts" | "map")));
    assert!(calls.contains(&"now"));
}

#[test]
fn explain_returns_the_latest_note_of_a_unit() {
    let mut a = ai(
        0,
        Difficulty::Hard,
        &[seat(1, SeatLevel::Tactical, None, "")],
        1,
    );
    a.set_planners(
        Box::new(Once(None)),
        Box::new(support::Draws { unit: 5, n: 0 }),
    );
    let mut p = ScriptPort::new(10);
    assert!(a.explain(UnitRef(5)).is_none());
    run(&mut a, &mut p, 1000);
    let last = a.notes().iter().rev().find(|n| n.unit == UnitRef(5));
    assert_eq!(a.explain(UnitRef(5)), last);
    assert!(a.explain(UnitRef(6)).is_none());
}

#[test]
fn the_hash_follows_the_state() {
    let go = |seed: u64, to: u64| {
        let mut a = ai(
            0,
            Difficulty::Hard,
            &[seat(1, SeatLevel::Tactical, None, "")],
            seed,
        );
        a.set_planners(
            Box::new(Once(None)),
            Box::new(support::Draws { unit: 5, n: 0 }),
        );
        let mut p = ScriptPort::new(10);
        run(&mut a, &mut p, to);
        a.hash()
    };
    assert_eq!(go(1, 800), go(1, 800));
    assert_ne!(go(1, 800), go(2, 800));
    assert_ne!(go(1, 800), go(1, 900));
}
