//! Order status (SPEC-015 section 5): what the issuer believes became of an order, a pure
//! function of the record and the clock. Timings are the command layer's (ADR 0010, D-05).

use crate::confidence::{check_schema, take_u32};
use crate::source::OrderRecord;
use battle_rules::{parse_table, DataError, Provenance};

/// Timing of the status machine, from `data/ui/status.toml`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatusParams {
    /// Ticks after the acknowledgement deadline before an order is presumed lost.
    pub lost_grace_ticks: u32,
    /// Provenance of the value.
    pub provenance: Provenance,
}

impl StatusParams {
    /// Parses `status.toml` text. Errors as for the other `data/ui` loaders.
    pub fn from_toml_str(text: &str) -> Result<StatusParams, DataError> {
        let mut root = parse_table(text)?;
        check_schema(&mut root, "ui-status/1")?;
        let mut t = root.take_table("params")?;
        let lost_grace_ticks = take_u32(&mut t, "lost_grace_ticks", 0, u32::MAX)?;
        let provenance = t.take_provenance()?;
        t.finish()?;
        root.finish()?;
        Ok(StatusParams {
            lost_grace_ticks,
            provenance,
        })
    }
}

/// The state of an order as shown in the orders panel and the timeline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrderStatus {
    /// Not yet sent.
    Drafted,
    /// On its way, within the estimate.
    InTransit,
    /// On its way, and the estimate has grown by this many ticks since it was sent.
    Delayed {
        /// `eta_hi - eta_hi0`.
        by_ticks: u64,
    },
    /// Past the deadline with no word yet.
    Overdue,
    /// No word long after the deadline: presumed lost.
    PresumedLost,
    /// Acknowledged.
    Received,
    /// The addressee has begun to carry it out.
    Executing,
    /// Carried out.
    Completed,
    /// Acknowledged, but understood as something other than what was sent.
    Misread,
    /// Known to be lost.
    Lost,
}

impl OrderStatus {
    /// Icon shape id, one per state (the delay figure does not change it).
    pub const fn icon(self) -> u8 {
        match self {
            OrderStatus::Drafted => 0,
            OrderStatus::InTransit => 1,
            OrderStatus::Delayed { .. } => 2,
            OrderStatus::Overdue => 3,
            OrderStatus::PresumedLost => 4,
            OrderStatus::Received => 5,
            OrderStatus::Executing => 6,
            OrderStatus::Completed => 7,
            OrderStatus::Misread => 8,
            OrderStatus::Lost => 9,
        }
    }
}

/// The status and the misread mark of one order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OrderView {
    /// Derived status.
    pub status: OrderStatus,
    /// The addressee understood something other than the order; stays set after completion.
    pub misread: bool,
}

/// First match wins: completed, known lost, acknowledged (misread, executing, received), drafted,
/// within the deadline (delayed or in transit), within the grace (overdue), presumed lost.
/// `eta_hi` is the acknowledgement deadline; a late acknowledgement outranks `Overdue` and
/// `PresumedLost` because the acknowledged states come first.
pub fn derive_status(rec: &OrderRecord, now: u64, p: &StatusParams) -> OrderStatus {
    if rec.completed_tick.is_some() {
        return OrderStatus::Completed;
    }
    if rec.lost_known {
        return OrderStatus::Lost;
    }
    if rec.ack_tick.is_some() {
        return if is_misread(rec) {
            OrderStatus::Misread
        } else if rec.executed_tick.is_some() {
            OrderStatus::Executing
        } else {
            OrderStatus::Received
        };
    }
    if rec.sent_tick.is_none() {
        return OrderStatus::Drafted;
    }
    if now <= rec.eta_hi {
        return if rec.eta_hi > rec.eta_hi0 {
            OrderStatus::Delayed {
                by_ticks: rec.eta_hi - rec.eta_hi0,
            }
        } else {
            OrderStatus::InTransit
        };
    }
    if now <= rec.eta_hi.saturating_add(u64::from(p.lost_grace_ticks)) {
        OrderStatus::Overdue
    } else {
        OrderStatus::PresumedLost
    }
}

fn is_misread(rec: &OrderRecord) -> bool {
    rec.interpreted.as_ref().is_some_and(|i| *i != rec.order)
}

/// The status and misread mark of `rec` at tick `now`.
pub fn order_view(rec: &OrderRecord, now: u64, p: &StatusParams) -> OrderView {
    OrderView {
        status: derive_status(rec, now, p),
        misread: is_misread(rec),
    }
}
