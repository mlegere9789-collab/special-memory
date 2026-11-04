//! The rules tables a battle needs (SPEC-004, SPEC-005), loaded together.

use crate::{
    AmmoTable, DamageTable, DataError, DrillTable, LocomotionTable, MoraleTable, TerrainTable,
    WeaponTable,
};

/// Every rules table the movement, drill and fire rules read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tables {
    /// Gaits, fatigue and steering constants.
    pub locomotion: LocomotionTable,
    /// Terrain classes.
    pub terrain: TerrainTable,
    /// Formations, training and transition durations.
    pub drill: DrillTable,
    /// Cohesion, shock and symptom rows (SPEC-006).
    pub morale: MoraleTable,
    /// Ammunition rows and physical constants.
    pub ammo: AmmoTable,
    /// Weapons.
    pub weapons: WeaponTable,
    /// Postures and damage outcomes.
    pub damage: DamageTable,
}

fn read(dir: &str, name: &str) -> Result<String, DataError> {
    let path = std::path::Path::new(dir).join(name);
    std::fs::read_to_string(&path)
        .map_err(|e| DataError::Parse(format!("cannot read {}: {e}", path.display())))
}

impl Tables {
    /// Loads `locomotion.toml`, `terrain.toml`, `drill.toml`, `morale.toml`, `ammo.toml`, `weapons.toml` and
    /// `damage.toml` from `dir`; every weapon's ammunition must name an ammunition row.
    ///
    /// Errors: [`DataError::Parse`] when a file cannot be read, otherwise whatever the table's
    /// own loader returns.
    pub fn load_dir(dir: &str) -> Result<Tables, DataError> {
        let ammo = AmmoTable::from_toml_str(&read(dir, "ammo.toml")?)?;
        let weapons = WeaponTable::from_toml_str(&read(dir, "weapons.toml")?)?;
        weapons.check_ammo(&ammo)?;
        Ok(Tables {
            locomotion: LocomotionTable::from_toml_str(&read(dir, "locomotion.toml")?)?,
            terrain: TerrainTable::from_toml_str(&read(dir, "terrain.toml")?)?,
            drill: DrillTable::from_toml_str(&read(dir, "drill.toml")?)?,
            morale: MoraleTable::from_toml_str(&read(dir, "morale.toml")?)?,
            ammo,
            weapons,
            damage: DamageTable::from_toml_str(&read(dir, "damage.toml")?)?,
        })
    }
}
