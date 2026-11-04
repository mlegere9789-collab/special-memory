//! Decision notes (SPEC-010 section 6, Explanation): every decision appends an [`AiNote`] with
//! the two values that decided it.

use crate::port::{SeatId, UnitRef};

/// The kind of plan a decision belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanKind {
    /// Operational candidate: hold the line.
    Stay,
    /// Operational candidate: seize the top objective.
    Attack,
    /// Operational candidate: fall back to the second line.
    Withdraw,
    /// Operational candidate: all force on the top objective.
    Concentrate,
    /// Tactical: take a position.
    Place,
    /// Tactical: open fire.
    Fire,
    /// Tactical: cease fire.
    Cease,
    /// The outbox sent an overdue order again.
    Resend,
    /// The outbox reported a silent recipient to the parent seat.
    Escalate,
}

impl PlanKind {
    /// Stable number for hashing.
    pub const fn tag(self) -> u8 {
        self as u8
    }
}

/// Why a decision was made.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReasonCode {
    /// Withdrawal: an order said so.
    Ordered,
    /// Withdrawal: own:enemy under `withdraw_ratio` for `withdraw_confirm_ticks`.
    Outnumbered,
    /// Withdrawal: a flank is exposed.
    FlankExposed,
    /// Withdrawal: ammunition nearly gone.
    AmmoOut,
    /// Withdrawal: cohesion below the gate.
    Wavering,
    /// Withdrawal: strength well under the reference.
    Bled,
    /// A discretionary objective was not undertaken.
    Declined,
    /// An enemy inside the line was met by the reserve.
    Penetration,
    /// A reserve unit was committed.
    ReserveCommit,
    /// A screen fell back from a stronger contact.
    Screened,
    /// No clear line to the target.
    NoLine,
    /// A recipient never acknowledged.
    NoReply,
    /// An advisor reply was rejected.
    AdvisorRejected,
    /// The plan assigned this order.
    Assigned,
    /// An overdue order was sent again unchanged.
    Resent,
}

impl ReasonCode {
    /// Stable number for hashing.
    pub const fn tag(self) -> u8 {
        self as u8
    }
}

/// One logged decision.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AiNote {
    /// Tick of the decision.
    pub tick: u64,
    /// Deciding seat.
    pub seat: SeatId,
    /// The unit it concerns.
    pub unit: UnitRef,
    /// The plan.
    pub plan: PlanKind,
    /// The reason.
    pub reason: ReasonCode,
    /// First deciding value.
    pub v0: i64,
    /// Second deciding value.
    pub v1: i64,
}
