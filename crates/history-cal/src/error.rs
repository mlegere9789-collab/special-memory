//! The error every history-cal loader returns.

use battle_rules::DataError;
use core::fmt;

/// Why a script, unit list or policy table was refused or could not be compiled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScriptError {
    /// A float, unknown key, provenance fault, out-of-range integer or malformed TOML.
    Data(DataError),
    /// Two rows share an id.
    Duplicate(String),
    /// A row names a unit, trigger or regiment that does not exist.
    Reference(String),
    /// A time outside the script's clock span, or a window that is empty or mis-ordered.
    Time(String),
    /// A tick holds more inputs than the 16-bit sequence number allows (65,535).
    TooManyInputs {
        /// The tick that overflowed.
        tick: u64,
    },
}

impl From<DataError> for ScriptError {
    fn from(e: DataError) -> Self {
        ScriptError::Data(e)
    }
}

impl fmt::Display for ScriptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScriptError::Data(e) => write!(f, "{e}"),
            ScriptError::Duplicate(id) => write!(f, "duplicate id {id}"),
            ScriptError::Reference(m) => write!(f, "unresolved reference: {m}"),
            ScriptError::Time(m) => write!(f, "bad time: {m}"),
            ScriptError::TooManyInputs { tick } => {
                write!(f, "more than 65535 inputs at tick {tick}")
            }
        }
    }
}

impl std::error::Error for ScriptError {}
