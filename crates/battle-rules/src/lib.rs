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

// T-020
pub mod ammo;
pub mod damage;
pub mod weapons;
pub use ammo::{drag_k, sphere_mass_g, AmmoKind, AmmoRow, AmmoTable, Burst, BurstShape, Constants};
pub use damage::{
    energy_joules, DamageTable, HitOutcome, OutcomeCell, Posture, PostureRow, Region,
};
pub use weapons::{AmmoLoad, LoadStep, WeaponClass, WeaponRow, WeaponTable};
