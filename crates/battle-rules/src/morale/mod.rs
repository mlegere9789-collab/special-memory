//! Morale and cohesion (SPEC-006): the rules table, its schema, and the pure helpers.
//!
//! A regiment's hidden cohesion is built from casualties and a few slow inputs; this module holds
//! the data ([`MoraleTable`]) and the arithmetic that needs no battle state ([`shock_unit`],
//! [`lerp`], [`load_gate`], [`classify_exposure`]). Everything is integer-only.

mod helpers;
mod table;

pub use crate::registry::{F_LOOKBACK, F_SKULK, STREAM_MORALE_BASE};
pub use helpers::{classify_exposure, lerp, load_gate, shock_unit, Exposure};
pub use table::{
    MoraleGlobals, MoraleKind, MoraleTable, MoraleTrainingRow, SymptomRamp, SymptomRow,
};

/// Ticks in one slow period (one second): casualties act every tick, everything else on ticks
/// with `tick % SLOW_PERIOD_TICKS == 0`.
pub const SLOW_PERIOD_TICKS: u64 = 20;
