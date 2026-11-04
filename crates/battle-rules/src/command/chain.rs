//! The chain of command (SPEC-007 section 6): an order that has been read waits for the staff of
//! its addressee, is interpreted, acknowledged to its sender and then either carried out (a
//! regiment) or passed to each direct subordinate with the shape of the formation kept.
//!
//! [`BattleState::dispatch`] is the one way an order enters the chain, for the human seat and
//! the AI alike. With the side's `instant_orders` an order skips the media: it is read on the
//! next tick, with no draws, no loss, no staff time and no reinterpretation.

use super::garble::note_seq;
use super::leg::scale32;
use super::media::{Arrival, NewMessage};
use super::state::{
    AckRecord, CommandState, MessageKind, MessagePayload, ModKind, OrderBody, OrderRecord, Truth,
};
use super::{
    decode_dispatch, CommandTable, DispatchError, InputError, Latitude, Level, Medium, MessageId,
    OfficerId,
};
use crate::registry::{STREAM_MESSAGE_BASE, TAG_DISPATCH};
use crate::{gauss_unit, BattleState, Order};
use core_math::{Fx, Rng, StreamId, Vec2Fx};

/// Words of an acknowledgement for latency: a short header and the two codes.
pub(crate) const ACK_WORDS: u32 = 6;

/// An order read by its addressee and waiting for his staff.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StaffJob {
    /// Tick the staff finishes.
    pub ready: u64,
    /// The order's message, which is also the key of its record.
    pub id: MessageId,
    /// The issuer, to whom the ack goes.
    pub from: OfficerId,
    /// The officer whose staff works on it.
    pub to: OfficerId,
    /// The issuer's hour of the order.
    pub seq: u32,
    /// The order as read, after garbling.
    pub received: OrderBody,
    /// The order is out of date and is only acknowledged.
    pub stale: bool,
    /// Sent with `instant_orders`: no friction, no reinterpretation.
    pub instant: bool,
    /// State of the message's stream after reading and the staff draw.
    pub rng: [u64; 4],
}

/// Staff time factor of a mask, permille: the product of the trait values, divided by 1000 per
/// factor.
fn process_pm(table: &CommandTable, mask: super::TraitMask) -> u64 {
    table
        .traits
        .iter()
        .filter(|row| mask.contains(row.mask))
        .fold(1000u64, |p, row| p * u64::from(row.process_pm) / 1000)
}

impl CommandState {
    /// Index of the officer in the pictures and officers vectors.
    fn at(&self, id: OfficerId) -> Option<usize> {
        self.index_of(id)
    }

    /// Indices of the regiments of a formation: those of the officers beneath `id`, `id` included.
    pub(crate) fn formation_regiments(&self, id: OfficerId) -> Vec<usize> {
        let mut out = Vec::new();
        for o in &self.officers {
            let Some(r) = o.regiment else { continue };
            let mut at = Some(o.id);
            while let Some(a) = at {
                if a == id {
                    out.push(usize::from(r.0));
                    break;
                }
                at = self.at(a).and_then(|i| self.officers[i].parent);
            }
        }
        out
    }

    fn record_mut(&mut self, id: MessageId) -> Option<&mut OrderRecord> {
        let i = self.records.binary_search_by_key(&id, |r| r.id).ok()?;
        Some(&mut self.records[i])
    }

    fn add_job(&mut self, job: StaffJob) {
        let at = self
            .staff
            .partition_point(|j| (j.ready, j.id) <= (job.ready, job.id));
        self.staff.insert(at, job);
    }
}

impl BattleState {
    /// Integer mean (truncating) of the anchors of the regiments with men in the formation of
    /// `id`, the place his subordinates are measured from; `None` when none has men.
    pub(crate) fn formation_centroid(&self, id: OfficerId) -> Option<Vec2Fx> {
        let cmd = self.cmd.as_ref()?;
        let (mut x, mut y, mut n) = (0i128, 0i128, 0i128);
        for r in cmd.formation_regiments(id) {
            let reg = self.regiments.get(r)?;
            if !reg.roster.is_empty() {
                x += i128::from(reg.anchor.x.raw());
                y += i128::from(reg.anchor.y.raw());
                n += 1;
            }
        }
        (n > 0).then(|| Vec2Fx::new(Fx::from_raw((x / n) as i64), Fx::from_raw((y / n) as i64)))
    }

    /// Latest estimate of a message of an ack's length sent from `from` to `to` now, in ticks from
    /// now: the return trip of an order.
    pub(crate) fn return_trip(&self, table: &CommandTable, from: OfficerId, to: OfficerId) -> u64 {
        self.estimate(table, from, to, None, ACK_WORDS)
            .map_or(0, |e| e.eta_hi - self.world.tick)
    }

    /// Sends an order from one officer to another of his side, as the human seat or the AI does.
    /// The order gets the sender's next `seq` and an [`OrderRecord`]; its fate is then the media's
    /// and the chain's. `medium` `None` picks the fastest usable one. With the sender's side on
    /// `instant_orders` it is read on the next tick without friction.
    ///
    /// Errors: [`DispatchError::UnknownOfficer`] for an officer that does not exist (or no
    /// command layer), [`DispatchError::WrongSide`] when the two are not on one side,
    /// [`DispatchError::SenderDown`] when the sender has fallen. Nothing changes on an error.
    pub fn dispatch(
        &mut self,
        table: &CommandTable,
        from: OfficerId,
        to: OfficerId,
        medium: Option<Medium>,
        order: Order,
        latitude: Latitude,
    ) -> Result<MessageId, DispatchError> {
        let cmd = self
            .cmd
            .as_ref()
            .ok_or(DispatchError::UnknownOfficer(from))?;
        let find = |id: OfficerId| {
            cmd.index_of(id)
                .map(|i| &cmd.officers[i])
                .ok_or(DispatchError::UnknownOfficer(id))
        };
        let (f, t) = (find(from)?, find(to)?);
        if f.side != t.side {
            return Err(DispatchError::WrongSide);
        }
        if f.down {
            return Err(DispatchError::SenderDown);
        }
        self.send_order(table, from, to, medium, OrderBody { order, latitude })
    }

    /// Applies one replay input of `actor` and, for a valid dispatch, sends it: the shell's entry
    /// point when it holds the friction table. Everything else is
    /// [`BattleState::apply_command_input`].
    pub fn apply_input(
        &mut self,
        table: &CommandTable,
        actor: u8,
        payload: &[u8],
    ) -> Result<(), InputError> {
        self.apply_command_input(actor, payload)?;
        if payload.first() == Some(&TAG_DISPATCH) {
            let d = decode_dispatch(payload)?;
            self.dispatch(table, d.from, d.to, d.medium, d.order, d.latitude)
                .map_err(InputError::Dispatch)?;
        }
        Ok(())
    }

    /// Sends an order the issuer has already been cleared to send.
    fn send_order(
        &mut self,
        table: &CommandTable,
        from: OfficerId,
        to: OfficerId,
        medium: Option<Medium>,
        body: OrderBody,
    ) -> Result<MessageId, DispatchError> {
        let side = self
            .cmd
            .as_ref()
            .and_then(|c| c.index_of(from).map(|i| c.officers[i].side))
            .ok_or(DispatchError::UnknownOfficer(from))?;
        let instant = self
            .cmd
            .as_ref()
            .is_some_and(|c| c.opt[usize::from(side)].instant_orders);
        if instant {
            self.instant_order(from, to, body)
        } else {
            self.send_message(table, from, to, medium, body)
        }
    }

    /// The instant path: the order takes an id and its stream is derived but never drawn from, it
    /// is queued for the next tick and its record shows it delivered then.
    fn instant_order(
        &mut self,
        from: OfficerId,
        to: OfficerId,
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
        let id = MessageId(cmd.next_msg);
        cmd.next_msg += 1;
        let stream = StreamId(STREAM_MESSAGE_BASE + (id.0 & 0x00FF_FFFF));
        let rng = self.world.rng.stream(stream).state();
        cmd.records.push(OrderRecord {
            id,
            from,
            to,
            sent_tick: tick,
            eta_lo: tick + 1,
            eta_hi: tick + 1,
            ack_deadline: tick + 1,
            medium: Medium::Courier,
            sent: body,
            ack: None,
            truth: Truth::InFlight,
            garble: None,
        });
        cmd.add_job(StaffJob {
            ready: tick + 1,
            id,
            from,
            to,
            seq,
            received: body,
            stale: false,
            instant: true,
            rng,
        });
        Ok(id)
    }

    /// The chain step of TK-03, run after arrivals: acks that arrived reach their issuers'
    /// records; orders that arrived are queued for the staff of their addressee (the staff
    /// draw comes right after the reading draws of the message's own stream); then every job
    /// whose staff time is up, in ascending `(ready, id)`, is carried out.
    pub fn step_chain(&mut self, table: &CommandTable, arrivals: &[Arrival]) {
        let now = self.world.tick;
        let Some(cmd) = self.cmd.as_mut() else { return };
        for a in arrivals {
            let m = &a.msg;
            match (m.kind, &a.reading, &m.payload) {
                (MessageKind::Order, Some(reading), _) => {
                    let Some(i) = cmd.index_of(m.to) else {
                        continue;
                    };
                    let o = &cmd.officers[i];
                    let staff = u64::from(table.levels[usize::from(o.level.tag())].staff_ticks)
                        * process_pm(table, o.mask)
                        / 1000;
                    let mut rng = Rng::from_state(m.rng);
                    let jitter = scale32(rng.next_u32(), table.globals.staff_jitter_ticks + 1);
                    cmd.add_job(StaffJob {
                        ready: now + staff + jitter,
                        id: m.id,
                        from: m.from,
                        to: m.to,
                        seq: m.seq,
                        received: reading.received,
                        stale: reading.stale,
                        instant: false,
                        rng: rng.state(),
                    });
                }
                (MessageKind::Ack, _, MessagePayload::Ack(ack)) => {
                    if let Some(r) = cmd.record_mut(m.root).filter(|r| r.ack.is_none()) {
                        r.ack = Some(AckRecord { tick: now, ..*ack });
                    }
                }
                _ => {}
            }
        }
        let due = cmd.staff.partition_point(|j| j.ready <= now);
        let jobs: Vec<StaffJob> = cmd.staff.drain(..due).collect();
        for job in jobs {
            self.run_job(table, job);
        }
    }

    /// Carries out one order whose staff time is up: interprets it, acknowledges it to its
    /// issuer, then executes or passes it down. A job whose officer has fallen is dropped (his
    /// issuer will see the order overdue and then lost).
    fn run_job(&mut self, table: &CommandTable, job: StaffJob) {
        let now = self.world.tick;
        let Some(cmd) = self.cmd.as_mut() else { return };
        let Some(oi) = cmd.index_of(job.to).filter(|&i| !cmd.officers[i].down) else {
            return;
        };
        let stale =
            job.stale || (job.instant && note_seq(&mut cmd.officers[oi], job.from, job.seq));
        if job.instant {
            if let Some(r) = cmd.record_mut(job.id) {
                r.truth = if stale {
                    Truth::Discarded
                } else {
                    Truth::Delivered
                };
            }
        }
        let (executing, kind) = if stale {
            (job.received, ModKind::Stale)
        } else if job.instant {
            (job.received, ModKind::None)
        } else {
            let mut rng = Rng::from_state(job.rng);
            let g = gauss_unit(&mut rng);
            let r = self.interpret_for(table, job.to, job.received, g);
            (r.body, r.kind)
        };
        let ack = AckRecord {
            tick: now,
            received: job.received,
            executing,
            kind,
        };
        if job.instant {
            if let Some(r) = self.cmd.as_mut().and_then(|c| c.record_mut(job.id)) {
                r.ack = Some(ack);
            }
        } else {
            // A message that cannot be sent is simply not sent.
            let _ = self.launch(
                table,
                NewMessage {
                    kind: MessageKind::Ack,
                    from: job.to,
                    to: job.from,
                    medium: None,
                    seq: 0,
                    body: OrderBody {
                        order: Order::Hold,
                        latitude: Latitude::Strict,
                    },
                    words: ACK_WORDS,
                    root: Some(job.id),
                    payload: MessagePayload::Ack(ack),
                },
            );
        }
        if !stale {
            self.carry_out(table, job.to, executing);
        }
    }

    /// A regiment officer gives the order to his regiment; any other sends one to each direct
    /// subordinate in ascending id, with the same latitude and his own `seq`. A move keeps the
    /// shape of the formation: the target shifts by the subordinate's centroid minus his own.
    fn carry_out(&mut self, table: &CommandTable, id: OfficerId, body: OrderBody) {
        let Some(cmd) = self.cmd.as_ref() else { return };
        let Some(oi) = cmd.index_of(id) else { return };
        if cmd.officers[oi].level == Level::Regiment {
            if let Some(r) = cmd.officers[oi].regiment {
                self.issue_order(r, body.order);
            }
            return;
        }
        let subs: Vec<OfficerId> = cmd
            .officers
            .iter()
            .filter(|o| o.parent == Some(id))
            .map(|o| o.id)
            .collect();
        let own = self.formation_centroid(id);
        for sub in subs {
            let order = match (body.order, own, self.formation_centroid(sub)) {
                (Order::Move { to, gait }, Some(c), Some(s)) => Order::Move {
                    to: to + (s - c),
                    gait,
                },
                (order, _, _) => order,
            };
            let sent = OrderBody { order, ..body };
            // A subordinate who cannot be reached (the sender has fallen) gets nothing.
            let _ = self.send_order(table, id, sub, None, sent);
        }
    }
}
