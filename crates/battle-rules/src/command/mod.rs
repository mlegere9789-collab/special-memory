//! Command friction and the information model (SPEC-007): couriers, flags and telegraph, the
//! chain of officers, and what each officer knows.
//!
//! The data layer ([`CommandTable`], [`load_officers`], [`trait_mask`]), the state, the media
//! and the arrival step, observation, reports and pictures, and the chain: orders are read, take
//! staff time, are interpreted by the officer's traits and picture ([`interpret`]),
//! acknowledged and passed down ([`BattleState::step_chain`]), with their status derived for
//! the issuer ([`BattleState::order_views`]). The command layer is optional state of a battle
//! (ADR 0006): none of this changes a battle that does not enable it.

mod calibration;
mod chain;
mod codec;
mod data;
mod garble;
mod hash;
mod input;
mod interpret;
mod leg;
mod media;
mod observe;
mod picture;
mod report;
mod state;
mod status;

pub use calibration::run_command_bands;
pub use chain::StaffJob;
pub use codec::{
    decode_dispatch, decode_options, decode_script, encode_dispatch, encode_options, encode_script,
    Dispatch, ScriptEvent,
};
pub use data::{
    load_officers, CommandGlobals, CommandTable, LevelRow, LinkSpec, MediumRow, OfficerSet,
    OfficerSpec, TraitRow,
};
pub use garble::read_order;
pub use hash::write_command_block;
pub use input::{DispatchError, InputError};
pub use interpret::{go_score, interpret, InterpretInput, Interpreted};
pub use media::{words, Arrival, Estimate, Reading};
pub use observe::{OwnVisibility, Visibility};
pub use picture::{ContactView, FriendView, PictureView};
pub use state::{
    AckRecord, CommandOptions, CommandState, Contact, Fate, FriendEntry, GarbleKind, Link, Message,
    MessageId, MessageKind, MessagePayload, ModKind, Officer, OrderBody, OrderRecord, Picture,
    Truth,
};
pub use status::{OrderStatus, OrderView};

use crate::registry::{
    LATITUDE_DISCRETIONARY, LATITUDE_STRICT, LEVEL_ARMY, LEVEL_BRIGADE, LEVEL_CORPS,
    LEVEL_DIVISION, LEVEL_REGIMENT, MEDIUM_COURIER, MEDIUM_FLAGS, MEDIUM_TELEGRAPH, T_AGGRESSIVE,
    T_CAUTIOUS, T_HIGH_DISC, T_LOW_DISC,
};
use crate::{parse_table, DataError};

/// Identifies an officer; ascending ids give the iteration order everywhere.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OfficerId(pub u16);

/// Rank of an officer in `Army > Corps > Division > Brigade > Regiment`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Level {
    /// Level 0.
    Army,
    /// Level 1.
    Corps,
    /// Level 2.
    Division,
    /// Level 3.
    Brigade,
    /// Level 4; owns one regiment.
    Regiment,
}

impl Level {
    /// Stable number (data and hash): army 0 to regiment 4.
    pub const fn tag(self) -> u8 {
        match self {
            Level::Army => LEVEL_ARMY,
            Level::Corps => LEVEL_CORPS,
            Level::Division => LEVEL_DIVISION,
            Level::Brigade => LEVEL_BRIGADE,
            Level::Regiment => LEVEL_REGIMENT,
        }
    }

    /// Inverse of [`Level::tag`].
    pub const fn from_tag(tag: u8) -> Option<Level> {
        match tag {
            LEVEL_ARMY => Some(Level::Army),
            LEVEL_CORPS => Some(Level::Corps),
            LEVEL_DIVISION => Some(Level::Division),
            LEVEL_BRIGADE => Some(Level::Brigade),
            LEVEL_REGIMENT => Some(Level::Regiment),
            _ => None,
        }
    }
}

/// How a message travels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Medium {
    /// A rider (wire number 1).
    Courier,
    /// Signal flags between two stations in sight of each other (2).
    Flags,
    /// Telegraph along a line (3).
    Telegraph,
}

impl Medium {
    /// Every medium in tag order.
    pub const ALL: [Medium; 3] = [Medium::Courier, Medium::Flags, Medium::Telegraph];

    /// Stable number (wire and hash): courier 1, flags 2, telegraph 3. Zero in a dispatch means
    /// automatic and is not a medium.
    pub const fn tag(self) -> u8 {
        match self {
            Medium::Courier => MEDIUM_COURIER,
            Medium::Flags => MEDIUM_FLAGS,
            Medium::Telegraph => MEDIUM_TELEGRAPH,
        }
    }

    /// Inverse of [`Medium::tag`]; zero and unknown numbers give `None`.
    pub const fn from_tag(tag: u8) -> Option<Medium> {
        match tag {
            MEDIUM_COURIER => Some(Medium::Courier),
            MEDIUM_FLAGS => Some(Medium::Flags),
            MEDIUM_TELEGRAPH => Some(Medium::Telegraph),
            _ => None,
        }
    }
}

/// How much freedom a recipient has in carrying out an order ("if practicable").
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Latitude {
    /// Execute as received (wire number 0).
    Strict,
    /// Interpret by trait and picture (1).
    Discretionary,
}

impl Latitude {
    /// Stable number (wire and hash).
    pub const fn tag(self) -> u8 {
        match self {
            Latitude::Strict => LATITUDE_STRICT,
            Latitude::Discretionary => LATITUDE_DISCRETIONARY,
        }
    }

    /// Inverse of [`Latitude::tag`].
    pub const fn from_tag(tag: u8) -> Option<Latitude> {
        match tag {
            LATITUDE_STRICT => Some(Latitude::Strict),
            LATITUDE_DISCRETIONARY => Some(Latitude::Discretionary),
            _ => None,
        }
    }
}

/// The command traits of an officer as bits: aggressive 1, cautious 2, high discretion 4, low
/// discretion 8 (`registry::T_*`). Zero is the neutral officer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct TraitMask(pub u8);

impl TraitMask {
    /// True when every bit of `other` is set here.
    pub const fn contains(self, other: TraitMask) -> bool {
        self.0 & other.0 == other.0
    }
}

/// Mask of a historical commander record (`data/history/commanders`, TOML text): the bits of the
/// record's `[[traits]]` named `aggressive`, `cautious`, `high_discretion` and `low_discretion`,
/// whatever their confidence. An opposing pair (aggressive with cautious, high with low
/// discretion) cancels; other trait names are ignored. A record with none gives the neutral mask.
///
/// Errors: [`DataError::Float`] for a float token, [`DataError::Parse`] for malformed TOML or a
/// trait entry with no `trait` string.
pub fn trait_mask(record: &str) -> Result<TraitMask, DataError> {
    let mut root = parse_table(record)?;
    let mut bits = 0u8;
    for mut row in root.take_rows_opt("traits")?.unwrap_or_default() {
        bits |= match row.take_str("trait")?.as_str() {
            "aggressive" => T_AGGRESSIVE,
            "cautious" => T_CAUTIOUS,
            "high_discretion" => T_HIGH_DISC,
            "low_discretion" => T_LOW_DISC,
            _ => 0,
        };
    }
    for pair in [T_AGGRESSIVE | T_CAUTIOUS, T_HIGH_DISC | T_LOW_DISC] {
        if bits & pair == pair {
            bits &= !pair;
        }
    }
    Ok(TraitMask(bits))
}
