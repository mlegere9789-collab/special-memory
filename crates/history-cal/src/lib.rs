//! Historical replay and calibration (SPEC-011): scripts that feed documented orders into the
//! deterministic core and the thresholds that judge the outcome.
//!
//! This crate is a pure library: no I/O, clock, threads or floating point. Scripts, unit lists and
//! the policy table are integer-only TOML read through the `battle-rules` reader (ADR 0004). Under
//! ADR 0010 decision D-06 arrivals, departures and weather belong to the SPEC-008 scenario; a
//! script holds orders, trigger hooks, observations and overrides (SPEC-011 amendment A1).

#![forbid(unsafe_code)]

mod codec;
mod compile;
mod driver;
mod error;
mod policy;
mod script;
mod stage;
mod units;

pub use codec::{decode_trigger, encode_trigger};
pub use compile::{compile_script, ScriptInput};
pub use driver::{apply_variant, DriverError, MarkerHook, ReplayDriver, TriggerHook, Variant};
pub use error::ScriptError;
pub use policy::Policy;
pub use script::{
    ObserveKind, ObserveRow, OrderKind, OrderRow, OverrideRow, OverrideTarget, OverrideVariant,
    ReplayScript, ScriptMeta, TriggerRow,
};
pub use stage::stage_battle;
pub use units::{units_from_toml_str, Level, UnitRecord};
