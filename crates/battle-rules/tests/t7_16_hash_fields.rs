//! T7-16, field-sensitivity half (SPEC-007 section 9): with `cmd = None` the hash has no command
//! block; with `Some`, changing any hashed field changes the hash.

mod t7_support;

use battle_rules::command::{
    AckRecord, CommandState, Contact, Fate, FriendEntry, GarbleKind, Latitude, Message, MessageId,
    MessageKind, MessagePayload, ModKind, OfficerId, OrderBody, OrderRecord, Truth,
};
use battle_rules::registry::MAGIC_COMMAND;
use battle_rules::{battle_hash, Order};
use core_math::{Fx, Vec2Fx};
use t7_support::{command_battle, plain_battle};

fn pt(x: i64, y: i64) -> Vec2Fx {
    Vec2Fx::new(Fx::from_raw(x), Fx::from_raw(y))
}

fn body() -> OrderBody {
    OrderBody {
        order: Order::Hold,
        latitude: Latitude::Strict,
    }
}

fn message() -> Message {
    Message {
        id: MessageId(0),
        kind: MessageKind::Order,
        from: OfficerId(1),
        to: OfficerId(2),
        medium: battle_rules::command::Medium::Courier,
        seq: 1,
        body: body(),
        words: 4,
        root: MessageId(0),
        leg_from: pt(1, 2),
        aim: pt(3, 4),
        legs: 1,
        due: 100,
        fate: Fate::InFlight,
        garbled: None,
        rng: [1, 2, 3, 4],
        payload: MessagePayload::None,
    }
}

fn record() -> OrderRecord {
    OrderRecord {
        id: MessageId(0),
        from: OfficerId(1),
        to: OfficerId(2),
        sent_tick: 5,
        eta_lo: 50,
        eta_hi: 70,
        ack_deadline: 120,
        medium: battle_rules::command::Medium::Courier,
        sent: body(),
        ack: Some(AckRecord {
            tick: 90,
            received: body(),
            executing: body(),
            kind: ModKind::None,
        }),
        truth: Truth::InFlight,
        garble: None,
    }
}

fn contact() -> Contact {
    Contact {
        id: 0,
        pos: pt(5, 6),
        strength: 300,
        quality_pm: 800,
        seen: 40,
        uncertainty_mm: 1000,
        intercepted: false,
    }
}

/// A battle whose command state holds one of everything that has a field.
fn full() -> battle_rules::BattleState {
    let mut b = command_battle();
    let c = b.cmd.as_mut().unwrap();
    c.msgs.push(message());
    c.records.push(record());
    c.next_msg = 1;
    c.officers[0].last_seq_from.push((OfficerId(2), 3));
    c.pics[0].contacts.push(contact());
    c.pics[0].friends.push(FriendEntry {
        regiment: battle_rules::RegimentId(0),
        pos: pt(7, 8),
        strength: 20,
        seen: 10,
    });
    b
}

type Edit = fn(&mut CommandState);

#[test]
fn every_hashed_field_changes_the_hash() {
    let edits: Vec<(&str, Edit)> = vec![
        ("opt omniscient", |c| c.opt[0].omniscient_view = true),
        ("opt instant", |c| c.opt[1].instant_orders = true),
        ("officer id", |c| c.officers[0].id = OfficerId(90)),
        ("officer side", |c| c.officers[0].side ^= 1),
        ("officer level", |c| {
            c.officers[0].level = battle_rules::command::Level::Corps
        }),
        ("officer parent", |c| {
            c.officers[0].parent = Some(OfficerId(3))
        }),
        ("officer regiment", |c| {
            c.officers[0].regiment = Some(battle_rules::RegimentId(1))
        }),
        ("officer mask", |c| {
            c.officers[0].mask = battle_rules::command::TraitMask(2)
        }),
        ("officer deputy", |c| {
            c.officers[0].deputy = Some(OfficerId(4))
        }),
        ("officer down", |c| c.officers[0].down = true),
        ("officer pos", |c| c.officers[0].pos = pt(1, 1)),
        ("officer hq", |c| c.officers[0].hq_fixed = Some(pt(1, 1))),
        ("officer post range", |c| c.officers[0].post_range_pm += 1),
        ("officer seq", |c| c.officers[0].seq += 1),
        ("officer last_seq", |c| {
            c.officers[0].last_seq_from[0].1 += 1
        }),
        ("officer last_seq id", |c| {
            c.officers[0].last_seq_from[0].0 = OfficerId(5)
        }),
        ("officer last_report", |c| c.officers[0].last_report += 1),
        ("link medium", |c| {
            c.links[0].medium = battle_rules::command::Medium::Telegraph
        }),
        ("link a", |c| c.links[0].a = OfficerId(1)),
        ("link b", |c| c.links[0].b = OfficerId(1)),
        ("link cut", |c| c.links[0].cut = true),
        ("msg id", |c| c.msgs[0].id = MessageId(1)),
        ("msg kind", |c| c.msgs[0].kind = MessageKind::Ack),
        ("msg from", |c| c.msgs[0].from = OfficerId(3)),
        ("msg to", |c| c.msgs[0].to = OfficerId(3)),
        ("msg medium", |c| {
            c.msgs[0].medium = battle_rules::command::Medium::Flags
        }),
        ("msg seq", |c| c.msgs[0].seq += 1),
        ("msg body order", |c| {
            c.msgs[0].body.order = Order::Form(battle_rules::Formation::Column)
        }),
        ("msg body latitude", |c| {
            c.msgs[0].body.latitude = Latitude::Discretionary
        }),
        ("msg words", |c| c.msgs[0].words += 1),
        ("msg root", |c| c.msgs[0].root = MessageId(9)),
        ("msg leg_from", |c| c.msgs[0].leg_from = pt(9, 9)),
        ("msg aim", |c| c.msgs[0].aim = pt(9, 9)),
        ("msg legs", |c| c.msgs[0].legs += 1),
        ("msg due", |c| c.msgs[0].due += 1),
        ("msg fate", |c| c.msgs[0].fate = Fate::Lost),
        ("msg garbled", |c| {
            c.msgs[0].garbled = Some(GarbleKind::Stalled)
        }),
        ("msg rng", |c| c.msgs[0].rng[3] ^= 1),
        ("msg payload ack", |c| {
            c.msgs[0].payload = MessagePayload::Ack(AckRecord {
                tick: 1,
                received: body(),
                executing: body(),
                kind: ModKind::Declined,
            })
        }),
        ("msg payload report", |c| {
            c.msgs[0].payload = MessagePayload::Report {
                contacts: vec![contact()],
                friends: Vec::new(),
            }
        }),
        ("record id", |c| c.records[0].id = MessageId(3)),
        ("record from", |c| c.records[0].from = OfficerId(3)),
        ("record to", |c| c.records[0].to = OfficerId(3)),
        ("record sent_tick", |c| c.records[0].sent_tick += 1),
        ("record eta_lo", |c| c.records[0].eta_lo += 1),
        ("record eta_hi", |c| c.records[0].eta_hi += 1),
        ("record sent", |c| {
            c.records[0].sent.latitude = Latitude::Discretionary
        }),
        ("record ack none", |c| c.records[0].ack = None),
        ("record ack tick", |c| {
            c.records[0].ack.as_mut().unwrap().tick += 1
        }),
        ("record ack kind", |c| {
            c.records[0].ack.as_mut().unwrap().kind = ModKind::Stale
        }),
        ("record ack received", |c| {
            c.records[0].ack.as_mut().unwrap().received.order =
                Order::Face(core_math::Angle::from_raw(5))
        }),
        ("record ack executing", |c| {
            c.records[0].ack.as_mut().unwrap().executing.latitude = Latitude::Discretionary
        }),
        ("record truth", |c| c.records[0].truth = Truth::Delivered),
        ("record garble", |c| {
            c.records[0].garble = Some(GarbleKind::Turned)
        }),
        ("next_msg", |c| c.next_msg += 1),
        ("contact id", |c| c.pics[0].contacts[0].id = 1),
        ("contact pos", |c| c.pics[0].contacts[0].pos = pt(0, 0)),
        ("contact strength", |c| c.pics[0].contacts[0].strength += 1),
        ("contact quality", |c| c.pics[0].contacts[0].quality_pm += 1),
        ("contact seen", |c| c.pics[0].contacts[0].seen += 1),
        ("contact uncertainty", |c| {
            c.pics[0].contacts[0].uncertainty_mm += 1
        }),
        ("contact intercepted", |c| {
            c.pics[0].contacts[0].intercepted = true
        }),
        ("friend regiment", |c| {
            c.pics[0].friends[0].regiment = battle_rules::RegimentId(1)
        }),
        ("friend pos", |c| c.pics[0].friends[0].pos = pt(0, 0)),
        ("friend strength", |c| c.pics[0].friends[0].strength += 1),
        ("friend seen", |c| c.pics[0].friends[0].seen += 1),
        ("pic next contact", |c| c.pics[0].next_contact += 1),
        ("pic added", |c| c.pics.push(Default::default())),
        ("officer added", |c| {
            let o = c.officers[0].clone();
            c.officers.push(o)
        }),
        ("msg dropped", |c| {
            c.msgs.clear();
        }),
    ];
    let base = full();
    let base_hash = base.hash();
    assert_eq!(full().hash(), base_hash, "building twice hashes the same");
    for (name, edit) in edits {
        let mut b = base.clone();
        edit(b.cmd.as_mut().unwrap());
        assert_ne!(b.hash(), base_hash, "field not hashed: {name}");
    }
}

#[test]
fn cmd_none_writes_no_block_and_some_writes_the_magic_block() {
    let plain = plain_battle();
    let mut b = command_battle();
    assert_ne!(b.hash(), plain.hash());
    assert_eq!(battle_hash(&b), b.hash());
    b.cmd = None;
    assert_eq!(
        b.hash(),
        plain.hash(),
        "cmd None leaves the SPEC-004 and SPEC-005 stream"
    );
    // The block starts with its magic: an empty layer differs from none and from a changed magic.
    assert_eq!(MAGIC_COMMAND, 0x434D_4431);
}
