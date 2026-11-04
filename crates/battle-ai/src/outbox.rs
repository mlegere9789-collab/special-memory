//! The outbox (SPEC-010 section 6, Messages): think delay, one unacknowledged order per
//! recipient, resend of an overdue order unchanged up to `max_resends`, then escalation.
//!
//! A decision made at tick `t` waits in the outbox until `t + think + reaction_ticks`. It is
//! submitted at once when its recipient has nothing in flight, or once the order in flight has
//! been outstanding for `ack_timeout_ticks`; a newer decision for a recipient replaces an older
//! one still waiting. An order in flight that nothing replaces is resent unchanged at each
//! timeout; after `max_resends` resends the next timeout frees the recipient and reports it.

use crate::port::{
    HqPort, MsgId, MsgStatus, ObjectiveBody, OrderBody, OutMessage, SeatId, UnitRef,
};
use battle_rules::{encode_order, RegimentId};
use sim_state::StateHasher;
use std::collections::BTreeMap;

/// Something the outbox did that the AI logs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Event {
    /// An overdue order was sent again.
    Resent {
        seat: SeatId,
        unit: UnitRef,
        id: MsgId,
        resends: u32,
    },
    /// A recipient never answered; the slot is free again.
    Silent {
        seat: SeatId,
        unit: UnitRef,
        resends: u32,
    },
}

struct Slot {
    msg: OutMessage,
    id: MsgId,
    sent: u64,
    resends: u32,
}

struct Waiting {
    due: u64,
    seq: u64,
    msg: OutMessage,
}

/// Orders waiting for their think time and orders in flight.
#[derive(Default)]
pub(crate) struct Outbox {
    in_flight: BTreeMap<UnitRef, Slot>,
    waiting: Vec<Waiting>,
    seq: u64,
    /// The last objective decided per `(seat, recipient)`.
    objectives: BTreeMap<(SeatId, UnitRef), ObjectiveBody>,
    /// Tick of the last order submitted per recipient.
    dwell: BTreeMap<UnitRef, u64>,
}

impl Outbox {
    /// Queues a decision for `due`. Returns false, queuing nothing, when it is an objective equal
    /// to the last one decided for that recipient.
    pub(crate) fn enqueue(&mut self, due: u64, msg: OutMessage) -> bool {
        if let OrderBody::Objective(o) = msg.body {
            if self.objectives.get(&(msg.from, msg.to)) == Some(&o) {
                return false;
            }
            self.objectives.insert((msg.from, msg.to), o);
        }
        self.waiting.retain(|w| w.msg.to != msg.to);
        self.seq += 1;
        self.waiting.push(Waiting {
            due,
            seq: self.seq,
            msg,
        });
        self.waiting.sort_by_key(|w| (w.due, w.seq));
        true
    }

    /// One tick: forget acknowledged orders, release due decisions, resend or give up on
    /// overdue orders. Events come in the order they happened.
    pub(crate) fn step(
        &mut self,
        port: &mut dyn HqPort,
        now: u64,
        timeout: u32,
        max_resends: u32,
    ) -> Vec<Event> {
        let overdue = |s: &Slot| now >= s.sent + u64::from(timeout);
        self.in_flight
            .retain(|_, s| port.status(s.id) != MsgStatus::Acked);
        let mut kept = Vec::new();
        for w in std::mem::take(&mut self.waiting) {
            let free = self.in_flight.get(&w.msg.to).is_none_or(overdue);
            if w.due > now || !free {
                kept.push(w);
                continue;
            }
            let id = port.submit(w.msg);
            self.dwell.insert(w.msg.to, now);
            self.in_flight.insert(
                w.msg.to,
                Slot {
                    msg: w.msg,
                    id,
                    sent: now,
                    resends: 0,
                },
            );
        }
        self.waiting = kept;
        let mut events = Vec::new();
        let mut silent = Vec::new();
        for (&unit, s) in &mut self.in_flight {
            if s.sent == now || !overdue(s) {
                continue;
            }
            if s.resends < max_resends {
                s.id = port.submit(s.msg);
                s.sent = now;
                s.resends += 1;
                self.dwell.insert(unit, now);
                events.push(Event::Resent {
                    seat: s.msg.from,
                    unit,
                    id: s.id,
                    resends: s.resends,
                });
            } else {
                silent.push(unit);
                events.push(Event::Silent {
                    seat: s.msg.from,
                    unit,
                    resends: s.resends,
                });
            }
        }
        for unit in silent {
            self.in_flight.remove(&unit);
        }
        events
    }

    /// Feeds the outstanding messages, the objective tables and the dwell timers to `h`.
    pub(crate) fn hash_into(&self, h: &mut StateHasher) {
        h.write_u32(self.in_flight.len() as u32);
        for (unit, s) in &self.in_flight {
            h.write_u16(unit.0);
            h.write_u32(s.id.0);
            h.write_u64(s.sent);
            h.write_u32(s.resends);
            hash_message(h, &s.msg);
        }
        h.write_u32(self.waiting.len() as u32);
        for w in &self.waiting {
            h.write_u64(w.due);
            hash_message(h, &w.msg);
        }
        h.write_u32(self.objectives.len() as u32);
        for ((seat, unit), o) in &self.objectives {
            h.write_u16(seat.0);
            h.write_u16(unit.0);
            hash_objective(h, o);
        }
        h.write_u32(self.dwell.len() as u32);
        for (unit, tick) in &self.dwell {
            h.write_u16(unit.0);
            h.write_u64(*tick);
        }
    }
}

fn hash_objective(h: &mut StateHasher, o: &ObjectiveBody) {
    h.write_u8(o.kind as u8);
    h.write_i64(o.center.x.raw());
    h.write_i64(o.center.y.raw());
    h.write_i64(o.radius_mm);
    h.write_u64(o.deadline_tick);
    h.write_u16(o.priority);
    h.write_u8(u8::from(o.discretionary));
}

fn hash_message(h: &mut StateHasher, m: &OutMessage) {
    h.write_u16(m.from.0);
    h.write_u16(m.to.0);
    match &m.body {
        OrderBody::Reg(order) => {
            h.write_u8(1);
            h.write_bytes(&encode_order(RegimentId(0), order));
        }
        OrderBody::Fire(f) => {
            h.write_u8(2);
            match f {
                crate::FireOrder::Cease => h.write_u8(0),
                crate::FireOrder::AtContact(c) => {
                    h.write_u8(1);
                    h.write_u32(c.0);
                }
            }
        }
        OrderBody::Objective(o) => {
            h.write_u8(3);
            hash_objective(h, o);
        }
    }
}
