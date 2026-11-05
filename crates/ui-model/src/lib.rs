//! Interface model (SPEC-015): the read-only snapshot, the commands that become input records,
//! the display-field registry and the pause and speed state. The crate reads a battle through
//! shared references only; `commands.rs` is the one file that builds inputs for it. Integers and
//! `Fx` only, ascending-id iteration, no clock.

#![forbid(unsafe_code)]

mod commands;
mod confidence;
mod difficulty;
mod display;
mod frame;
mod hash;
mod morale;
mod snapshot;
mod source;
mod state;
mod status;

pub use commands::{Route, UiCommand};
pub use confidence::{confidence_band, confidence_pm, ConfBand, ConfidenceParams};
pub use difficulty::Difficulty;
pub use display::{display_fields, DisplayField, FieldKind};
pub use frame::{soldier_frame, SoldierFrame};
pub use hash::snapshot_hash;
pub use morale::{morale_view, FactorBand, LevelInfo, MoraleView, MoraleViewParams};
pub use snapshot::{capture, EnemyView, InfoMode, RegimentView, Snapshot};
pub use source::{FullInfo, InfoSource, MoraleReport, MoraleSource, OrderRecord, Sighting};
pub use state::{UiAction, UiError, UiMode, UiState, LIVE_SPEED_CAP, SPEEDS};
pub use status::{derive_status, order_view, OrderStatus, OrderView, StatusParams};
