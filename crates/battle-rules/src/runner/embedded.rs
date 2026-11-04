//! The rules files compiled into the crate, so that [`Battle::from_scenario`](crate::Battle)
//! needs no file access and a scenario cannot meet a different table on another machine.

use crate::{
    AmmoTable, DamageTable, DrillTable, FidelityTable, LocomotionTable, MoraleTable, ScenarioError,
    Tables, TerrainTable, WeaponTable,
};

/// Loads the tables and the fidelity table from the files of `data/rules` as they were when the
/// crate was built; the same files [`Tables::load_dir`] reads.
///
/// Errors: [`ScenarioError::Data`] when a file does not load (only if the data are broken).
pub fn embedded_rules() -> Result<(Tables, FidelityTable), ScenarioError> {
    let ammo = AmmoTable::from_toml_str(include_str!("../../../../data/rules/ammo.toml"))?;
    let weapons = WeaponTable::from_toml_str(include_str!("../../../../data/rules/weapons.toml"))?;
    weapons.check_ammo(&ammo)?;
    let tables = Tables {
        locomotion: LocomotionTable::from_toml_str(include_str!(
            "../../../../data/rules/locomotion.toml"
        ))?,
        terrain: TerrainTable::from_toml_str(include_str!("../../../../data/rules/terrain.toml"))?,
        drill: DrillTable::from_toml_str(include_str!("../../../../data/rules/drill.toml"))?,
        morale: MoraleTable::from_toml_str(include_str!("../../../../data/rules/morale.toml"))?,
        ammo,
        weapons,
        damage: DamageTable::from_toml_str(include_str!("../../../../data/rules/damage.toml"))?,
    };
    let fidelity =
        FidelityTable::from_toml_str(include_str!("../../../../data/rules/fidelity.toml"))?;
    Ok((tables, fidelity))
}
