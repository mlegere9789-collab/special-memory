//! The rules tables a battle needs (SPEC-004), loaded together.

use crate::{DataError, DrillTable, LocomotionTable, TerrainTable};

/// Every rules table the movement and drill rules read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tables {
    /// Gaits, fatigue and steering constants.
    pub locomotion: LocomotionTable,
    /// Terrain classes.
    pub terrain: TerrainTable,
    /// Formations, training and transition durations.
    pub drill: DrillTable,
}

fn read(dir: &str, name: &str) -> Result<String, DataError> {
    let path = std::path::Path::new(dir).join(name);
    std::fs::read_to_string(&path)
        .map_err(|e| DataError::Parse(format!("cannot read {}: {e}", path.display())))
}

impl Tables {
    /// Loads `locomotion.toml`, `terrain.toml` and `drill.toml` from `dir`.
    ///
    /// Errors: [`DataError::Parse`] when a file cannot be read, otherwise whatever the table's
    /// own loader returns.
    pub fn load_dir(dir: &str) -> Result<Tables, DataError> {
        Ok(Tables {
            locomotion: LocomotionTable::from_toml_str(&read(dir, "locomotion.toml")?)?,
            terrain: TerrainTable::from_toml_str(&read(dir, "terrain.toml")?)?,
            drill: DrillTable::from_toml_str(&read(dir, "drill.toml")?)?,
        })
    }
}
