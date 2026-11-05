//! Sound propagation, acoustic shadow, aggregation and listeners (SPEC-012).
//!
//! This crate reads simulation output and never writes to `BattleState`. Every result is an
//! integer: levels in centibels, distances in millimetres, times in milliseconds. It holds the integer
//! math ([`intmath`]), the rules data ([`AcousticTables`]), propagation, aggregation into
//! clusters and the camera packet.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![cfg_attr(not(test), deny(clippy::float_arithmetic))]

mod aggregate;
pub mod intmath;
mod listener;
mod packet;
pub mod propagation;
mod relief;
mod report;
pub mod scenes;
mod tables;
mod weather;

pub use relief::Relief;
pub use tables::{
    AcousticTables, BandRow, Budget, Globals, GroundRow, SourceRow, WeatherPreset, CLASS_COUNT,
};
pub use weather::{to_mm, Weather};

pub use aggregate::{Aggregator, Cluster, SoundEvent};
pub use listener::{render_packet, Listener, ListenerKind, ListenerState};
pub use packet::{packet_hash, BedOut, MixPacket, SourceOut};
pub use report::{hearing_report, report_hash, HearClass, HearingReport};
