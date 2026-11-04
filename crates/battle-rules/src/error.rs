//! The error every rules loader returns.

use core::fmt;

/// Why a rules file was refused. Each variant carries a human-readable location or message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DataError {
    /// Malformed TOML, a missing key, a value of the wrong type or an integer beyond `i64`.
    Parse(String),
    /// A floating-point token (rules data are integers only); carries the key path.
    Float(String),
    /// A key the loader does not know; carries the dotted key path.
    UnknownKey(String),
    /// Provenance rule broken (placeholder without calibration target, non-placeholder without
    /// sources, `verified` without second source, bad confidence).
    Provenance(String),
    /// An integer outside its allowed range.
    Range(String),
}

impl fmt::Display for DataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DataError::Parse(m) => write!(f, "parse error: {m}"),
            DataError::Float(p) => write!(f, "floating-point value at {p}"),
            DataError::UnknownKey(p) => write!(f, "unknown key {p}"),
            DataError::Provenance(m) => write!(f, "provenance: {m}"),
            DataError::Range(m) => write!(f, "out of range: {m}"),
        }
    }
}

impl std::error::Error for DataError {}
