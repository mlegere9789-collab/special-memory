//! Status feedback (SPEC-007 section 8): what the issuer of an order believes became of it, as a
//! pure function of the record and the clock, and the views the interface and the AI read.
//! The truth (what really happened) is shown only with the side's omniscient view or after the
//! battle (D-05: SPEC-007 owns the states and timings).

use super::state::{GarbleKind, OrderRecord, Truth};
use super::MessageId;
use super::{CommandTable, ModKind, OfficerId, OrderBody};
use crate::BattleState;

/// What the issuer believes of an order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrderStatus {
    /// Still being written out.
    Pending,
    /// On its way; no ack is due yet.
    InTransit,
    /// The ack is late.
    Overdue,
    /// No ack long after it was due: presumed lost.
    Lost,
    /// Acknowledged and executed as sent.
    Delivered,
    /// Acknowledged, but the addressee read something other than what was sent.
    Misread,
    /// Acknowledged, but the addressee carries it out differently.
    Modified,
    /// The addressee declined.
    Declined,
    /// The addressee had a later order from the same sender and discarded this one.
    Stale,
}

/// An order as the issuer sees it. `sent`, `received` and `executing` are all listed so the
/// interface shows the difference; `received` and `executing` exist once the ack has arrived.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrderView {
    /// The order's message.
    pub id: MessageId,
    /// Issuer.
    pub from: OfficerId,
    /// Addressee.
    pub to: OfficerId,
    /// Tick it was sent.
    pub sent_tick: u64,
    /// Perceived status now.
    pub status: OrderStatus,
    /// Earliest estimated arrival.
    pub eta_lo: u64,
    /// Latest estimated arrival.
    pub eta_hi: u64,
    /// Tick by which the ack should be back.
    pub ack_deadline: u64,
    /// The order as sent.
    pub sent: OrderBody,
    /// The order as the addressee read it.
    pub received: Option<OrderBody>,
    /// The order as the addressee carries it out.
    pub executing: Option<OrderBody>,
    /// What truly became of it (omniscient view or end of battle only).
    pub truth: Option<Truth>,
    /// How it was garbled, shown with the truth.
    pub garble: Option<GarbleKind>,
}

/// Status of a record at tick `now`. With an ack: `Stale` if marked, else `Declined`, else
/// `Misread` if `received != sent`, else `Modified` if `executing != received`, else
/// `Delivered`. With none: `Pending` until `sent_tick + write_ticks` of its medium, `InTransit`
/// through `ack_deadline`, `Overdue` through `ack_deadline + lost_grace_ticks`, then `Lost`.
pub fn order_status(table: &CommandTable, r: &OrderRecord, now: u64) -> OrderStatus {
    if let Some(a) = &r.ack {
        return match a.kind {
            ModKind::Stale => OrderStatus::Stale,
            ModKind::Declined => OrderStatus::Declined,
            _ if a.received != r.sent => OrderStatus::Misread,
            _ if a.executing != a.received => OrderStatus::Modified,
            _ => OrderStatus::Delivered,
        };
    }
    let write = u64::from(table.media[usize::from(r.medium.tag()) - 1].write_ticks);
    if now < r.sent_tick + write {
        OrderStatus::Pending
    } else if now <= r.ack_deadline {
        OrderStatus::InTransit
    } else if now <= r.ack_deadline + u64::from(table.globals.lost_grace_ticks) {
        OrderStatus::Overdue
    } else {
        OrderStatus::Lost
    }
}

impl BattleState {
    /// The orders `issuer` has sent, ascending by id, with their perceived status. Only the
    /// issuer's own orders show: what lower hops did reaches him only through the pictures. The
    /// truth is filled only when the issuer's side has `omniscient_view`. Empty when the layer
    /// is off or the officer unknown.
    pub fn order_views(&self, table: &CommandTable, issuer: OfficerId) -> Vec<OrderView> {
        let reveal = self.cmd.as_ref().is_some_and(|c| {
            c.index_of(issuer)
                .is_some_and(|i| c.opt[usize::from(c.officers[i].side)].omniscient_view)
        });
        self.views(table, issuer, reveal)
    }

    /// [`BattleState::order_views`] with the truth always filled: for the end of the battle.
    pub fn order_views_final(&self, table: &CommandTable, issuer: OfficerId) -> Vec<OrderView> {
        self.views(table, issuer, true)
    }

    fn views(&self, table: &CommandTable, issuer: OfficerId, reveal: bool) -> Vec<OrderView> {
        let Some(cmd) = self.cmd.as_ref() else {
            return Vec::new();
        };
        let now = self.world.tick;
        cmd.records
            .iter()
            .filter(|r| r.from == issuer)
            .map(|r| OrderView {
                id: r.id,
                from: r.from,
                to: r.to,
                sent_tick: r.sent_tick,
                status: order_status(table, r, now),
                eta_lo: r.eta_lo,
                eta_hi: r.eta_hi,
                ack_deadline: r.ack_deadline,
                sent: r.sent,
                received: r.ack.map(|a| a.received),
                executing: r.ack.map(|a| a.executing),
                truth: reveal.then_some(r.truth),
                garble: if reveal { r.garble } else { None },
            })
            .collect()
    }
}
