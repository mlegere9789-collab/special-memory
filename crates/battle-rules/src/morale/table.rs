//! The morale table (SPEC-006 section 3): `data/rules/morale.toml`, schema `morale/1`.

use crate::ammo::check_schema;
use crate::{parse_table, DataError, Provenance, Table, Training};

/// The state of a regiment's morale.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MoraleKind {
    /// Holding and firing normally.
    Steady,
    /// Showing strain; fire degrades as cohesion falls.
    Wavering,
    /// Retiring in order.
    FallingBack,
    /// Running.
    Broken,
    /// Recovered and reforming.
    Rallied,
}

impl MoraleKind {
    /// Every state in tag order.
    pub const ALL: [MoraleKind; 5] = [
        MoraleKind::Steady,
        MoraleKind::Wavering,
        MoraleKind::FallingBack,
        MoraleKind::Broken,
        MoraleKind::Rallied,
    ];

    /// Stable numeric tag (hash format): steady 0, wavering 1, falling back 2, broken 3,
    /// rallied 4.
    pub const fn tag(self) -> u8 {
        self as u8
    }

    /// Inverse of [`MoraleKind::tag`].
    pub const fn from_tag(tag: u8) -> Option<MoraleKind> {
        match tag {
            0 => Some(MoraleKind::Steady),
            1 => Some(MoraleKind::Wavering),
            2 => Some(MoraleKind::FallingBack),
            3 => Some(MoraleKind::Broken),
            4 => Some(MoraleKind::Rallied),
            _ => None,
        }
    }

    fn from_id(id: &str) -> Option<MoraleKind> {
        match id {
            "steady" => Some(MoraleKind::Steady),
            "wavering" => Some(MoraleKind::Wavering),
            "falling_back" => Some(MoraleKind::FallingBack),
            "broken" => Some(MoraleKind::Broken),
            "rallied" => Some(MoraleKind::Rallied),
            _ => None,
        }
    }
}

/// Cohesion parameters of one training level.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MoraleTrainingRow {
    /// The level.
    pub id: Training,
    /// Ceiling of an unburdened regiment, ppm.
    pub base_ppm: u32,
    /// Casualty capacity, permille of the roster that a regiment absorbs per full cohesion.
    pub cap_pm: u32,
    /// Permille of the gap to the ceiling closed per second while steadying.
    pub up_pm: u32,
    /// Permille of the gap closed per second while rallying.
    pub rally_pm: u32,
    /// Permille of absentees returning on rally.
    pub return_pm: u32,
    /// Neighbour-shock multiplier, permille.
    pub nbr_pm: u32,
    /// Provenance of the row.
    pub provenance: Provenance,
}

/// Wavering values at the fall threshold; the row's own values hold at the steady threshold.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SymptomRamp {
    /// Fire rate at the fall threshold, permille.
    pub fire_end_pm: u32,
    /// Shake factor at the fall threshold, permille.
    pub shake_end_pm: u32,
    /// Look-back rate at the fall threshold, permille per second.
    pub look_end_pm: u32,
    /// Skulk rate at the fall threshold, ppm per second.
    pub skulk_end_ppm: u32,
    /// Cohesion below which skulking begins, ppm.
    pub skulk_from_ppm: u32,
}

/// What soldiers show in one morale state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SymptomRow {
    /// The state.
    pub id: MoraleKind,
    /// Fire rate (loading progress), permille.
    pub fire_pm: u32,
    /// Aim-spread factor, permille.
    pub shake_pm: u32,
    /// Chance per second a soldier looks back, permille.
    pub look_pm: u32,
    /// Chance per second a soldier leaves the ranks, ppm.
    pub skulk_ppm: u32,
    /// The ramp to the fall threshold; present for `Wavering` only.
    pub ramp: Option<SymptomRamp>,
    /// Provenance of the row.
    pub provenance: Provenance,
}

/// Conversion of a range-checked integer into a field type.
trait Narrow {
    fn narrow(v: i64) -> Self;
}

impl Narrow for u32 {
    fn narrow(v: i64) -> u32 {
        // Every caller has range-checked `v` already; the saturation is never reached.
        u32::try_from(v).unwrap_or(u32::MAX)
    }
}

impl Narrow for i64 {
    fn narrow(v: i64) -> i64 {
        v
    }
}

fn narrow<T: Narrow>(v: i64) -> T {
    T::narrow(v)
}

/// Declares [`MoraleGlobals`] and its loader from one list: field, type, allowed range, doc. A
/// trailing underscore on a field name (`break_`, since `break` is a keyword) is not part of its
/// key in the file.
macro_rules! globals {
    ($($name:ident: $ty:ty = $lo:expr, $hi:expr, $doc:literal;)*) => {
        /// Table-wide constants of `[globals]`. Unsuffixed fields are ppm unless the doc says
        /// otherwise; lengths are millimetres and durations ticks.
        #[derive(Clone, Debug, PartialEq, Eq)]
        pub struct MoraleGlobals {
            $(#[doc = $doc] pub $name: $ty,)*
            /// Provenance of the table.
            pub provenance: Provenance,
        }

        impl MoraleGlobals {
            fn load(root: &mut Table) -> Result<MoraleGlobals, DataError> {
                let mut g = root.take_table("globals")?;
                let out = MoraleGlobals {
                    $($name: narrow(g.take_int_in(stringify!($name).trim_end_matches('_'), $lo, $hi)?),)*
                    provenance: g.take_provenance()?,
                };
                g.finish()?;
                Ok(out)
            }
        }
    };
}

const PPM: i64 = 1_000_000;
const PM: i64 = 10_000;
const LEN: i64 = 100_000_000;
const TICKS: i64 = 1_000_000;

globals! {
    waver: u32 = 0, PPM, "Steady becomes Wavering below this cohesion.";
    steady: u32 = 0, PPM, "Wavering becomes Steady at or above this cohesion.";
    fall: u32 = 0, PPM, "Wavering becomes FallingBack below this cohesion.";
    fall_exit: u32 = 0, PPM, "FallingBack may rally at or above this cohesion.";
    break_: u32 = 0, PPM, "FallingBack becomes Broken below this cohesion.";
    rally: u32 = 0, PPM, "Broken may rally at or above this cohesion.";
    nerve_spread: u32 = 0, PPM, "Half-width of the per-regiment threshold offset.";
    delay: u32 = 0, PPM, "Chance per second that a downward step is allowed.";
    down_pm: u32 = 0, PM, "Permille of a negative gap applied per second.";
    perm_pm: u32 = 0, PM, "Permille of a shock that is permanent loss.";
    w_killed_pm: u32 = 0, PM, "Shock weight of a killed soldier, permille.";
    w_wounded_pm: u32 = 0, PM, "Shock weight of a wounded soldier, permille.";
    w_disabled_pm: u32 = 0, PM, "Shock weight of a disabled soldier, permille.";
    w_officer_pm: u32 = 0, PM, "Shock weight of an officer, permille.";
    w_colonel_pm: u32 = 0, PM, "Shock weight of the colonel, permille.";
    calm_pm: u32 = 0, PM, "Shock reduction at full leader strength, permille.";
    broken_ceiling: u32 = 0, PPM, "Ceiling cap while Broken.";
    repeat_pen: u32 = 0, PPM, "Ceiling penalty per earlier break (at most three).";
    fatigue_pen: u32 = 0, PPM, "Ceiling penalty at full fatigue.";
    fatigue_knee: u32 = 0, PPM - 1, "Mean fatigue below which there is no penalty.";
    ammo_pen: u32 = 0, PPM, "Ceiling penalty with no ammunition.";
    ammo_knee_pm: u32 = 1, PM, "Ammunition share, permille, above which there is no penalty.";
    supply_pen: u32 = 0, PPM, "Ceiling penalty with no supply.";
    flank_pen: u32 = 0, PPM, "Ceiling penalty at full flank exposure.";
    flank_f_pm: u32 = 0, PM, "Exposure coefficient of a flank enemy, permille.";
    enfilade_f_pm: u32 = 0, PM, "Exposure coefficient of an enfilade enemy, permille.";
    rear_f_pm: u32 = 0, PM, "Exposure coefficient of a rear enemy, permille.";
    charge_pen: u32 = 0, PPM, "Ceiling penalty from a full-strength charge.";
    disorder_pen: u32 = 0, PPM, "Ceiling penalty at full line disorder.";
    cover_bonus: u32 = 0, PPM, "Ceiling bonus at full cover.";
    wall_bonus: u32 = 0, PPM, "Ceiling bonus behind a wall.";
    leader_bonus: u32 = 0, PPM, "Ceiling bonus at full leader prestige.";
    nbr_shock: u32 = 0, PPM, "Shock to a friendly neighbour of a Broke event.";
    nbr_fall_pm: u32 = 0, PM, "Scale of the neighbour shock for a Fell event, permille.";
    enemy_boost: u32 = 0, PPM, "Boost to an enemy neighbour of a Broke event.";
    rally_leader_pm: u32 = 0, PM, "Rally-rate gain at full leader strength, permille.";
    cause_min: u32 = 0, PPM, "Smallest cause magnitude reported.";
    nbr_radius: i64 = 1, LEN, "Neighbour radius, mm.";
    threat_range: i64 = 1, LEN, "Range within which an enemy counts for exposure, mm.";
    pressure_radius: i64 = 1, LEN, "Enemy distance that prevents recovery, mm.";
    charge_range: i64 = 1, LEN, "Enemy distance at which a charge counts, mm.";
    leader_radius: i64 = 1, LEN, "Distance within which a leader counts, mm.";
    wall_reach: i64 = 0, LEN, "Distance within which a wall counts, mm.";
    skulk_rear: i64 = 0, LEN, "How far behind the line a skulker stands, mm.";
    flight_distance: i64 = 1, LEN, "Distance of a flight order, mm.";
    min_fall: u32 = 0, TICKS, "Least ticks in FallingBack before rallying.";
    min_broken: u32 = 0, TICKS, "Least ticks in Broken before rallying.";
    min_rallied: u32 = 0, TICKS, "Least ticks in Rallied before settling.";
    officerless_rally: u32 = 0, TICKS, "Ticks after which a Broken regiment without officers rallies.";
    dwell: u32 = 0, 65_535, "Ticks an indicator improvement is held back.";
}

fn kind_of(row: &mut Table) -> Result<MoraleKind, DataError> {
    let text = row.take_str("id")?;
    MoraleKind::from_id(&text).ok_or_else(|| {
        DataError::Range(format!("{}.id = {text:?} is not a known state", row.path()))
    })
}

fn level_of(row: &mut Table) -> Result<Training, DataError> {
    let text = row.take_str("id")?;
    Training::ALL
        .into_iter()
        .zip(["raw", "trained", "veteran"])
        .find(|(_, id)| *id == text)
        .map(|(t, _)| t)
        .ok_or_else(|| {
            DataError::Range(format!(
                "{}.id = {text:?} is not a training level",
                row.path()
            ))
        })
}

/// Places `item` at `index` of a table of `N` slots, refusing a repeat.
fn place<T, const N: usize>(
    slots: &mut [Option<T>; N],
    index: usize,
    item: T,
    at: &str,
) -> Result<(), DataError> {
    match slots.get_mut(index) {
        Some(slot @ None) => {
            *slot = Some(item);
            Ok(())
        }
        _ => Err(DataError::Range(format!("{at}: id repeated"))),
    }
}

fn complete<T, const N: usize>(slots: [Option<T>; N], what: &str) -> Result<[T; N], DataError> {
    let all: Vec<T> = slots.into_iter().flatten().collect();
    all.try_into()
        .map_err(|_| DataError::Range(format!("{what}: every id needs one row")))
}

fn check_order(g: &MoraleGlobals) -> Result<(), DataError> {
    let chain = [g.break_, g.fall, g.rally, g.fall_exit, g.waver, g.steady];
    if chain.windows(2).all(|w| w[0] < w[1]) {
        Ok(())
    } else {
        Err(DataError::Range(
            "globals: thresholds must satisfy break < fall < rally < fall_exit < waver < steady"
                .to_owned(),
        ))
    }
}

/// The loaded morale table. Every training level and every state is present exactly once.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MoraleTable {
    /// Table-wide constants.
    pub globals: MoraleGlobals,
    /// Training rows indexed by [`Training::tag`].
    pub training: [MoraleTrainingRow; 3],
    /// Symptom rows indexed by [`MoraleKind::tag`].
    pub symptoms: [SymptomRow; 5],
}

impl MoraleTable {
    /// Parses and validates `morale.toml` text.
    ///
    /// Errors: the shared reader's [`DataError::Float`], [`DataError::UnknownKey`],
    /// [`DataError::Provenance`] and [`DataError::Parse`]; [`DataError::Range`] for a value out of
    /// range, thresholds not strictly ordered, an unknown, repeated or missing id, or a ramp on
    /// any state but Wavering (or none on Wavering).
    pub fn from_toml_str(text: &str) -> Result<MoraleTable, DataError> {
        let mut root = parse_table(text)?;
        check_schema(&mut root, "morale/1")?;

        let mut training: [Option<MoraleTrainingRow>; 3] = [None, None, None];
        for mut row in root.take_rows("training")? {
            let id = level_of(&mut row)?;
            let parsed = MoraleTrainingRow {
                id,
                base_ppm: narrow(row.take_int_in("base_ppm", 0, PPM)?),
                cap_pm: narrow(row.take_int_in("cap_pm", 1, PM)?),
                up_pm: narrow(row.take_int_in("up_pm", 0, PM)?),
                rally_pm: narrow(row.take_int_in("rally_pm", 0, PM)?),
                return_pm: narrow(row.take_int_in("return_pm", 0, PM)?),
                nbr_pm: narrow(row.take_int_in("nbr_pm", 0, PM)?),
                provenance: row.take_provenance()?,
            };
            let at = row.path().to_owned();
            row.finish()?;
            place(&mut training, usize::from(id.tag()), parsed, &at)?;
        }

        let globals = MoraleGlobals::load(&mut root)?;
        check_order(&globals)?;

        let mut symptoms: [Option<SymptomRow>; 5] = [None, None, None, None, None];
        for mut row in root.take_rows("symptom")? {
            let id = kind_of(&mut row)?;
            let fire_pm = narrow(row.take_int_in("fire_pm", 0, PM)?);
            let shake_pm = narrow(row.take_int_in("shake_pm", 0, PM)?);
            let look_pm = narrow(row.take_int_in("look_pm", 0, PM)?);
            let skulk_ppm = narrow(row.take_int_in("skulk_ppm", 0, PPM)?);
            let ramp = take_ramp(&mut row)?;
            let provenance = row.take_provenance()?;
            let at = row.path().to_owned();
            row.finish()?;
            if ramp.is_some() != (id == MoraleKind::Wavering) {
                return Err(DataError::Range(format!(
                    "{at}: only the wavering row carries a ramp, and it must"
                )));
            }
            let parsed = SymptomRow {
                id,
                fire_pm,
                shake_pm,
                look_pm,
                skulk_ppm,
                ramp,
                provenance,
            };
            place(&mut symptoms, usize::from(id.tag()), parsed, &at)?;
        }
        root.finish()?;

        Ok(MoraleTable {
            globals,
            training: complete(training, "training")?,
            symptoms: complete(symptoms, "symptom")?,
        })
    }

    /// Cohesion parameters of a training level.
    pub fn training(&self, level: Training) -> &MoraleTrainingRow {
        &self.training[usize::from(level.tag())]
    }

    /// Symptom values of a morale state.
    pub fn symptom(&self, kind: MoraleKind) -> &SymptomRow {
        &self.symptoms[usize::from(kind.tag())]
    }
}

/// An optional integer key that, when present, must lie in `0..=hi`.
fn opt_in(row: &mut Table, key: &str, hi: i64) -> Result<Option<u32>, DataError> {
    match row.take_int_opt(key)? {
        Some(v) if !(0..=hi).contains(&v) => Err(DataError::Range(format!(
            "{}.{key} = {v} outside 0..={hi}",
            row.path()
        ))),
        other => Ok(other.map(narrow)),
    }
}

/// Reads the five ramp keys: all present, or none.
fn take_ramp(row: &mut Table) -> Result<Option<SymptomRamp>, DataError> {
    let fire_end_pm = opt_in(row, "fire_end_pm", PM)?;
    let shake_end_pm = opt_in(row, "shake_end_pm", PM)?;
    let look_end_pm = opt_in(row, "look_end_pm", PM)?;
    let skulk_end_ppm = opt_in(row, "skulk_end_ppm", PPM)?;
    let skulk_from_ppm = opt_in(row, "skulk_from_ppm", PPM)?;
    match (fire_end_pm, shake_end_pm, look_end_pm, skulk_end_ppm, skulk_from_ppm) {
        (Some(fire_end_pm), Some(shake_end_pm), Some(look_end_pm), Some(skulk_end_ppm), Some(skulk_from_ppm)) => {
            Ok(Some(SymptomRamp {
                fire_end_pm,
                shake_end_pm,
                look_end_pm,
                skulk_end_ppm,
                skulk_from_ppm,
            }))
        }
        (None, None, None, None, None) => Ok(None),
        _ => Err(DataError::Range(format!(
            "{}: a ramp needs all of fire_end_pm, shake_end_pm, look_end_pm, skulk_end_ppm and skulk_from_ppm",
            row.path()
        ))),
    }
}
