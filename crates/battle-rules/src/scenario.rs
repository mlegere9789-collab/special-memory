//! Scenario files (SPEC-008 sections 2.3 and 3): the parsed, validated and sorted order of
//! battle, historical rows and weather that a data scenario carries.
//!
//! [`Scenario::from_toml_str`] reads `data/scenarios/<id>.toml` through the shared integer-only
//! reader, applies rules V1 to V12 and returns the value with every row sorted, so file order
//! never matters. [`scenario_hash`] is FNV-1a 64 over a canonical little-endian encoding of
//! that value. Title, date and provenance are prose and are not hashed; every number that
//! reaches the simulation is.

mod hash;
mod load;

use crate::{
    DataError, Formation, Gait, Provenance, RegimentId, TerrainGrid, Training, WeaponTable,
};
use core::fmt;

pub use hash::scenario_hash;

/// Why a scenario was refused. Each variant carries a human-readable location or message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScenarioError {
    /// V1 and V12: a float, an unknown key, a missing or mistyped key, or broken provenance.
    Data(DataError),
    /// V3: a reference that does not resolve.
    Reference(String),
    /// V2: an id (or a unique time) used twice.
    Duplicate(String),
    /// V6, V9, V11 and the field ranges: a number outside its allowed range.
    Range(String),
    /// V5: a time outside the scenario, out of order or outside its evidence window.
    Time(String),
    /// V4: a parent chain that loops or crosses sides.
    Cycle(String),
    /// V7: a strength check whose sum is outside its tolerance.
    Strength(String),
    /// V8: `fidelity_class` above 2.
    Class(String),
    /// V10: terrain rows missing, malformed or not matching their hash.
    Terrain(String),
    /// V8: `f0_cap` above `agent_cap`, or `agent_cap` above 200,000.
    Cap(String),
}

impl fmt::Display for ScenarioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScenarioError::Data(e) => write!(f, "scenario data: {e}"),
            ScenarioError::Reference(m) => write!(f, "scenario reference: {m}"),
            ScenarioError::Duplicate(m) => write!(f, "scenario duplicate: {m}"),
            ScenarioError::Range(m) => write!(f, "scenario range: {m}"),
            ScenarioError::Time(m) => write!(f, "scenario time: {m}"),
            ScenarioError::Cycle(m) => write!(f, "scenario hierarchy: {m}"),
            ScenarioError::Strength(m) => write!(f, "scenario strength: {m}"),
            ScenarioError::Class(m) => write!(f, "scenario class: {m}"),
            ScenarioError::Terrain(m) => write!(f, "scenario terrain: {m}"),
            ScenarioError::Cap(m) => write!(f, "scenario caps: {m}"),
        }
    }
}

impl std::error::Error for ScenarioError {}

impl From<DataError> for ScenarioError {
    fn from(e: DataError) -> ScenarioError {
        ScenarioError::Data(e)
    }
}

/// Which army a unit belongs to; the battle `team` is [`Side::team`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Side {
    /// The Union army, team 0.
    Union,
    /// The Confederate army, team 1.
    Confederate,
}

impl Side {
    /// The `AgentStore.team` value: Union 0, Confederate 1.
    pub const fn team(self) -> u8 {
        self as u8
    }
}

/// What a unit is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Arm {
    /// Foot soldiers.
    Infantry,
    /// Mounted or dismounted cavalry.
    Cavalry,
    /// Guns and crews.
    Battery,
    /// A headquarters: forms the hierarchy and becomes no entity.
    Hq,
}

/// One unit of the order of battle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unit {
    /// Id, `[a-z0-9_-]{1,32}`, unique.
    pub id: String,
    /// Army.
    pub side: Side,
    /// Parent unit id, or empty.
    pub parent: String,
    /// Kind of unit.
    pub arm: Arm,
    /// Soldiers (crew for a battery); 0 for a headquarters.
    pub men: u32,
    /// Guns; at least 1 for a battery, else 0.
    pub guns: u32,
    /// Training level.
    pub training: Training,
    /// Weapon id (a `weapons.toml` row); empty for a headquarters.
    pub weapon: String,
    /// Rounds each soldier carries at the start.
    pub rounds_per_man: u32,
    /// Commander record id (`data/history/commanders`), or empty.
    pub commander: String,
    /// The regiment this unit becomes: ascending over non-headquarters units by id.
    pub regiment: Option<RegimentId>,
    /// Where the numbers come from.
    pub provenance: Provenance,
}

/// A unit entering the map.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Arrival {
    /// Unit id.
    pub unit: String,
    /// Seconds from scenario start.
    pub at_s: u32,
    /// Anchor position, mm.
    pub x_mm: i64,
    /// Anchor position, mm.
    pub y_mm: i64,
    /// Whole degrees counter-clockwise from east, `0..=359`.
    pub heading_deg: u16,
    /// Formation on arrival.
    pub formation: Formation,
    /// Earliest time the evidence allows, seconds.
    pub low_s: u32,
    /// Latest time the evidence allows, seconds.
    pub high_s: u32,
    /// Where the numbers come from.
    pub provenance: Provenance,
}

/// A unit leaving the map.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Departure {
    /// Unit id.
    pub unit: String,
    /// Seconds from scenario start.
    pub at_s: u32,
    /// Where the numbers come from.
    pub provenance: Provenance,
}

/// A check on the strength present at a time (the audit "strengths sum").
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StrengthCheck {
    /// Seconds from scenario start.
    pub at_s: u32,
    /// Army counted.
    pub side: Side,
    /// Expected sum of `men` of present units.
    pub men: u32,
    /// Allowed difference.
    pub tol_men: u32,
    /// Where the numbers come from.
    pub provenance: Provenance,
}

/// A place whose holder is reported.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Objective {
    /// Id, unique.
    pub id: String,
    /// Army that scores it.
    pub side: Side,
    /// Centre, mm.
    pub x_mm: i64,
    /// Centre, mm.
    pub y_mm: i64,
    /// Radius, mm, at least 1.
    pub radius_mm: u32,
    /// Points for the holder.
    pub points: u32,
    /// Where the numbers come from.
    pub provenance: Provenance,
}

/// The order a trigger gives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TriggerOrder {
    /// Stand where you are.
    Hold,
    /// March the anchor to a point (mm) at a gait.
    Move {
        /// Destination, mm.
        x_mm: i64,
        /// Destination, mm.
        y_mm: i64,
        /// Regiment gait.
        gait: Gait,
    },
    /// Change formation.
    Form(Formation),
    /// Turn to a heading in whole degrees.
    Face(u16),
}

/// What a trigger does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TriggerKind {
    /// Issue an order.
    Order(TriggerOrder),
    /// Fire at will at the unit with this id.
    FireMode {
        /// Target unit id (not a headquarters).
        target: String,
    },
    /// Add rounds to every soldier's supply.
    Resupply {
        /// Rounds per soldier.
        rounds: u32,
    },
    /// A marker with no effect on the rules.
    Marker,
}

/// A fixed historical event at a time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Trigger {
    /// Id, unique.
    pub id: String,
    /// Seconds from scenario start.
    pub at_s: u32,
    /// Unit addressed.
    pub unit: String,
    /// The action.
    pub kind: TriggerKind,
    /// Where the numbers come from.
    pub provenance: Provenance,
}

/// One weather row; the last row with `from_s <= t` applies.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WeatherRow {
    /// Seconds from scenario start; the first row is 0.
    pub from_s: u32,
    /// Wind, mm/s east.
    pub wind_x_mm_s: i64,
    /// Wind, mm/s north.
    pub wind_y_mm_s: i64,
    /// Wetness of the air, permille.
    pub wet_pm: u32,
    /// Wetness of the ground, permille.
    pub ground_pm: u32,
    /// Visibility, metres.
    pub visibility_m: u32,
    /// Where the numbers come from.
    pub provenance: Provenance,
}

/// The map rectangle in mm.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapSpec {
    /// Width, mm, `1..=12_000_000`.
    pub width_mm: i64,
    /// Height, mm, `1..=12_000_000`.
    pub height_mm: i64,
    /// West edge, mm.
    pub origin_x_mm: i64,
    /// South edge, mm.
    pub origin_y_mm: i64,
}

/// The terrain grid and what is hashed of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerrainSpec {
    /// Cell size, mm.
    pub cell_mm: i64,
    /// Legend character to class id, ascending by character.
    pub legend: Vec<(char, u8)>,
    /// FNV-1a 64 of the canonical rows text (each row then a newline).
    pub rows_hash: u64,
    /// The grid, row 0 at the south.
    pub grid: TerrainGrid,
}

/// A validated scenario with every row sorted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scenario {
    /// Id, `[a-z0-9_-]{1,32}`.
    pub id: String,
    /// Title (prose, not hashed).
    pub title: String,
    /// Date text (prose, not hashed).
    pub date: String,
    /// Clock second of tick 0, counted from 00:00 on 1863-07-01.
    pub clock_start_s: u32,
    /// Length in seconds.
    pub duration_s: u32,
    /// Ticks per second; always 20.
    pub tick_hz: u32,
    /// Sidecar directory name under `data/scenarios/` (default the id).
    pub family: String,
    /// Finest fidelity class the scenario permits, 0 to 2.
    pub fidelity_class: u8,
    /// Soldiers allowed in class 0.
    pub f0_cap: u32,
    /// Soldiers allowed in classes 0 and 1 together.
    pub agent_cap: u32,
    /// The map rectangle.
    pub map: MapSpec,
    /// Terrain.
    pub terrain: TerrainSpec,
    /// Units ascending by id (bytewise).
    pub units: Vec<Unit>,
    /// Arrivals ascending by `(at_s, unit)`.
    pub arrivals: Vec<Arrival>,
    /// Departures ascending by `(at_s, unit)`.
    pub departures: Vec<Departure>,
    /// Strength checks ascending by `(at_s, side)`.
    pub strength_checks: Vec<StrengthCheck>,
    /// Objectives ascending by id.
    pub objectives: Vec<Objective>,
    /// Triggers ascending by `(at_s, id)`.
    pub triggers: Vec<Trigger>,
    /// Weather rows ascending by `from_s`.
    pub weather: Vec<WeatherRow>,
}

impl Scenario {
    /// Parses, validates and sorts a scenario file. `terrain_rows` is the text of the file named
    /// by `[terrain] source` (one row per line), required exactly when `source` is given.
    ///
    /// Errors: see [`ScenarioError`]; rules V1 to V12 of SPEC-008 section 3.
    pub fn from_toml_str(
        text: &str,
        tables: &crate::Tables,
        terrain_rows: Option<&str>,
    ) -> Result<Scenario, ScenarioError> {
        load::load(text, tables, terrain_rows)
    }

    /// The `[terrain] source` path (`terrain/<name>.rows`, relative to `data/scenarios`) that
    /// `text` names, or `None` for inline rows. A caller reads that file, then calls
    /// [`Scenario::from_toml_str`] with its text.
    ///
    /// Errors: [`ScenarioError::Data`] when the text is not a scenario table.
    pub fn terrain_source(text: &str) -> Result<Option<String>, ScenarioError> {
        load::terrain_source(text)
    }

    /// Checks that every non-headquarters unit's weapon is a row of `weapons` (V3). The weapon
    /// table is not part of [`crate::Tables`], so this is a separate call.
    ///
    /// Errors: [`ScenarioError::Reference`] naming the first unit with an unknown weapon.
    pub fn check_weapons(&self, weapons: &WeaponTable) -> Result<(), ScenarioError> {
        for u in self.units.iter().filter(|u| u.arm != Arm::Hq) {
            if weapons.get(&u.weapon).is_none() {
                return Err(ScenarioError::Reference(format!(
                    "unit {} names unknown weapon {:?}",
                    u.id, u.weapon
                )));
            }
        }
        Ok(())
    }

    /// The unit with this id.
    pub fn unit(&self, id: &str) -> Option<&Unit> {
        self.units
            .binary_search_by(|u| u.id.as_str().cmp(id))
            .ok()
            .map(|i| &self.units[i])
    }
}
