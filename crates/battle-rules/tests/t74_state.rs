//! T-074 state: the staff queue and the new order-record fields are hashed, and `step_with_command`
//! leaves a battle without the command layer exactly as `step` does.

mod t7_support;

use battle_rules::command::{
    CommandTable, Latitude, Medium, MessageId, OfficerId, OrderBody, StaffJob,
};
use battle_rules::Order;
use t7_support::{command_battle, plain_battle, TABLE};

type Edit = fn(&mut StaffJob);

fn job() -> StaffJob {
    StaffJob {
        ready: 500,
        id: MessageId(3),
        from: OfficerId(4),
        to: OfficerId(5),
        seq: 2,
        received: OrderBody {
            order: Order::Hold,
            latitude: Latitude::Strict,
        },
        stale: false,
        instant: false,
        rng: [1, 2, 3, 4],
    }
}

#[test]
fn every_staff_job_field_changes_the_hash() {
    let base = {
        let mut b = command_battle();
        b.cmd.as_mut().unwrap().staff.push(job());
        b.hash()
    };
    assert_ne!(base, command_battle().hash());
    let edits: Vec<(&str, Edit)> = vec![
        ("ready", |j| j.ready += 1),
        ("id", |j| j.id = MessageId(4)),
        ("from", |j| j.from = OfficerId(6)),
        ("to", |j| j.to = OfficerId(6)),
        ("seq", |j| j.seq += 1),
        ("received", |j| {
            j.received.latitude = Latitude::Discretionary
        }),
        ("stale", |j| j.stale = true),
        ("instant", |j| j.instant = true),
        ("rng", |j| j.rng[3] ^= 1),
    ];
    for (name, edit) in edits {
        let mut b = command_battle();
        let mut j = job();
        edit(&mut j);
        b.cmd.as_mut().unwrap().staff.push(j);
        assert_ne!(b.hash(), base, "{name}");
    }
}

#[test]
fn the_new_record_fields_change_the_hash() {
    let make = || {
        let mut b = command_battle();
        let t = CommandTable::from_toml_str(TABLE).unwrap();
        b.send_message(
            &t,
            OfficerId(5),
            OfficerId(6),
            Some(Medium::Courier),
            OrderBody {
                order: Order::Hold,
                latitude: Latitude::Strict,
            },
        )
        .unwrap();
        b
    };
    let base = make().hash();
    let mut b = make();
    b.cmd.as_mut().unwrap().records[0].ack_deadline += 1;
    assert_ne!(b.hash(), base);
    let mut b = make();
    b.cmd.as_mut().unwrap().records[0].medium = Medium::Flags;
    assert_ne!(b.hash(), base);
}

#[test]
fn without_the_command_layer_step_with_command_is_step() {
    let t = CommandTable::from_toml_str(TABLE).unwrap();
    let (mut a, mut b) = (plain_battle(), plain_battle());
    for _ in 0..60 {
        a.step();
        b.step_with_command(&t);
        assert_eq!(a.hash(), b.hash());
    }
}

#[test]
fn with_the_layer_on_the_command_step_comes_before_movement() {
    // An instant order issued before the tick is queued; the tick after, the regiment has it
    // and the same call moves it.
    let t = CommandTable::from_toml_str(TABLE).unwrap();
    let mut b = command_battle();
    let p = battle_rules::command::encode_options(
        0,
        battle_rules::command::CommandOptions {
            omniscient_view: false,
            instant_orders: true,
        },
    );
    b.apply_command_input(0, &p).unwrap();
    let to = core_math::Vec2Fx::new(core_math::Fx::from_int(300), core_math::Fx::from_int(20));
    b.dispatch(
        &t,
        OfficerId(5),
        OfficerId(5),
        None,
        Order::Move {
            to,
            gait: battle_rules::Gait::Quick,
        },
        Latitude::Strict,
    )
    .unwrap();
    b.step_with_command(&t);
    assert_eq!(b.regiments[0].order, Order::Hold);
    b.step_with_command(&t);
    assert!(matches!(b.regiments[0].order, Order::Move { .. }));
}
