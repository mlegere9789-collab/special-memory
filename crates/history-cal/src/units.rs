//! The unit list a script refers to (SPEC-011 section 3, interim form of the scenario `[[unit]]`
//! rows) and the regiment numbering of amendment A1.

use crate::ScriptError;
use battle_rules::{parse_table, DataError, Formation, Table, Training};
use std::collections::BTreeSet;

/// Level of a unit in the hierarchy; `Regiment` and `Battery` are the leaves that become regiments.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    /// An army.
    Army,
    /// A corps.
    Corps,
    /// A division.
    Division,
    /// A brigade.
    Brigade,
    /// A regiment.
    Regiment,
    /// A battery.
    Battery,
}

impl Level {
    /// True for the levels that become a simulated regiment; the others are headquarters.
    pub const fn is_regiment(self) -> bool {
        matches!(self, Level::Regiment | Level::Battery)
    }

    /// Depth in the hierarchy; a parent's depth is below its child's.
    const fn depth(self) -> u8 {
        match self {
            Level::Battery => Level::Regiment as u8,
            other => other as u8,
        }
    }
}

/// One unit of the order of battle a script addresses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitRecord {
    /// Unit id, `^[a-z0-9_]+$`.
    pub id: String,
    /// Hierarchy level.
    pub level: Level,
    /// Parent unit id, empty for none.
    pub parent: String,
    /// Side, 0 or 1.
    pub side: u8,
    /// Men (guns for a battery); 0 for a headquarters.
    pub strength: u16,
    /// Training level.
    pub training: Training,
    /// Formation at arrival.
    pub formation: Formation,
    /// Anchor x in whole metres at arrival (0 for a headquarters, whose anchor is derived).
    pub x_m: i64,
    /// Anchor y in whole metres at arrival.
    pub y_m: i64,
}

fn bad(t: &Table, key: &str, v: &str) -> ScriptError {
    DataError::Parse(format!("{}.{key}: unknown value {v:?}", t.path())).into()
}

pub(crate) fn parse_formation(t: &Table, v: &str) -> Result<Formation, ScriptError> {
    Formation::ALL
        .into_iter()
        .zip(["column", "line", "skirmish"])
        .find_map(|(f, name)| (name == v).then_some(f))
        .ok_or_else(|| bad(t, "formation", v))
}

fn parse_level(t: &Table, v: &str) -> Result<Level, ScriptError> {
    Ok(match v {
        "army" => Level::Army,
        "corps" => Level::Corps,
        "division" => Level::Division,
        "brigade" => Level::Brigade,
        "regiment" => Level::Regiment,
        "battery" => Level::Battery,
        _ => return Err(bad(t, "level", v)),
    })
}

fn parse_training(t: &Table, v: &str) -> Result<Training, ScriptError> {
    Ok(match v {
        "raw" => Training::Raw,
        "trained" => Training::Trained,
        "veteran" => Training::Veteran,
        _ => return Err(bad(t, "training", v)),
    })
}

/// Reads `units.toml` (`schema = "replay-units/1"`, rows `[[unit]]`) into records in ascending id.
///
/// Errors: [`ScriptError::Data`] for TOML, float, unknown key, provenance or range faults;
/// [`ScriptError::Duplicate`] for a repeated id; [`ScriptError::Reference`] for a missing parent or
/// a parent that is not above its child.
pub fn units_from_toml_str(text: &str) -> Result<Vec<UnitRecord>, ScriptError> {
    let mut root = parse_table(text)?;
    if root.take_str("schema")? != "replay-units/1" {
        return Err(DataError::Parse("schema must be replay-units/1".into()).into());
    }
    let mut out = Vec::new();
    for mut t in root.take_rows("unit")? {
        let id = t.take_str("id")?;
        let level_s = t.take_str("level")?;
        let level = parse_level(&t, &level_s)?;
        let parent = t.take_str("parent")?;
        let side = t.take_int_in("side", 0, 1)? as u8;
        let strength = t.take_int_in("strength", 0, i64::from(u16::MAX))? as u16;
        let training_s = t.take_str("training")?;
        let training = parse_training(&t, &training_s)?;
        let formation_s = t.take_str("formation")?;
        let formation = parse_formation(&t, &formation_s)?;
        let x_m = coordinate(&mut t, "x_m")?;
        let y_m = coordinate(&mut t, "y_m")?;
        t.take_provenance()?;
        t.finish()?;
        out.push(UnitRecord {
            id,
            level,
            parent,
            side,
            strength,
            training,
            formation,
            x_m,
            y_m,
        });
    }
    root.finish()?;
    out.sort_by(|a, b| a.id.cmp(&b.id));
    let mut seen = BTreeSet::new();
    for u in &out {
        if !seen.insert(u.id.as_str()) {
            return Err(ScriptError::Duplicate(u.id.clone()));
        }
    }
    for u in &out {
        if u.parent.is_empty() {
            continue;
        }
        match out.binary_search_by(|p| p.id.as_str().cmp(&u.parent)) {
            Ok(i) if out[i].level.depth() < u.level.depth() => {}
            _ => {
                return Err(ScriptError::Reference(format!(
                    "parent {:?} of unit {}",
                    u.parent, u.id
                )))
            }
        }
    }
    Ok(out)
}

/// Largest coordinate magnitude in whole metres (keeps every `Fx` position exact).
pub(crate) const COORD_LIMIT_M: i64 = 1_000_000;

/// An optional coordinate, 0 when absent (a headquarters has no anchor of its own).
fn coordinate(t: &mut Table, key: &str) -> Result<i64, ScriptError> {
    let v = t.take_int_opt(key)?.unwrap_or(0);
    if v.abs() > COORD_LIMIT_M {
        return Err(DataError::Range(format!("{}.{key} = {v}", t.path())).into());
    }
    Ok(v)
}
