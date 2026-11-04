//! Regiments and the drill state machine types (SPEC-004 section 7).

use crate::{Aggregate, FidelityClass, Formation, Order, SlotLocal, Training};
use core_math::{Angle, Vec2Fx};
use sim_state::AgentId;

/// Identifies a regiment; assigned ascending from 0 in spawn order. The id is also the `unit`
/// field of every member in the agent store.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RegimentId(pub u16);

/// The drill state of a regiment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegimentState {
    /// Settled in a formation.
    Steady(Formation),
    /// Changing formation; becomes `Steady(to)` when `ticks_left` reaches zero.
    Forming {
        /// Formation before the change.
        from: Formation,
        /// Formation being taken up.
        to: Formation,
        /// Ticks still to run.
        ticks_left: u32,
    },
}

impl RegimentState {
    /// The formation the regiment is in or is taking up: the layout its slots follow.
    pub const fn target_formation(self) -> Formation {
        match self {
            RegimentState::Steady(f) => f,
            RegimentState::Forming { to, .. } => to,
        }
    }
}

/// Parameters of [`BattleState::spawn_regiment`](crate::BattleState::spawn_regiment).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RegimentSpec {
    /// Side the soldiers belong to (`AgentStore.team`).
    pub team: u8,
    /// Number of soldiers.
    pub count: usize,
    /// Training level.
    pub training: Training,
    /// Starting formation; the men stand on its slots.
    pub formation: Formation,
    /// Anchor position in metres.
    pub anchor: Vec2Fx,
    /// Heading of the regiment.
    pub heading: Angle,
}

/// Whether a regiment is on the field (SPEC-004 amendment A4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegimentStatus {
    /// Its arrival time has not come.
    NotArrived,
    /// On the field.
    Present,
    /// Left the field.
    Departed,
}

/// Rounds of a regiment, in the ledger of SPEC-005 amendment A7: with `held` the rounds in
/// the hands of its live soldiers and in its aggregate, `held + spent + lost == initial +
/// resupplied`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RoundsAccount {
    /// Rounds given by `equip_regiment`.
    pub initial: u64,
    /// Rounds added by `resupply`.
    pub resupplied: u64,
    /// Shots taken, misfires included.
    pub spent: u64,
    /// Rounds that left with a killed soldier, or that a re-equipment replaced.
    pub lost: u64,
}

/// One regiment: its command state and the soldiers that belong to it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Regiment {
    /// Identity, equal to the index in `BattleState::regiments`.
    pub id: RegimentId,
    /// Side.
    pub team: u8,
    /// Training level.
    pub training: Training,
    /// Drill state.
    pub state: RegimentState,
    /// Anchor point of the formation frame.
    pub anchor: Vec2Fx,
    /// Heading of the formation frame.
    pub heading: Angle,
    /// The standing order (never `Form`: a formation change lives in `state`).
    pub order: Order,
    /// An order waiting for the current transition to end.
    pub pending: Option<Order>,
    /// Live, fit members in ascending slot index.
    pub roster: Vec<AgentId>,
    /// The roster changed since the layout was last built.
    pub dirty: bool,
    /// The current layout, in canonical slot order, one entry per rostered soldier.
    pub slots: Vec<SlotLocal>,
    /// On the field or not (SPEC-004 amendment A4).
    pub status: RegimentStatus,
    /// Fidelity class (SPEC-008): 2 means the soldiers are in `aggregate`, not in the store.
    pub class: FidelityClass,
    /// Tick of the last class change.
    pub since: u64,
    /// The soldiers of a class 2 regiment; the carried wounded and killed otherwise.
    pub aggregate: Aggregate,
    /// Round account.
    pub acct: RoundsAccount,
}

/// True when the regiment may fire: `Steady(Line)` or `Steady(Skirmish)`. A regiment in column or
/// in the middle of a formation change cannot.
pub fn can_fire(regiment: &Regiment) -> bool {
    matches!(
        regiment.state,
        RegimentState::Steady(Formation::Line | Formation::Skirmish)
    )
}
