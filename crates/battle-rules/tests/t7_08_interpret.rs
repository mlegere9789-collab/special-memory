//! T7-08 (SPEC-007 section 6): reinterpretation by trait. With `judge_sigma_pm = 0` a strict order
//! executes as received for every mask; a discretionary one goes with the trait's reach and gait
//! shift or is declined; scores equal a table derived by hand from the formula.

mod t73_support;
mod t74_support;

use battle_rules::command::{
    go_score, interpret, InterpretInput, Latitude, ModKind, OfficerId, OrderBody, TraitMask,
};
use battle_rules::{Gait, Order};
use core_math::{Fx, Vec2Fx};
use t73_support::{field, place};
use t74_support::{exact_table, metres};

const AGGR: u8 = 1;
const CAUT: u8 = 2;
const HIGH: u8 = 4;
const LOW: u8 = 8;

fn go(to: Vec2Fx, latitude: Latitude) -> OrderBody {
    OrderBody {
        order: Order::Move {
            to,
            gait: Gait::Quick,
        },
        latitude,
    }
}

fn input(body: OrderBody, own_pm: u32, fatigue_ppm: u32) -> InterpretInput {
    InterpretInput {
        body,
        centroid: Vec2Fx::ZERO,
        own_pm,
        fatigue_ppm,
    }
}

#[test]
fn scores_equal_the_hand_derived_table() {
    let t = exact_table();
    // base 600; own term (own_pm - 500) * 800 / 1000; fatigue term fat * 600 / 10^6;
    // biases aggressive +250, cautious -250, high +100, low -200.
    let table: [(u8, u32, u32, i64); 12] = [
        (0, 1000, 0, 1000),
        (0, 500, 0, 600),
        (AGGR, 500, 0, 850),
        (CAUT, 500, 0, 350),
        (CAUT, 1000, 0, 750),
        (LOW, 450, 800_000, -120),
        (LOW, 450, 600_000, 0),
        (HIGH, 700, 0, 860),
        (0, 700, 0, 760),
        (0, 1, 0, 201),
        (0, 500, 333_333, 401),
        (AGGR | LOW, 500, 0, 650),
    ];
    for (mask, own, fat, want) in table {
        assert_eq!(
            go_score(&t, TraitMask(mask), own, fat, Fx::ZERO),
            want,
            "mask {mask} own {own} fatigue {fat}"
        );
    }
}

#[test]
fn the_judgement_term_is_sigma_times_the_gauss_value() {
    let mut t = exact_table();
    t.globals.judge_sigma_pm = 120;
    assert_eq!(go_score(&t, TraitMask(0), 500, 0, Fx::ONE), 720);
    assert_eq!(go_score(&t, TraitMask(0), 500, 0, -Fx::ONE), 480);
    assert_eq!(
        go_score(&t, TraitMask(0), 500, 0, Fx::from_raw(Fx::ONE.raw() / 2)),
        660
    );
}

#[test]
fn strict_orders_execute_as_received_for_every_mask() {
    let t = exact_table();
    for mask in 0..16u8 {
        for order in [
            go(metres(1000, 0), Latitude::Strict),
            OrderBody {
                order: Order::Hold,
                latitude: Latitude::Strict,
            },
        ] {
            let r = interpret(&t, TraitMask(mask), &input(order, 0, 1_000_000), Fx::ZERO);
            assert_eq!((r.body, r.kind), (order, ModKind::None), "mask {mask}");
        }
    }
}

#[test]
fn a_discretionary_order_takes_the_trait_reach_and_gait_shift() {
    let t = exact_table();
    let body = go(metres(1000, 0), Latitude::Discretionary);
    let r = interpret(&t, TraitMask(AGGR), &input(body, 1000, 0), Fx::ZERO);
    assert_eq!(
        r.body,
        go(metres(1200, 0), Latitude::Discretionary).with_gait(Gait::DoubleQuick)
    );
    assert_eq!(r.kind, ModKind::Extended);
    let r = interpret(&t, TraitMask(CAUT), &input(body, 1000, 0), Fx::ZERO);
    assert_eq!(r.body, go(metres(800, 0), Latitude::Discretionary));
    assert_eq!(r.kind, ModKind::StoppedShort);
    // A neutral officer and a high-discretion one go as ordered.
    for mask in [0, HIGH] {
        let r = interpret(&t, TraitMask(mask), &input(body, 1000, 0), Fx::ZERO);
        assert_eq!((r.body, r.kind), (body, ModKind::None));
    }
}

#[test]
fn the_target_never_falls_behind_the_centroid_and_keeps_its_line() {
    let t = exact_table();
    let near = go(metres(0, 100), Latitude::Discretionary);
    let r = interpret(&t, TraitMask(CAUT), &input(near, 1000, 0), Fx::ZERO);
    assert_eq!(r.body, go(Vec2Fx::ZERO, Latitude::Discretionary));
    assert_eq!(r.kind, ModKind::StoppedShort);
    // A diagonal target moves along its own line: 3-4-5 triangle, 500 m out, 200 m further.
    let diag = go(metres(300, 400), Latitude::Discretionary);
    let r = interpret(&t, TraitMask(AGGR), &input(diag, 1000, 0), Fx::ZERO);
    let Order::Move { to, .. } = r.body.order else {
        panic!("a move")
    };
    let (x, y) = (to.x.raw() as i128, to.y.raw() as i128);
    let one = i128::from(Fx::ONE.raw());
    assert!(
        (x - 420 * one).abs() <= 4 && (y - 560 * one).abs() <= 4,
        "{x} {y}"
    );
}

#[test]
fn a_tired_low_discretion_officer_with_poor_odds_declines() {
    let t = exact_table();
    let body = go(metres(1000, 0), Latitude::Discretionary);
    let r = interpret(&t, TraitMask(LOW), &input(body, 450, 800_000), Fx::ZERO);
    assert_eq!(r.kind, ModKind::Declined);
    assert_eq!(
        r.body,
        OrderBody {
            order: Order::Hold,
            latitude: Latitude::Discretionary
        }
    );
    // Exactly at the threshold the officer goes: own_pm 375 gives 600 - 100 = 500.
    let r = interpret(&t, TraitMask(0), &input(body, 375, 0), Fx::ZERO);
    assert_eq!((r.body, r.kind), (body, ModKind::None));
    // 374 still gives 500 (the odds term truncates to -100); 373 gives 499.
    let r = interpret(&t, TraitMask(0), &input(body, 374, 0), Fx::ZERO);
    assert_eq!(r.kind, ModKind::None);
    let r = interpret(&t, TraitMask(0), &input(body, 373, 0), Fx::ZERO);
    assert_eq!(r.kind, ModKind::Declined);
}

/// Dispatch from the brigade officer 4 to the regiment officer 5 through the whole chain.
fn through_the_chain(mask: u8, latitude: Latitude, tired: bool) -> (Order, ModKind) {
    let t = exact_table();
    let mut b = field(5, 1, metres(5000, 5000));
    place(&mut b, 4, 0, 0);
    b.world.advance_tick();
    {
        let cmd = b.cmd.as_mut().unwrap();
        let i = cmd.index_of(OfficerId(5)).unwrap();
        cmd.officers[i].mask = TraitMask(mask);
    }
    if tired {
        let ids: Vec<_> = b.regiments[0].roster.clone();
        for id in ids {
            b.loco[id.index as usize].fatigue = 800_000;
        }
    }
    b.dispatch(
        &t,
        OfficerId(4),
        OfficerId(5),
        None,
        Order::Move {
            to: metres(1000, 20),
            gait: Gait::Quick,
        },
        latitude,
    )
    .unwrap();
    while b.world.tick < 6000 {
        b.step_command(&t);
        b.world.advance_tick();
    }
    let kind = b.cmd.as_ref().unwrap().records[0].ack.unwrap().kind;
    (b.regiments[0].order, kind)
}

#[test]
fn the_chain_applies_the_officers_own_traits() {
    let (o, k) = through_the_chain(0, Latitude::Strict, false);
    assert_eq!(k, ModKind::None);
    assert!(matches!(
        o,
        Order::Move {
            gait: Gait::Quick,
            ..
        }
    ));
    let (o, k) = through_the_chain(AGGR, Latitude::Strict, false);
    assert_eq!(k, ModKind::None);
    assert!(matches!(
        o,
        Order::Move {
            gait: Gait::Quick,
            ..
        }
    ));
    let (o, k) = through_the_chain(AGGR, Latitude::Discretionary, false);
    assert_eq!(k, ModKind::Extended);
    assert!(matches!(
        o,
        Order::Move {
            gait: Gait::DoubleQuick,
            ..
        }
    ));
    let (o, k) = through_the_chain(LOW, Latitude::Discretionary, true);
    assert_eq!(k, ModKind::Declined);
    assert_eq!(o, Order::Hold);
}

trait WithGait {
    fn with_gait(self, gait: Gait) -> OrderBody;
}

impl WithGait for OrderBody {
    fn with_gait(self, gait: Gait) -> OrderBody {
        match self.order {
            Order::Move { to, .. } => OrderBody {
                order: Order::Move { to, gait },
                ..self
            },
            _ => self,
        }
    }
}
