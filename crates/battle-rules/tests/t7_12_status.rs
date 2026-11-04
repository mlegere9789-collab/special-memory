//! T7-12 (SPEC-007 section 8): the perceived status of an order is a pure function of what has
//! reached its sender. Without an ack it runs Pending, InTransit, Overdue, Lost on exact
//! boundaries; an ack of any kind replaces all four; nothing else is possible.

mod t73_support;

use battle_rules::command::{
    AckRecord, Latitude, Medium, MessageId, ModKind, OfficerId, OrderBody, OrderRecord,
    OrderStatus, Truth,
};
use battle_rules::{Gait, Order};
use proptest::prelude::*;
use t73_support::*;

const ISSUER: OfficerId = OfficerId(4);

fn body(order: Order) -> OrderBody {
    OrderBody {
        order,
        latitude: Latitude::Strict,
    }
}

fn mv(x: i64) -> Order {
    Order::Move {
        to: metres(x, 0),
        gait: Gait::Quick,
    }
}

fn record(
    sent: u64,
    medium: Medium,
    span: (u64, u64),
    deadline: u64,
    ack: Option<AckRecord>,
) -> OrderRecord {
    OrderRecord {
        id: MessageId(0),
        from: ISSUER,
        to: OfficerId(5),
        sent_tick: sent,
        eta_lo: span.0,
        eta_hi: span.1,
        ack_deadline: deadline,
        medium,
        sent: body(mv(10)),
        ack,
        truth: Truth::InFlight,
        garble: None,
    }
}

fn status_at(r: &OrderRecord, now: u64) -> OrderStatus {
    let t = calm_table();
    let mut b = field(1, 1, metres(5000, 5000));
    b.cmd.as_mut().unwrap().records.push(r.clone());
    b.world.tick = now;
    let views = b.order_views(&t, ISSUER);
    assert_eq!(views.len(), 1);
    views[0].status
}

#[test]
fn the_unacknowledged_phases_have_exact_boundaries() {
    let t = calm_table();
    let write = u64::from(t.media[usize::from(Medium::Telegraph.tag()) - 1].write_ticks);
    let grace = u64::from(t.globals.lost_grace_ticks);
    let r = record(1000, Medium::Telegraph, (1700, 1900), 3000, None);
    let at = |now| status_at(&r, now);
    assert_eq!(at(1000), OrderStatus::Pending);
    assert_eq!(at(1000 + write - 1), OrderStatus::Pending);
    assert_eq!(at(1000 + write), OrderStatus::InTransit);
    assert_eq!(at(3000), OrderStatus::InTransit);
    assert_eq!(at(3001), OrderStatus::Overdue);
    assert_eq!(at(3000 + grace), OrderStatus::Overdue);
    assert_eq!(at(3000 + grace + 1), OrderStatus::Lost);
}

#[test]
fn an_ack_names_what_became_of_the_order() {
    let sent = body(mv(10));
    let other = body(mv(99));
    let hold = body(Order::Hold);
    let ack = |received, executing, kind| {
        Some(AckRecord {
            tick: 2000,
            received,
            executing,
            kind,
        })
    };
    let cases = [
        (ack(sent, sent, ModKind::None), OrderStatus::Delivered),
        (ack(other, other, ModKind::None), OrderStatus::Misread),
        (ack(sent, other, ModKind::Extended), OrderStatus::Modified),
        (
            ack(sent, other, ModKind::StoppedShort),
            OrderStatus::Modified,
        ),
        (ack(sent, hold, ModKind::Declined), OrderStatus::Declined),
        (ack(other, hold, ModKind::Declined), OrderStatus::Declined),
        (ack(sent, sent, ModKind::Stale), OrderStatus::Stale),
        (ack(other, hold, ModKind::Stale), OrderStatus::Stale),
        (ack(other, sent, ModKind::None), OrderStatus::Misread),
    ];
    for (a, want) in cases {
        let r = record(1000, Medium::Courier, (3000, 3200), 5000, a);
        // The ack decides, whatever the clock says.
        for now in [1000, 3100, 5001, 99_999] {
            assert_eq!(status_at(&r, now), want, "now {now}");
        }
    }
}

#[test]
fn the_view_lists_both_bodies_and_hides_the_truth() {
    let t = calm_table();
    let mut b = field(1, 1, metres(5000, 5000));
    let sent = body(mv(10));
    let got = body(mv(99));
    let mut r = record(
        10,
        Medium::Courier,
        (100, 300),
        900,
        Some(AckRecord {
            tick: 500,
            received: got,
            executing: got,
            kind: ModKind::None,
        }),
    );
    r.truth = Truth::Delivered;
    r.garble = Some(battle_rules::command::GarbleKind::Displaced);
    b.cmd.as_mut().unwrap().records.push(r);
    b.world.tick = 600;
    let v = &b.order_views(&t, ISSUER)[0];
    assert_eq!(
        (v.sent, v.received, v.executing),
        (sent, Some(got), Some(got))
    );
    assert_eq!((v.eta_lo, v.eta_hi, v.ack_deadline), (100, 300, 900));
    assert_eq!((v.truth, v.garble), (None, None));
    // The omniscient view of the issuer's side, or the end of the battle, shows the truth.
    b.cmd.as_mut().unwrap().opt[0].omniscient_view = true;
    let v = &b.order_views(&t, ISSUER)[0];
    assert_eq!(v.truth, Some(Truth::Delivered));
    assert_eq!(v.garble, Some(battle_rules::command::GarbleKind::Displaced));
    b.cmd.as_mut().unwrap().opt[0].omniscient_view = false;
    assert_eq!(
        b.order_views_final(&t, ISSUER)[0].truth,
        Some(Truth::Delivered)
    );
    // Other officers see none of it.
    assert!(b.order_views(&t, OfficerId(5)).is_empty());
}

fn rank(s: OrderStatus) -> u8 {
    match s {
        OrderStatus::Pending => 0,
        OrderStatus::InTransit => 1,
        OrderStatus::Overdue => 2,
        OrderStatus::Lost => 3,
        _ => 4,
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn statuses_only_move_forward_and_settle(
        sent in 0u64..5000,
        lo in 1u64..4000,
        spread in 0u64..2000,
        extra in 1u64..4000,
        medium in 1u8..4,
    ) {
        let t = calm_table();
        let medium = Medium::from_tag(medium).unwrap();
        let write = u64::from(t.media[usize::from(medium.tag()) - 1].write_ticks);
        let grace = u64::from(t.globals.lost_grace_ticks);
        let eta_lo = sent + write + lo;
        let deadline = eta_lo + spread + extra;
        let r = record(sent, medium, (eta_lo, eta_lo + spread), deadline, None);
        let mut last = 0u8;
        let mut b = field(1, 1, metres(5000, 5000));
        b.cmd.as_mut().unwrap().records.push(r);
        let horizon = deadline + grace + 50;
        let mut now = sent;
        while now <= horizon {
            b.world.tick = now;
            let s = b.order_views(&t, ISSUER)[0].status;
            let k = rank(s);
            prop_assert!(k >= last, "status went backwards at {now}: {s:?}");
            prop_assert!(k <= 3, "no ack, yet {s:?}");
            last = k;
            let expect = if now < sent + write { OrderStatus::Pending }
                else if now <= deadline { OrderStatus::InTransit }
                else if now <= deadline + grace { OrderStatus::Overdue }
                else { OrderStatus::Lost };
            prop_assert_eq!(s, expect);
            now += 37;
        }
        // After the deadline and the grace nothing is still pending or in transit.
        b.world.tick = deadline + grace + 1;
        prop_assert_eq!(b.order_views(&t, ISSUER)[0].status, OrderStatus::Lost);
    }
}

#[test]
fn a_late_ack_replaces_overdue_and_lost() {
    let r = record(0, Medium::Courier, (1000, 1200), 2400, None);
    let grace = u64::from(calm_table().globals.lost_grace_ticks);
    assert_eq!(status_at(&r, 2400 + grace + 10), OrderStatus::Lost);
    let mut late = r.clone();
    late.ack = Some(AckRecord {
        tick: 2400 + grace + 5,
        received: late.sent,
        executing: late.sent,
        kind: ModKind::None,
    });
    assert_eq!(status_at(&late, 2400 + grace + 10), OrderStatus::Delivered);
    assert_eq!(status_at(&late, 2500), OrderStatus::Delivered);
}
