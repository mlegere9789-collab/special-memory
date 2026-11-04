//! The command hash block (SPEC-007 section 9, SPEC-004 amendment A2).

use super::state::{
    AckRecord, CommandState, Contact, FriendEntry, Message, MessagePayload, OrderBody, OrderRecord,
};
use crate::registry::{MAGIC_COMMAND, OPT_INSTANT_ORDERS, OPT_OMNISCIENT_VIEW};
use crate::{encode_order, RegimentId};
use core_math::Vec2Fx;
use sim_state::StateHasher;

fn len(h: &mut StateHasher, n: usize) {
    h.write_u32(u32::try_from(n).unwrap_or(u32::MAX));
}

fn point(h: &mut StateHasher, p: Vec2Fx) {
    h.write_i64(p.x.raw());
    h.write_i64(p.y.raw());
}

/// `Option` as a `u8` tag (0 none, 1 some) followed by the value when there is one.
fn opt<T>(h: &mut StateHasher, v: Option<T>, put: impl FnOnce(&mut StateHasher, T)) {
    match v {
        None => h.write_u8(0),
        Some(v) => {
            h.write_u8(1);
            put(h, v);
        }
    }
}

fn body(h: &mut StateHasher, b: &OrderBody) {
    h.write_bytes(&encode_order(RegimentId(0), &b.order));
    h.write_u8(b.latitude.tag());
}

fn ack(h: &mut StateHasher, a: &AckRecord) {
    h.write_u64(a.tick);
    body(h, &a.received);
    body(h, &a.executing);
    h.write_u8(a.kind as u8);
}

fn contact(h: &mut StateHasher, c: &Contact) {
    h.write_u32(c.id);
    point(h, c.pos);
    h.write_u32(c.strength);
    h.write_u32(c.quality_pm);
    h.write_u64(c.seen);
    h.write_i64(c.uncertainty_mm);
    h.write_u8(u8::from(c.intercepted));
}

fn friend(h: &mut StateHasher, f: &FriendEntry) {
    h.write_u16(f.regiment.0);
    point(h, f.pos);
    h.write_u32(f.strength);
    h.write_u64(f.seen);
}

fn contacts_and_friends(h: &mut StateHasher, cs: &[Contact], fs: &[FriendEntry]) {
    len(h, cs.len());
    cs.iter().for_each(|c| contact(h, c));
    len(h, fs.len());
    fs.iter().for_each(|f| friend(h, f));
}

fn message(h: &mut StateHasher, m: &Message) {
    h.write_u32(m.id.0);
    h.write_u8(m.kind as u8);
    h.write_u16(m.from.0);
    h.write_u16(m.to.0);
    h.write_u8(m.medium.tag());
    h.write_u32(m.seq);
    body(h, &m.body);
    h.write_u32(m.words);
    h.write_u32(m.root.0);
    point(h, m.leg_from);
    point(h, m.aim);
    h.write_u8(m.legs);
    h.write_u64(m.due);
    h.write_u8(m.fate as u8);
    opt(h, m.garbled, |h, g| h.write_u8(g as u8));
    m.rng.iter().for_each(|&w| h.write_u64(w));
    match &m.payload {
        MessagePayload::None => h.write_u8(0),
        MessagePayload::Ack(a) => {
            h.write_u8(1);
            ack(h, a);
        }
        MessagePayload::Report { contacts, friends } => {
            h.write_u8(2);
            contacts_and_friends(h, contacts, friends);
        }
    }
}

fn record(h: &mut StateHasher, r: &OrderRecord) {
    h.write_u32(r.id.0);
    h.write_u16(r.from.0);
    h.write_u16(r.to.0);
    h.write_u64(r.sent_tick);
    h.write_u64(r.eta_lo);
    h.write_u64(r.eta_hi);
    h.write_u64(r.ack_deadline);
    h.write_u8(r.medium.tag());
    body(h, &r.sent);
    opt(h, r.ack, |h, a| ack(h, &a));
    h.write_u8(r.truth as u8);
    opt(h, r.garble, |h, g| h.write_u8(g as u8));
}

/// Writes the command block: the magic `0x434D_4431`, then every field of [`CommandState`] in
/// declaration order, little-endian and fixed width, each `Vec` as a `u32` length and its items
/// ascending, orders as their wire bytes, `Option` as a `u8` tag then the value.
pub fn write_command_block(h: &mut StateHasher, c: &CommandState) {
    h.write_u32(MAGIC_COMMAND);
    for o in &c.opt {
        h.write_u8(
            (if o.omniscient_view {
                OPT_OMNISCIENT_VIEW
            } else {
                0
            }) | (if o.instant_orders {
                OPT_INSTANT_ORDERS
            } else {
                0
            }),
        );
    }
    len(h, c.officers.len());
    for o in &c.officers {
        h.write_u16(o.id.0);
        h.write_u8(o.side);
        h.write_u8(o.level.tag());
        opt(h, o.parent, |h, p| h.write_u16(p.0));
        opt(h, o.regiment, |h, r| h.write_u16(r.0));
        h.write_u8(o.mask.0);
        opt(h, o.deputy, |h, d| h.write_u16(d.0));
        h.write_u8(u8::from(o.down));
        point(h, o.pos);
        opt(h, o.hq_fixed, point);
        h.write_u32(o.post_range_pm);
        h.write_u32(o.seq);
        len(h, o.last_seq_from.len());
        for (from, seq) in &o.last_seq_from {
            h.write_u16(from.0);
            h.write_u32(*seq);
        }
        h.write_u64(o.last_report);
    }
    len(h, c.links.len());
    for l in &c.links {
        h.write_u8(l.medium.tag());
        h.write_u16(l.a.0);
        h.write_u16(l.b.0);
        h.write_u8(u8::from(l.cut));
    }
    len(h, c.msgs.len());
    c.msgs.iter().for_each(|m| message(h, m));
    len(h, c.records.len());
    c.records.iter().for_each(|r| record(h, r));
    h.write_u32(c.next_msg);
    len(h, c.pics.len());
    for p in &c.pics {
        contacts_and_friends(h, &p.contacts, &p.friends);
        h.write_u32(p.next_contact);
    }
    len(h, c.staff.len());
    for j in &c.staff {
        h.write_u64(j.ready);
        h.write_u32(j.id.0);
        h.write_u16(j.from.0);
        h.write_u16(j.to.0);
        h.write_u32(j.seq);
        body(h, &j.received);
        h.write_u8(u8::from(j.stale));
        h.write_u8(u8::from(j.instant));
        j.rng.iter().for_each(|&w| h.write_u64(w));
    }
}
