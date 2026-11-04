//! T7-11 (instant-order and AI-parity parts, SPEC-007 sections 4, 7 and 9): human and AI
//! dispatches with equal inputs deliver on equal ticks; `instant_orders` delivers next tick with
//! no draws and leaves every message stream as it was; bad dispatches are refused.

mod t73_support;

use battle_rules::command::{
    encode_dispatch, encode_options, CommandOptions, DispatchError, InputError, Latitude, Medium,
    OfficerId,
};
use battle_rules::{Gait, Order};
use t73_support::*;

/// No loss or garbling, and officers stay where `place` put them.
fn quiet() -> battle_rules::command::CommandTable {
    let mut t = calm_table();
    t.globals.pos_interval_ticks = 1_000_000;
    t
}

fn mv() -> Order {
    Order::Move {
        to: metres(400, 300),
        gait: Gait::Quick,
    }
}

fn first_move_tick(actor: u8) -> (u64, u64) {
    let t = quiet();
    let mut b = field(21, 1, metres(5000, 5000));
    place(&mut b, 4, 0, 0);
    place(&mut b, 5, 1500, 0);
    b.world.advance_tick();
    let p = encode_dispatch(OfficerId(4), OfficerId(5), 0, Latitude::Strict, &mv());
    b.apply_input(&t, actor, &p).unwrap();
    while b.world.tick < 30_000 {
        b.step_command(&t);
        if matches!(b.regiments[0].order, Order::Move { .. }) {
            return (b.world.tick, b.hash());
        }
        b.world.advance_tick();
    }
    panic!("never delivered")
}

#[test]
fn ai_and_human_dispatches_deliver_on_equal_ticks() {
    let human = first_move_tick(0);
    let ai = first_move_tick(0x80);
    assert_eq!(human, ai);
}

#[test]
fn dispatches_are_checked_for_side_and_sender() {
    let t = quiet();
    let mut b = field(21, 1, metres(5000, 5000));
    let send = |b: &mut battle_rules::BattleState, from, to| {
        b.dispatch(
            &t,
            OfficerId(from),
            OfficerId(to),
            None,
            Order::Hold,
            Latitude::Strict,
        )
    };
    assert_eq!(
        send(&mut b, 4, 99),
        Err(DispatchError::UnknownOfficer(OfficerId(99)))
    );
    assert_eq!(
        send(&mut b, 98, 4),
        Err(DispatchError::UnknownOfficer(OfficerId(98)))
    );
    assert_eq!(send(&mut b, 4, 12), Err(DispatchError::WrongSide));
    assert_eq!(send(&mut b, 12, 4), Err(DispatchError::WrongSide));
    b.cmd.as_mut().unwrap().officers[3].down = true; // officer 4
    assert_eq!(send(&mut b, 4, 5), Err(DispatchError::SenderDown));
    // Nothing was created by a refused dispatch.
    let cmd = b.cmd.as_ref().unwrap();
    assert!(cmd.records.is_empty() && cmd.msgs.is_empty() && cmd.next_msg == 0);
    // The shell path refuses what the seat may not send.
    let p = encode_dispatch(
        OfficerId(4),
        OfficerId(5),
        0,
        Latitude::Strict,
        &Order::Hold,
    );
    assert_eq!(
        b.apply_input(&t, 0, &p),
        Err(InputError::Dispatch(DispatchError::SenderDown))
    );
    assert_eq!(
        b.apply_input(&t, 1, &p),
        Err(InputError::Dispatch(DispatchError::WrongSide))
    );
}

#[test]
fn instant_orders_deliver_next_tick_and_leave_streams_unchanged() {
    let t = quiet();
    let run = |instant: bool| {
        let mut b = field(33, 1, metres(5000, 5000));
        place(&mut b, 4, 0, 0);
        place(&mut b, 5, 1500, 0);
        b.world.advance_tick();
        b.cmd.as_mut().unwrap().opt[0] = CommandOptions {
            omniscient_view: false,
            instant_orders: instant,
        };
        let first = b
            .dispatch(
                &t,
                OfficerId(4),
                OfficerId(5),
                Some(Medium::Courier),
                mv(),
                Latitude::Strict,
            )
            .unwrap();
        let after_first = b.cmd.as_ref().unwrap().msgs.len();
        // A later message in the same battle, sent by courier either way.
        b.cmd.as_mut().unwrap().opt[0].instant_orders = false;
        let x = b
            .dispatch(
                &t,
                OfficerId(5),
                OfficerId(4),
                Some(Medium::Courier),
                Order::Hold,
                Latitude::Strict,
            )
            .unwrap();
        let sent = b
            .cmd
            .as_ref()
            .unwrap()
            .msgs
            .iter()
            .find(|m| m.id == x)
            .cloned()
            .unwrap();
        let mut first_tick = None;
        while b.world.tick < 30_000 {
            b.step_command(&t);
            if first_tick.is_none() && matches!(b.regiments[0].order, Order::Move { .. }) {
                first_tick = Some(b.world.tick);
            }
            b.world.advance_tick();
        }
        let rec = b
            .cmd
            .as_ref()
            .unwrap()
            .records
            .iter()
            .find(|r| r.id == x)
            .cloned()
            .unwrap();
        (
            first,
            after_first,
            first_tick,
            sent.rng,
            sent.due,
            rec.ack.is_some(),
        )
    };
    let (id_i, msgs_i, tick_i, rng_i, due_i, ack_i) = run(true);
    let (id_c, msgs_c, tick_c, rng_c, due_c, ack_c) = run(false);
    assert_eq!(id_i, id_c);
    // The instant order makes no message and delivers on the next tick.
    // Sent at tick 1, so delivered at tick 2.
    assert_eq!((msgs_i, tick_i), (0, Some(2)));
    assert_eq!(msgs_c, 1);
    assert!(tick_c.unwrap() > 1000);
    // The later message has the same stream and first leg either way, and is acknowledged.
    // (Its ack takes a later id when the instant order sends none, so the tick may differ.)
    assert_eq!((rng_i, due_i, ack_i), (rng_c, due_c, ack_c));
}

#[test]
fn instant_orders_pass_down_the_chain_one_tick_per_hop() {
    let t = quiet();
    let mut b = field(33, 1, metres(5000, 5000));
    let p = encode_options(
        0,
        CommandOptions {
            omniscient_view: false,
            instant_orders: true,
        },
    );
    b.apply_command_input(0, &p).unwrap();
    let order = Order::Move {
        to: metres(700, 700),
        gait: Gait::Walk,
    };
    // Officer 3 is a division, 4 a brigade over regiments 5 and 6: two hops to the regiments.
    b.dispatch(
        &t,
        OfficerId(3),
        OfficerId(4),
        None,
        order,
        Latitude::Strict,
    )
    .unwrap();
    let mut got = [None, None];
    while b.world.tick < 20 {
        b.step_command(&t);
        for (k, g) in got.iter_mut().enumerate() {
            if g.is_none() && matches!(b.regiments[k].order, Order::Move { .. }) {
                *g = Some(b.world.tick);
            }
        }
        b.world.advance_tick();
    }
    assert_eq!(got, [Some(2), Some(2)]);
    assert!(b
        .cmd
        .as_ref()
        .unwrap()
        .records
        .iter()
        .all(|r| r.ack.is_some()));
}
