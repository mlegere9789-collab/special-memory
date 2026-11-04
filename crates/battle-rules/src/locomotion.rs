//! Locomotion table (SPEC-004 sections 3 and 4): gait speeds and fatigue drain, steering globals
//! and the fatigue speed curve, loaded from `data/rules/locomotion.toml`.

use crate::{fx_ratio, parse_table, DataError, Provenance, Table};
use core_math::Fx;

/// Schema tag the file must carry.
const SCHEMA: &str = "locomotion/1";

/// Upper bound of the fatigue scale: 1.0 in parts per million.
pub const FATIGUE_MAX: u32 = 1_000_000;

/// A marching pace, slowest first (the derived order is the speed order).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Gait {
    /// Common time.
    Walk,
    /// Quick time.
    Quick,
    /// Double-quick time.
    DoubleQuick,
    /// Running.
    Run,
}

impl Gait {
    /// Every gait in speed order.
    pub const ALL: [Gait; 4] = [Gait::Walk, Gait::Quick, Gait::DoubleQuick, Gait::Run];

    /// The id used in `locomotion.toml`.
    pub const fn id(self) -> &'static str {
        match self {
            Gait::Walk => "walk",
            Gait::Quick => "quick",
            Gait::DoubleQuick => "double_quick",
            Gait::Run => "run",
        }
    }

    /// Position in [`Gait::ALL`].
    pub const fn index(self) -> usize {
        self as usize
    }

    fn from_id(id: &str) -> Option<Gait> {
        Gait::ALL.into_iter().find(|g| g.id() == id)
    }
}

/// One row of the gait table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GaitRow {
    /// Which gait this row describes.
    pub gait: Gait,
    /// Ground speed in mm/s on open ground at no fatigue.
    pub speed_mm_s: u32,
    /// Fatigue gained per tick of movement on open ground, in ppm.
    pub drain_ppm_per_tick: u32,
    /// Where the numbers come from.
    pub provenance: Provenance,
}

/// Steering and fatigue-recovery constants shared by every soldier.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocomotionGlobals {
    /// Acceleration limit in mm/s per second.
    pub accel_mm_s2: u32,
    /// Body radius in mm.
    pub body_radius_mm: u32,
    /// Distance below which neighbours push each other apart, in mm.
    pub personal_space_mm: u32,
    /// Peak push speed in mm/s.
    pub avoid_gain_mm_s: u32,
    /// Largest permitted slot error in mm.
    pub max_lag_mm: u32,
    /// Slot error beyond which a rested soldier uses the catch-up gait, in mm.
    pub catchup_mm: u32,
    /// Fatigue below which the catch-up gait is allowed, in ppm.
    pub catchup_fatigue_limit_ppm: u32,
    /// Fatigue recovered per tick of rest, in ppm.
    pub rest_recover_ppm_per_tick: u32,
    /// Applied speed below which a soldier counts as resting, in mm/s.
    pub rest_speed_mm_s: u32,
    /// Where the numbers come from.
    pub provenance: Provenance,
}

/// One breakpoint of the fatigue speed curve.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CurvePoint {
    /// Fatigue in ppm, `0..=1_000_000`.
    pub fatigue_ppm: u32,
    /// Speed multiplier in permille, `0..=1000`.
    pub speed_pm: u32,
    /// Where the numbers come from.
    pub provenance: Provenance,
}

/// The loaded locomotion data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocomotionTable {
    gaits: [GaitRow; 4],
    globals: LocomotionGlobals,
    curve: Vec<CurvePoint>,
    rest_speed: Fx,
}

fn u32_in(t: &mut Table, key: &str, lo: u32, hi: u32) -> Result<u32, DataError> {
    let v = t.take_int_in(key, i64::from(lo), i64::from(hi))?;
    u32::try_from(v).map_err(|_| DataError::Range(format!("{} = {v}", t.path())))
}

fn load_gait(mut t: Table) -> Result<GaitRow, DataError> {
    let id = t.take_str("id")?;
    let gait = Gait::from_id(&id)
        .ok_or_else(|| DataError::Parse(format!("{}.id: unknown gait {id:?}", t.path())))?;
    let speed_mm_s = u32_in(&mut t, "speed_mm_s", 1, u32::MAX)?;
    let drain_ppm_per_tick = u32_in(&mut t, "drain_ppm_per_tick", 0, FATIGUE_MAX)?;
    let provenance = t.take_provenance()?;
    t.finish()?;
    Ok(GaitRow {
        gait,
        speed_mm_s,
        drain_ppm_per_tick,
        provenance,
    })
}

fn load_globals(mut t: Table) -> Result<LocomotionGlobals, DataError> {
    let g = LocomotionGlobals {
        accel_mm_s2: u32_in(&mut t, "accel_mm_s2", 1, u32::MAX)?,
        body_radius_mm: u32_in(&mut t, "body_radius_mm", 0, u32::MAX)?,
        personal_space_mm: u32_in(&mut t, "personal_space_mm", 0, u32::MAX)?,
        avoid_gain_mm_s: u32_in(&mut t, "avoid_gain_mm_s", 0, u32::MAX)?,
        max_lag_mm: u32_in(&mut t, "max_lag_mm", 0, u32::MAX)?,
        catchup_mm: u32_in(&mut t, "catchup_mm", 0, u32::MAX)?,
        catchup_fatigue_limit_ppm: u32_in(&mut t, "catchup_fatigue_limit_ppm", 0, FATIGUE_MAX)?,
        rest_recover_ppm_per_tick: u32_in(&mut t, "rest_recover_ppm_per_tick", 0, FATIGUE_MAX)?,
        rest_speed_mm_s: u32_in(&mut t, "rest_speed_mm_s", 0, u32::MAX)?,
        provenance: t.take_provenance()?,
    };
    t.finish()?;
    Ok(g)
}

fn load_curve(rows: Vec<Table>) -> Result<Vec<CurvePoint>, DataError> {
    let mut curve: Vec<CurvePoint> = Vec::with_capacity(rows.len());
    for mut t in rows {
        let at = t.path().to_owned();
        let fatigue_ppm = u32_in(&mut t, "fatigue_ppm", 0, FATIGUE_MAX)?;
        let speed_pm = u32_in(&mut t, "speed_pm", 0, 1000)?;
        let provenance = t.take_provenance()?;
        t.finish()?;
        match curve.last() {
            None if fatigue_ppm != 0 => {
                return Err(DataError::Range(format!("{at}: the curve must start at 0")))
            }
            Some(prev) if fatigue_ppm <= prev.fatigue_ppm => {
                return Err(DataError::Range(format!(
                    "{at}: fatigue_ppm must be strictly increasing"
                )))
            }
            Some(prev) if speed_pm > prev.speed_pm => {
                return Err(DataError::Range(format!(
                    "{at}: speed_pm must not increase with fatigue"
                )))
            }
            _ => {}
        }
        curve.push(CurvePoint {
            fatigue_ppm,
            speed_pm,
            provenance,
        });
    }
    match curve.last() {
        Some(last) if curve.len() >= 2 && last.fatigue_ppm == FATIGUE_MAX => Ok(curve),
        _ => Err(DataError::Range(
            "fatigue_curve: at least two points, the last at 1000000".to_owned(),
        )),
    }
}

impl LocomotionTable {
    /// Parses and validates `locomotion.toml` text.
    ///
    /// Errors: [`DataError::Float`], [`DataError::UnknownKey`], [`DataError::Provenance`] from the
    /// shared reader; [`DataError::Parse`] for a wrong schema tag or a missing, duplicate or
    /// unknown gait; [`DataError::Range`] for out-of-range numbers, a tick rate other than 20 and
    /// a curve that does not run from 0 to 1,000,000 ppm strictly increasing with non-increasing
    /// speed.
    pub fn from_toml_str(text: &str) -> Result<LocomotionTable, DataError> {
        let mut root = parse_table(text)?;
        let schema = root.take_str("schema")?;
        if schema != SCHEMA {
            return Err(DataError::Parse(format!(
                "schema must be {SCHEMA:?}, found {schema:?}"
            )));
        }
        root.take_int_in(
            "tick_hz",
            i64::from(crate::TICK_HZ),
            i64::from(crate::TICK_HZ),
        )?;
        let mut slots: [Option<GaitRow>; 4] = [None, None, None, None];
        for row in root.take_rows("gait")? {
            let row = load_gait(row)?;
            let slot = &mut slots[row.gait.index()];
            if slot.is_some() {
                return Err(DataError::Parse(format!(
                    "duplicate gait {}",
                    row.gait.id()
                )));
            }
            *slot = Some(row);
        }
        let [walk, quick, double_quick, run] = slots;
        let need = |g: Gait, r: Option<GaitRow>| {
            r.ok_or_else(|| DataError::Parse(format!("missing gait {}", g.id())))
        };
        let gaits = [
            need(Gait::Walk, walk)?,
            need(Gait::Quick, quick)?,
            need(Gait::DoubleQuick, double_quick)?,
            need(Gait::Run, run)?,
        ];
        let globals = load_globals(root.take_table("globals")?)?;
        let curve = load_curve(root.take_rows("fatigue_curve")?)?;
        root.finish()?;
        let rest_speed = fx_ratio(i64::from(globals.rest_speed_mm_s), 1000);
        Ok(LocomotionTable {
            gaits,
            globals,
            curve,
            rest_speed,
        })
    }

    /// The row of one gait.
    pub fn gait(&self, gait: Gait) -> &GaitRow {
        &self.gaits[gait.index()]
    }

    /// All four gait rows in speed order.
    pub fn gaits(&self) -> &[GaitRow; 4] {
        &self.gaits
    }

    /// The shared steering and recovery constants.
    pub fn globals(&self) -> &LocomotionGlobals {
        &self.globals
    }

    /// The fatigue speed curve, ascending in fatigue.
    pub fn curve(&self) -> &[CurvePoint] {
        &self.curve
    }

    /// Open-ground speed of a gait in m/s.
    pub fn speed(&self, gait: Gait) -> Fx {
        fx_ratio(i64::from(self.gait(gait).speed_mm_s), 1000)
    }

    /// The resting threshold in m/s.
    pub fn rest_speed(&self) -> Fx {
        self.rest_speed
    }
}
