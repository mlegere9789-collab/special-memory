//! Weapon table (SPEC-005 section 1): `data/rules/weapons.toml`.
//!
//! A weapon names the ammunition it fires (with each one's starting round count), its loading
//! steps in order, and its dispersion, misfire, fouling and range numbers. Steps are authored for
//! trained troops; training scales them at run time.

use crate::ammo::{check_id, check_schema, take_u32};
use crate::{parse_table, AmmoTable, DataError, Provenance, Table};

/// Weapon family.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WeaponClass {
    /// Smoothbore musket.
    Smoothbore,
    /// Rifled musket.
    Rifled,
    /// Carbine.
    Carbine,
    /// Revolver (magazine, cycle time between shots).
    Revolver,
    /// Field gun.
    FieldGun,
}

/// One loading step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadStep {
    /// Step id, unique within the weapon.
    pub id: String,
    /// Duration in ticks for trained troops, at least 1.
    pub ticks: u32,
}

/// One ammunition a weapon fires.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AmmoLoad {
    /// Id of a row of the ammunition table.
    pub id: String,
    /// Rounds carried at the start.
    pub rounds: u16,
}

/// One weapon row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WeaponRow {
    /// Unique id.
    pub id: String,
    /// Family.
    pub class: WeaponClass,
    /// Ammunition fired; the position is the `ammo_sel` of a soldier.
    pub ammo: Vec<AmmoLoad>,
    /// Rounds held before a full reload (1 except for the revolver).
    pub magazine: u32,
    /// Ticks between shots from a loaded magazine (revolver), else 0.
    pub cycle_ticks: u32,
    /// Loading steps in order; after the last the weapon is ready.
    pub load_steps: Vec<LoadStep>,
    /// Index into `load_steps` where a misfire restarts loading.
    pub misfire_restart_step: u8,
    /// Dispersion at zero range in micro-radians.
    pub sigma0_urad: u32,
    /// Dispersion added per 100 m of range in micro-radians.
    pub sigma_slope_urad_per_100m: u32,
    /// Scale of the shooter's own aim error in permille.
    pub aim_scale_pm: u32,
    /// Powder per shot in milligrams.
    pub powder_mg: u32,
    /// Base misfire chance in parts per million.
    pub misfire_ppm: u32,
    /// Added misfire chance at full fouling in parts per million.
    pub misfire_fouling_ppm: u32,
    /// Fouling added per shot (cap 1000).
    pub fouling_per_shot: u32,
    /// Extra ramming time at full fouling in permille.
    pub ram_penalty_pm: u32,
    /// Fatigue per shot in parts per million.
    pub fire_drain_ppm: u32,
    /// Effective range in millimetres.
    pub effective_mm: u32,
    /// Extended range in millimetres.
    pub extended_mm: u32,
    /// Distance at which a projectile ends, in millimetres.
    pub reach_mm: u32,
    /// Row provenance.
    pub provenance: Provenance,
}

impl WeaponRow {
    /// Sum of the step durations in ticks (trained).
    pub fn load_sum_ticks(&self) -> u32 {
        self.load_steps.iter().map(|s| s.ticks).sum()
    }

    /// Index of the step with id `ram`, if the weapon has one.
    pub fn ram_step(&self) -> Option<usize> {
        self.load_steps.iter().position(|s| s.id == "ram")
    }

    /// Starting rounds of the ammunition at position `ammo_sel`.
    pub fn rounds_for(&self, ammo_sel: usize) -> Option<u16> {
        self.ammo.get(ammo_sel).map(|a| a.rounds)
    }
}

/// The loaded weapon table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WeaponTable {
    rows: Vec<WeaponRow>,
}

fn load_row(mut row: Table) -> Result<WeaponRow, DataError> {
    let id = row.take_str("id")?;
    check_id(row.path(), &id)?;
    let class = match row.take_str("class")?.as_str() {
        "smoothbore" => WeaponClass::Smoothbore,
        "rifled" => WeaponClass::Rifled,
        "carbine" => WeaponClass::Carbine,
        "revolver" => WeaponClass::Revolver,
        "field_gun" => WeaponClass::FieldGun,
        other => {
            return Err(DataError::Parse(format!(
                "{}.class: unknown class {other:?}",
                row.path()
            )))
        }
    };
    let mut ammo: Vec<AmmoLoad> = Vec::new();
    for mut a in row.take_rows("ammo")? {
        let aid = a.take_str("id")?;
        check_id(a.path(), &aid)?;
        let rounds = u16::try_from(a.take_int_in("rounds", 1, 65_535)?).unwrap_or(1);
        a.finish()?;
        if ammo.iter().any(|x| x.id == aid) {
            return Err(DataError::Parse(format!("{id}: ammo {aid} listed twice")));
        }
        ammo.push(AmmoLoad { id: aid, rounds });
    }
    if ammo.is_empty() {
        return Err(DataError::Parse(format!("{id}: ammo list is empty")));
    }
    let magazine = take_u32(&mut row, "magazine", 1, 255)?;
    let cycle_ticks = take_u32(&mut row, "cycle_ticks", 0, 1_000_000)?;
    let mut load_steps: Vec<LoadStep> = Vec::new();
    for mut s in row.take_rows("load_steps")? {
        let sid = s.take_str("id")?;
        check_id(s.path(), &sid)?;
        let ticks = take_u32(&mut s, "ticks", 1, 100_000)?;
        s.finish()?;
        if load_steps.iter().any(|x| x.id == sid) {
            return Err(DataError::Parse(format!("{id}: step {sid} listed twice")));
        }
        load_steps.push(LoadStep { id: sid, ticks });
    }
    if load_steps.is_empty() || load_steps.len() > 16 {
        return Err(DataError::Range(format!(
            "{id}: load_steps needs 1..=16 steps, found {}",
            load_steps.len()
        )));
    }
    let restart_id = row.take_str("misfire_restart_step")?;
    let misfire_restart_step = load_steps
        .iter()
        .position(|s| s.id == restart_id)
        .and_then(|i| u8::try_from(i).ok())
        .ok_or_else(|| {
            DataError::Parse(format!(
                "{id}: misfire_restart_step {restart_id:?} is not a step"
            ))
        })?;
    let sigma0_urad = take_u32(&mut row, "sigma0_urad", 0, 1_000_000)?;
    let sigma_slope_urad_per_100m = take_u32(&mut row, "sigma_slope_urad_per_100m", 0, 1_000_000)?;
    let aim_scale_pm = take_u32(&mut row, "aim_scale_pm", 0, 10_000)?;
    let powder_mg = take_u32(&mut row, "powder_mg", 0, 10_000_000)?;
    let misfire_ppm = take_u32(&mut row, "misfire_ppm", 0, 1_000_000)?;
    let misfire_fouling_ppm = take_u32(&mut row, "misfire_fouling_ppm", 0, 1_000_000)?;
    let fouling_per_shot = take_u32(&mut row, "fouling_per_shot", 0, 1000)?;
    let ram_penalty_pm = take_u32(&mut row, "ram_penalty_pm", 0, 100_000)?;
    let fire_drain_ppm = take_u32(&mut row, "fire_drain_ppm", 0, 1_000_000)?;
    let effective_mm = take_u32(&mut row, "effective_mm", 1, 100_000_000)?;
    let extended_mm = take_u32(&mut row, "extended_mm", 1, 100_000_000)?;
    let reach_mm = take_u32(&mut row, "reach_mm", 1, 200_000_000)?;
    let provenance = row.take_provenance()?;
    row.finish()?;
    if effective_mm > extended_mm || extended_mm > reach_mm {
        return Err(DataError::Range(format!(
            "{id}: need effective_mm <= extended_mm <= reach_mm"
        )));
    }
    if (class == WeaponClass::Revolver) != (magazine > 1) {
        return Err(DataError::Range(format!(
            "{id}: only a revolver holds more than one round"
        )));
    }
    if magazine > 1 && cycle_ticks == 0 {
        return Err(DataError::Range(format!(
            "{id}: a magazine weapon needs cycle_ticks"
        )));
    }
    Ok(WeaponRow {
        id,
        class,
        ammo,
        magazine,
        cycle_ticks,
        load_steps,
        misfire_restart_step,
        sigma0_urad,
        sigma_slope_urad_per_100m,
        aim_scale_pm,
        powder_mg,
        misfire_ppm,
        misfire_fouling_ppm,
        fouling_per_shot,
        ram_penalty_pm,
        fire_drain_ppm,
        effective_mm,
        extended_mm,
        reach_mm,
        provenance,
    })
}

impl WeaponTable {
    /// Parses and validates `data/rules/weapons.toml` text. Ammunition ids are checked against
    /// an ammunition table separately by [`WeaponTable::check_ammo`].
    pub fn from_toml_str(text: &str) -> Result<WeaponTable, DataError> {
        let mut root = parse_table(text)?;
        check_schema(&mut root, "weapons/1")?;
        let mut rows: Vec<WeaponRow> = Vec::new();
        for row in root.take_rows("weapon")? {
            let r = load_row(row)?;
            if rows.iter().any(|x| x.id == r.id) {
                return Err(DataError::Parse(format!("duplicate weapon id {}", r.id)));
            }
            rows.push(r);
        }
        root.finish()?;
        if rows.is_empty() || rows.len() > 255 {
            return Err(DataError::Range(format!(
                "weapon needs 1..=255 rows, found {}",
                rows.len()
            )));
        }
        Ok(WeaponTable { rows })
    }

    /// Checks that every ammunition id of every weapon names a row of `ammo`.
    pub fn check_ammo(&self, ammo: &AmmoTable) -> Result<(), DataError> {
        for w in &self.rows {
            for a in &w.ammo {
                if ammo.get(&a.id).is_none() {
                    return Err(DataError::Parse(format!(
                        "weapon {} fires unknown ammunition {}",
                        w.id, a.id
                    )));
                }
            }
        }
        Ok(())
    }

    /// All rows in file order.
    pub fn rows(&self) -> &[WeaponRow] {
        &self.rows
    }

    /// The row with this id.
    pub fn get(&self, id: &str) -> Option<&WeaponRow> {
        self.rows.iter().find(|r| r.id == id)
    }

    /// The index of the row with this id.
    pub fn index_of(&self, id: &str) -> Option<usize> {
        self.rows.iter().position(|r| r.id == id)
    }
}
