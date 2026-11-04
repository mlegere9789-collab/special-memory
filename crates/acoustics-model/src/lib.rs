//! Sound propagation, acoustic shadow, aggregation and listeners (SPEC-012).
//!
//! This crate reads simulation output and never writes to `BattleState`. Every result is an
//! integer: levels in centibels, distances in millimetres, times in milliseconds. This first
//! slice holds the integer math ([`intmath`]) and the rules data ([`AcousticTables`]).

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![cfg_attr(not(test), deny(clippy::float_arithmetic))]

pub mod intmath;
mod tables;

pub use tables::{
    AcousticTables, BandRow, Budget, Globals, GroundRow, SourceRow, WeatherPreset, CLASS_COUNT,
};
