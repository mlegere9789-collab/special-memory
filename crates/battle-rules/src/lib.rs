//! Battle rules: movement, drill and formations (SPEC-004), later weapons and fire (SPEC-005).
//!
//! This crate holds the shared conventions ([`fx_ratio`], tick rate, stream and flag constants),
//! the integer-only rules-data reader ([`parse_table`], [`Table`], [`Provenance`]) and the
//! [`DataError`] every loader returns. State is `Fx` and integers only; iteration is ascending
//! id; rules data never reach the simulation as floating point (ADR 0004).

#![forbid(unsafe_code)]

mod conventions;
mod error;
mod provenance;
mod reader;

pub use conventions::{
    fx_ratio, F_DISABLED, F_GUN, F_KNEEL, F_PRONE, F_WOUNDED, STREAM_BATTLE_SETUP, TICK_HZ,
};
pub use error::DataError;
pub use provenance::{Confidence, Provenance, SourceRef};
pub use reader::{parse_table, Table};

// T-016
mod fatigue;
mod locomotion;
mod terrain;
pub use fatigue::{fatigue_speed_pm, fatigue_step};
pub use locomotion::{CurvePoint, Gait, GaitRow, LocomotionGlobals, LocomotionTable, FATIGUE_MAX};
pub use terrain::{TerrainClass, TerrainGrid, TerrainTable, DEFAULT_CELL_MM};
