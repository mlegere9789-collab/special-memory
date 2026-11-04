//! Battle AI (SPEC-010). This skeleton holds the data layer: the AI tables ([`AiTables`]) and the
//! personality a commander record yields ([`personality_for`]). The layers, the message
//! interface and the lookahead arrive in later tickets.
//!
//! State is integers only; iteration is ascending id; no `HashMap`; no wall-clock.

#![forbid(unsafe_code)]

mod personality;
mod tables;

pub use personality::{personality_for, Personality};
pub use tables::{
    AiGlobals, AiTables, Bounds, Difficulty, DifficultyRow, RelationFact, RelationKind, Trait,
    TraitFact, TraitRow,
};
