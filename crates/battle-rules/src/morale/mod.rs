//! Morale and cohesion (SPEC-006): the rules table, its schema, and the pure helpers.
//!
//! A regiment's hidden cohesion is built from casualties and a few slow inputs; this module holds
//! the data ([`MoraleTable`]) and the arithmetic that needs no battle state ([`shock_unit`],
//! [`lerp`], [`load_gate`], [`classify_exposure`]), the state of a battle's morale ([`MoraleState`])
//! and its slow-period inputs and ceiling. Everything is integer-only.

mod ceiling;
mod events;
mod helpers;
mod inputs;
mod state;
mod step;
mod table;
mod transition;

pub use crate::registry::{F_LOOKBACK, F_SKULK, STREAM_MORALE_BASE};
pub use ceiling::{
    ceiling_target, decay_window, effective_ceiling, officer_factor_pm, relax_step, Drive,
};
pub use events::{enemy_boost, neighbour_shock};
pub use helpers::{classify_exposure, lerp, load_gate, shock_unit, Exposure};
pub use inputs::MoraleInputs;
pub use state::{
    officers_total, Indicator, Leader, MoraleEvent, MoraleEventKind, MoraleState, RegimentMorale,
};
pub use table::{
    MoraleGlobals, MoraleKind, MoraleTable, MoraleTrainingRow, SymptomRamp, SymptomRow,
};
pub use transition::{next_kind, TransitionIn};

/// Ticks in one slow period (one second): casualties act every tick, everything else on ticks
/// with `tick % SLOW_PERIOD_TICKS == 0`.
pub const SLOW_PERIOD_TICKS: u64 = 20;
