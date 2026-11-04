//! Media, latency, loss and arrival (SPEC-007 section 5): estimates, sending a message on its
//! first leg, and the arrival step that handles smoke waits, extra courier legs, succession,
//! captures, reading and the stale rule. What an arrived order does next (staff time,
//! interpretation, acknowledgement) belongs to the chain.

use super::garble::{note_seq, read_order};
use super::leg::dist_mm;
use super::state::{
    CommandState, Contact, Fate, GarbleKind, Message, MessageKind, MessagePayload, OrderBody,
    OrderRecord, Truth,
};
use super::{CommandTable, DispatchError, Medium, MessageId, Officer, OfficerId};
use crate::registry::STREAM_MESSAGE_BASE;
use crate::{BattleState, Order};
use core_math::{Rng, StreamId};

/// Words of an order for latency: hold 4, face 5, form 5, move 12.
pub fn words(order: &Order) -> u32 {
    match order {
        Order::Hold => 4,
        Order::Face(_) | Order::Form(_) => 5,
        Order::Move { .. } => 12,
    }
}

/// When a message sent now should arrive, and by which medium.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Estimate {
    /// The medium used: the one asked for when it is usable, else a courier.
    pub medium: Medium,
    /// Earliest arrival tick.
    pub eta_lo: u64,
    /// Latest arrival tick.
    pub eta_hi: u64,
}

/// How an addressee read an order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reading {
    /// The body as read, after garbling.
    pub received: OrderBody,
    /// How it was garbled, if it was.
    pub garbled: Option<GarbleKind>,
    /// Out of date: not above the highest `seq` already read from that sender.
    pub stale: bool,
}

/// A message that reached its addressee this tick; orders carry how they were read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Arrival {
    /// The message, addressed to the officer who now holds it.
    pub msg: Message,
    /// Present for orders.
    pub reading: Option<Reading>,
}

/// A message to start on its way (the chain sends acks, reports and fan-out through this).
pub(crate) struct NewMessage {
    pub kind: MessageKind,
    pub from: OfficerId,
    pub to: OfficerId,
    pub medium: Option<Medium>,
    pub seq: u32,
    pub body: OrderBody,
    pub words: u32,
    pub root: Option<MessageId>,
    pub payload: MessagePayload,
}

enum Step {
    Keep(Message),
    Arrived(Arrival),
    Gone,
}

fn index_pair(l: (OfficerId, OfficerId)) -> (OfficerId, OfficerId) {
    (l.0.min(l.1), l.0.max(l.1))
}

fn set_truth(cmd: &mut CommandState, id: MessageId, truth: Truth) {
    if let Ok(i) = cmd.records.binary_search_by_key(&id, |r| r.id) {
        cmd.records[i].truth = truth;
    }
}

impl CommandState {
    /// Men in the formation of an officer: the rosters of every regiment beneath him.
    pub(crate) fn formation_strength(&self, b: &BattleState, id: OfficerId) -> u32 {
        let mut total = 0;
        for o in &self.officers {
            let Some(r) = o.regiment.and_then(|r| b.regiments.get(usize::from(r.0))) else {
                continue;
            };
            let mut at = Some(o.id);
            while let Some(a) = at {
                if a == id {
                    total += r.roster.len() as u32;
                    break;
                }
                at = self.index_of(a).and_then(|i| self.officers[i].parent);
            }
        }
        total
    }

    /// The first officer who is up on the deputy chain of `id`.
    fn living_deputy(&self, id: OfficerId) -> Option<OfficerId> {
        let mut at = self.index_of(id)?;
        for _ in 0..self.officers.len() {
            let d = self.officers[at].deputy?;
            at = self.index_of(d)?;
            if !self.officers[at].down {
                return Some(d);
            }
        }
        None
    }

    fn linked(&self, medium: Medium, a: OfficerId, b: OfficerId) -> bool {
        self.links
            .iter()
            .any(|l| l.medium == medium && !l.cut && index_pair((l.a, l.b)) == index_pair((a, b)))
    }
}

impl BattleState {
    fn estimate_in(
        &self,
        table: &CommandTable,
        cmd: &CommandState,
        (from, to): (&Officer, &Officer),
        medium: Option<Medium>,
        words: u32,
    ) -> Estimate {
        let (t0, g) = (self.world.tick, &table.globals);
        let row = |m: Medium| &table.media[usize::from(m.tag()) - 1];
        let line = |m: Medium, extra: u64| {
            let r = row(m);
            let lo = t0
                + u64::from(r.write_ticks)
                + u64::from(r.base_ticks)
                + u64::from(r.per_word_ticks) * u64::from(words);
            Estimate {
                medium: m,
                eta_lo: lo,
                eta_hi: lo + u64::from(g.jitter_ticks) + extra,
            }
        };
        let r = row(Medium::Courier);
        let leg = self.leg_ticks(table, from.pos, to.pos);
        let lo = t0 + u64::from(r.write_ticks) + u64::from(r.base_ticks) + leg;
        let courier = Estimate {
            medium: Medium::Courier,
            eta_lo: lo,
            eta_hi: lo + u64::from(g.jitter_ticks),
        };
        let waits = u64::from(g.smoke_max_waits) * u64::from(g.smoke_wait_ticks);
        let in_range = dist_mm(from.pos, to.pos) <= g.flag_range_mm;
        let flags = (in_range && cmd.linked(Medium::Flags, from.id, to.id))
            .then(|| line(Medium::Flags, waits));
        let telegraph = cmd
            .linked(Medium::Telegraph, from.id, to.id)
            .then(|| line(Medium::Telegraph, 0));
        let usable = [Some(courier), flags, telegraph].into_iter().flatten();
        match medium {
            None => usable.min_by_key(|e| e.eta_lo),
            Some(m) => usable.into_iter().find(|e| e.medium == m),
        }
        .unwrap_or(courier)
    }

    /// Estimated arrival of a message of `words` words sent now. A medium that cannot be used
    /// (flags with no link or out of range, a missing or cut telegraph) gives a courier; `None`
    /// picks the smallest earliest arrival, ties to the lower medium number. `None` result when
    /// an officer does not exist.
    pub fn estimate(
        &self,
        table: &CommandTable,
        from: OfficerId,
        to: OfficerId,
        medium: Option<Medium>,
        words: u32,
    ) -> Option<Estimate> {
        let cmd = self.cmd.as_ref()?;
        let (f, t) = (cmd.index_of(from)?, cmd.index_of(to)?);
        let pair = (&cmd.officers[f], &cmd.officers[t]);
        Some(self.estimate_in(table, cmd, pair, medium, words))
    }

    /// Sends an order from officer to officer: assigns the sender's next `seq`, records it, and
    /// starts its first leg. Side checks and the instant-order path are the dispatcher's.
    ///
    /// Errors: [`DispatchError::UnknownOfficer`], [`DispatchError::SenderDown`].
    pub fn send_message(
        &mut self,
        table: &CommandTable,
        from: OfficerId,
        to: OfficerId,
        medium: Option<Medium>,
        body: OrderBody,
    ) -> Result<MessageId, DispatchError> {
        let tick = self.world.tick;
        let cmd = self
            .cmd
            .as_mut()
            .ok_or(DispatchError::UnknownOfficer(from))?;
        let fi = cmd
            .index_of(from)
            .ok_or(DispatchError::UnknownOfficer(from))?;
        cmd.index_of(to).ok_or(DispatchError::UnknownOfficer(to))?;
        if cmd.officers[fi].down {
            return Err(DispatchError::SenderDown);
        }
        cmd.officers[fi].seq += 1;
        let seq = cmd.officers[fi].seq;
        let (id, est, fate) = self.launch(
            table,
            NewMessage {
                kind: MessageKind::Order,
                from,
                to,
                medium,
                seq,
                body,
                words: words(&body.order),
                root: None,
                payload: MessagePayload::None,
            },
        )?;
        let truth = match fate {
            Fate::InFlight => Truth::InFlight,
            Fate::Lost => Truth::Lost,
            Fate::Captured => Truth::Captured,
        };
        let cmd = self.cmd.as_mut().expect("layer is on");
        cmd.records.push(OrderRecord {
            id,
            from,
            to,
            sent_tick: tick,
            eta_lo: est.eta_lo,
            eta_hi: est.eta_hi,
            sent: body,
            ack: None,
            truth,
            garble: None,
        });
        Ok(id)
    }

    /// Creates a message, derives its stream and draws its first leg. A message lost on that
    /// leg is not kept. Returns its id, the estimate and what became of it.
    pub(crate) fn launch(
        &mut self,
        table: &CommandTable,
        n: NewMessage,
    ) -> Result<(MessageId, Estimate, Fate), DispatchError> {
        let mut cmd = self
            .cmd
            .take()
            .ok_or(DispatchError::UnknownOfficer(n.from))?;
        let result = self.launch_in(table, &mut cmd, n);
        self.cmd = Some(cmd);
        result
    }

    fn launch_in(
        &self,
        table: &CommandTable,
        cmd: &mut CommandState,
        n: NewMessage,
    ) -> Result<(MessageId, Estimate, Fate), DispatchError> {
        let find = |id| cmd.index_of(id).ok_or(DispatchError::UnknownOfficer(id));
        let (f, t) = (&cmd.officers[find(n.from)?], &cmd.officers[find(n.to)?]);
        if f.down {
            return Err(DispatchError::SenderDown);
        }
        let est = self.estimate_in(table, cmd, (f, t), n.medium, n.words);
        let id = MessageId(cmd.next_msg);
        let stream = StreamId(STREAM_MESSAGE_BASE + (id.0 & 0x00FF_FFFF));
        let mut rng = self.world.rng.stream(stream);
        let (jitter, fate) = self.draw_leg(table, &mut rng, est.medium, f.side, f.pos, t.pos);
        let msg = Message {
            id,
            kind: n.kind,
            from: n.from,
            to: n.to,
            medium: est.medium,
            seq: n.seq,
            body: n.body,
            words: n.words,
            root: n.root.unwrap_or(id),
            leg_from: f.pos,
            aim: t.pos,
            legs: u8::from(est.medium == Medium::Courier),
            due: est.eta_lo + jitter,
            fate,
            garbled: None,
            rng: rng.state(),
            payload: n.payload,
        };
        cmd.next_msg += 1;
        if fate != Fate::Lost {
            cmd.msgs.push(msg);
        }
        Ok((id, est, fate))
    }

    /// Handles every message due at or before this tick, in ascending `(due, id)`, and returns
    /// those that reached their addressee, in that order. The chain step takes them from here.
    pub fn step_messages(&mut self, table: &CommandTable) -> Vec<Arrival> {
        let Some(mut cmd) = self.cmd.take() else {
            return Vec::new();
        };
        let now = self.world.tick;
        let mut due: Vec<(u64, MessageId)> = cmd
            .msgs
            .iter()
            .filter(|m| m.due <= now)
            .map(|m| (m.due, m.id))
            .collect();
        due.sort_unstable();
        let mut arrived = Vec::new();
        for (_, id) in due {
            let Ok(i) = cmd.msgs.binary_search_by_key(&id, |m| m.id) else {
                continue;
            };
            let m = cmd.msgs.remove(i);
            match self.arrive(table, &mut cmd, m, now) {
                Step::Keep(m) => {
                    let at = cmd
                        .msgs
                        .binary_search_by_key(&m.id, |x| x.id)
                        .unwrap_or_else(|e| e);
                    cmd.msgs.insert(at, m);
                }
                Step::Arrived(a) => arrived.push(a),
                Step::Gone => {}
            }
        }
        self.cmd = Some(cmd);
        arrived
    }

    fn arrive(
        &self,
        table: &CommandTable,
        cmd: &mut CommandState,
        mut m: Message,
        now: u64,
    ) -> Step {
        let g = &table.globals;
        if m.fate == Fate::Captured {
            self.add_capture(table, cmd, &m, now);
            return Step::Gone;
        }
        let (Some(fi), Some(ti)) = (cmd.index_of(m.from), cmd.index_of(m.to)) else {
            return Step::Gone;
        };
        if cmd.officers[ti].down {
            return match cmd.living_deputy(m.to) {
                Some(d) => {
                    m.to = d;
                    m.due = now + u64::from(g.redirect_ticks);
                    Step::Keep(m)
                }
                None => {
                    set_truth(cmd, m.id, Truth::Lost);
                    Step::Gone
                }
            };
        }
        let (from_pos, to_pos) = (cmd.officers[fi].pos, cmd.officers[ti].pos);
        if m.medium == Medium::Flags {
            let blocked = |p| self.smoke.density_at(p) > g.smoke_limit;
            if blocked(from_pos) || blocked(to_pos) {
                if u32::from(m.legs) < g.smoke_max_waits {
                    m.legs += 1;
                    m.due = now + u64::from(g.smoke_wait_ticks);
                    return Step::Keep(m);
                }
                m.medium = Medium::Courier;
                m.legs = 1;
                (m.leg_from, m.aim) = (from_pos, to_pos);
                return self.fly(table, cmd, m, now);
            }
        }
        if m.medium == Medium::Courier
            && dist_mm(m.aim, to_pos) > g.find_radius_mm
            && u32::from(m.legs) < g.max_legs
        {
            m.legs += 1;
            (m.leg_from, m.aim) = (m.aim, to_pos);
            return self.fly(table, cmd, m, now);
        }
        let reading = (m.kind == MessageKind::Order).then(|| {
            let mut rng = Rng::from_state(m.rng);
            let (received, garbled) = read_order(table, m.medium, &mut rng, m.body);
            m.rng = rng.state();
            m.garbled = garbled;
            let stale = note_seq(&mut cmd.officers[ti], m.from, m.seq);
            if let Ok(i) = cmd.records.binary_search_by_key(&m.id, |r| r.id) {
                cmd.records[i].garble = garbled;
                cmd.records[i].truth = if stale {
                    Truth::Discarded
                } else {
                    Truth::Delivered
                };
            }
            Reading {
                received,
                garbled,
                stale,
            }
        });
        Step::Arrived(Arrival { msg: m, reading })
    }

    /// Starts a courier leg from `m.leg_from` to `m.aim` at tick `now`.
    fn fly(&self, table: &CommandTable, cmd: &mut CommandState, mut m: Message, now: u64) -> Step {
        let side = cmd.index_of(m.from).map_or(0, |i| cmd.officers[i].side);
        let mut rng = Rng::from_state(m.rng);
        let (jitter, fate) =
            self.draw_leg(table, &mut rng, Medium::Courier, side, m.leg_from, m.aim);
        m.rng = rng.state();
        m.due = now + self.leg_ticks(table, m.leg_from, m.aim) + jitter;
        m.fate = fate;
        match fate {
            Fate::InFlight => Step::Keep(m),
            Fate::Lost => {
                set_truth(cmd, m.id, Truth::Lost);
                Step::Gone
            }
            Fate::Captured => {
                set_truth(cmd, m.id, Truth::Captured);
                Step::Keep(m)
            }
        }
    }

    /// The enemy officer nearest the destination (lowest id on ties) learns of the order: a
    /// contact at its destination, as strong as the sender's formation.
    fn add_capture(&self, table: &CommandTable, cmd: &mut CommandState, m: &Message, now: u64) {
        let Some(side) = cmd.index_of(m.from).map(|i| cmd.officers[i].side) else {
            return;
        };
        if m.kind != MessageKind::Order {
            return;
        }
        let taker = cmd
            .officers
            .iter()
            .enumerate()
            .filter(|(_, o)| o.side != side)
            .min_by_key(|(_, o)| (dist_mm(m.aim, o.pos), o.id))
            .map(|(i, _)| i);
        let Some(i) = taker else { return };
        let strength = cmd.formation_strength(self, m.from);
        let pic = &mut cmd.pics[i];
        pic.contacts.push(Contact {
            id: pic.next_contact,
            pos: m.aim,
            strength,
            quality_pm: table.globals.capture_quality_pm,
            seen: now,
            uncertainty_mm: 0,
            intercepted: true,
        });
        pic.next_contact += 1;
    }
}
