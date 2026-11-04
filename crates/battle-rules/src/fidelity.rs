//! Fidelity table and classifier (SPEC-008 sections 2.1 and 4.1).
//!
//! [`FidelityTable`] is `data/rules/fidelity.toml`. [`classify`] is a pure integer function of
//! the present regiments: it picks each regiment's class from its gap to the nearest enemy, with
//! hysteresis and a dwell time against flicker, then applies the soldier caps as strict prefixes
//! in `(gap, id)` order. It reads nothing but its arguments.

use crate::ammo::check_schema;
use crate::{parse_table, Arm, DataError, Provenance, RegimentId, Table};
use core_math::Vec2Fx;

/// A fidelity class: 0 (F0, every projectile), 1 (F1, volley bundles) or 2 (F2, aggregate).
/// Ordered by coarseness: `F0 < F1 < F2`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FidelityClass(pub u8);

/// Class 0: soldiers fire real projectiles.
pub const F0: FidelityClass = FidelityClass(0);
/// Class 1: soldiers fire bundled volleys.
pub const F1: FidelityClass = FidelityClass(1);
/// Class 2: the regiment is one aggregate.
pub const F2: FidelityClass = FidelityClass(2);

/// Table-wide constants of `[globals]`; lengths mm, durations ticks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FidelityGlobals {
    /// Ticks between classification boundaries.
    pub assign_interval_ticks: u32,
    /// Extra distance a regiment keeps its finer class, mm.
    pub hysteresis_mm: u32,
    /// Ticks a class is held before a coarser one applies.
    pub min_dwell_ticks: u32,
    /// Ticks between F1 bundle releases per regiment.
    pub bundle_ticks: u32,
    /// Ticks between F2 exchanges per regiment.
    pub f2_interval_ticks: u32,
    /// Side of a sector, mm.
    pub sector_mm: u32,
    /// Chebyshev margin of Hot sectors around a class 0 or 1 regiment.
    pub hot_margin_sectors: u32,
    /// Agent grid cell size, mm.
    pub grid_cell_mm: u32,
    /// Most agent grid cells before the cell size doubles.
    pub max_grid_cells: u32,
    /// Provenance of the globals.
    pub provenance: Provenance,
}

/// The distance limits of one arm of service, mm.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArmRow {
    /// The arm.
    pub arm: Arm,
    /// Gap up to which a regiment is class 0.
    pub f0_mm: u32,
    /// Gap up to which a regiment is class 1.
    pub f1_mm: u32,
    /// Provenance of the row.
    pub provenance: Provenance,
}

/// The loaded fidelity table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FidelityTable {
    /// Table-wide constants.
    pub globals: FidelityGlobals,
    /// Rows for infantry, cavalry and battery, in that order.
    pub arms: [ArmRow; 3],
}

const LEN: i64 = 12_000_000;
const TICKS: i64 = 6000;

fn slot_of(arm: Arm) -> Option<usize> {
    match arm {
        Arm::Infantry => Some(0),
        Arm::Cavalry => Some(1),
        Arm::Battery => Some(2),
        Arm::Hq => None,
    }
}

fn arm_of(text: &str) -> Option<Arm> {
    match text {
        "infantry" => Some(Arm::Infantry),
        "cavalry" => Some(Arm::Cavalry),
        "battery" => Some(Arm::Battery),
        _ => None,
    }
}

fn narrow(v: i64) -> u32 {
    // Callers range-check first; the saturation is never reached.
    u32::try_from(v).unwrap_or(u32::MAX)
}

fn load_globals(root: &mut Table) -> Result<FidelityGlobals, DataError> {
    let mut g = root.take_table("globals")?;
    let out = FidelityGlobals {
        assign_interval_ticks: narrow(g.take_int_in("assign_interval_ticks", 1, TICKS)?),
        hysteresis_mm: narrow(g.take_int_in("hysteresis_mm", 0, LEN)?),
        min_dwell_ticks: narrow(g.take_int_in("min_dwell_ticks", 0, 1_000_000)?),
        bundle_ticks: narrow(g.take_int_in("bundle_ticks", 1, TICKS)?),
        f2_interval_ticks: narrow(g.take_int_in("f2_interval_ticks", 1, TICKS)?),
        sector_mm: narrow(g.take_int_in("sector_mm", 1000, LEN)?),
        hot_margin_sectors: narrow(g.take_int_in("hot_margin_sectors", 0, 16)?),
        grid_cell_mm: narrow(g.take_int_in("grid_cell_mm", 100, 1_000_000)?),
        max_grid_cells: narrow(g.take_int_in("max_grid_cells", 1, 268_435_456)?),
        provenance: g.take_provenance()?,
    };
    g.finish()?;
    Ok(out)
}

impl FidelityTable {
    /// Parses and validates `fidelity.toml` text.
    ///
    /// Errors: the shared reader's [`DataError::Float`], [`DataError::UnknownKey`],
    /// [`DataError::Provenance`] and [`DataError::Parse`]; [`DataError::Range`] for a value out
    /// of range, `f1_mm <= f0_mm`, an unknown, repeated or missing arm id.
    pub fn from_toml_str(text: &str) -> Result<FidelityTable, DataError> {
        let mut root = parse_table(text)?;
        check_schema(&mut root, "fidelity/1")?;
        let globals = load_globals(&mut root)?;
        let mut slots: [Option<ArmRow>; 3] = [None, None, None];
        for mut row in root.take_rows("arm")? {
            let id = row.take_str("id")?;
            let arm = arm_of(&id).ok_or_else(|| {
                DataError::Range(format!("{}.id = {id:?} is not an arm", row.path()))
            })?;
            let parsed = ArmRow {
                arm,
                f0_mm: narrow(row.take_int_in("f0_mm", 1, LEN)?),
                f1_mm: narrow(row.take_int_in("f1_mm", 1, LEN)?),
                provenance: row.take_provenance()?,
            };
            if parsed.f1_mm <= parsed.f0_mm {
                return Err(DataError::Range(format!(
                    "{}.f1_mm must exceed f0_mm",
                    row.path()
                )));
            }
            let at = row.path().to_owned();
            row.finish()?;
            match slot_of(arm).and_then(|i| slots.get_mut(i)) {
                Some(slot @ None) => *slot = Some(parsed),
                _ => return Err(DataError::Range(format!("{at}: id repeated"))),
            }
        }
        root.finish()?;
        let all: Vec<ArmRow> = slots.into_iter().flatten().collect();
        let arms: [ArmRow; 3] = all
            .try_into()
            .map_err(|_| DataError::Range("arm: every arm needs one row".to_owned()))?;
        Ok(FidelityTable { globals, arms })
    }

    /// The row of `arm`; `None` for a headquarters, which has no class.
    pub fn arm_row(&self, arm: Arm) -> Option<&ArmRow> {
        slot_of(arm).and_then(|i| self.arms.get(i))
    }
}

/// What the classifier reads of one present regiment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassInput {
    /// Regiment id; ties in the caps break by it. Unique in a call.
    pub id: RegimentId,
    /// Side.
    pub team: u8,
    /// Arm of service; a headquarters is always class 2 and takes no part in gaps or caps.
    pub arm: Arm,
    /// Anchor, metres.
    pub anchor: Vec2Fx,
    /// Reach of the formation, mm (SPEC-008 section 1).
    pub reach_mm: u32,
    /// Fit soldiers (what the caps count).
    pub fit: u32,
    /// Class held now.
    pub class: FidelityClass,
    /// Tick at which `class` was set.
    pub since: u64,
}

/// Floor of the square root of `n`, integer-only (Newton iteration).
pub(crate) fn isqrt(n: u128) -> u128 {
    if n < 2 {
        return n;
    }
    let bits = 128 - n.leading_zeros();
    let mut x = 1u128 << bits.div_ceil(2);
    loop {
        let y = (x + n / x) >> 1;
        if y >= x {
            return x;
        }
        x = y;
    }
}

/// Anchor in whole millimetres, flooring (`raw * 1000 >> 32`).
pub(crate) fn anchor_mm(a: Vec2Fx) -> (i128, i128) {
    let f = |raw: i64| (i128::from(raw) * 1000) >> 32;
    (f(a.x.raw()), f(a.y.raw()))
}

/// Gap of every input to its nearest present enemy, mm; `None` when it has no enemy (or is a
/// headquarters). All pairs, so the result does not depend on slice order.
fn gaps(inputs: &[ClassInput]) -> Vec<Option<i128>> {
    let pos: Vec<(i128, i128)> = inputs.iter().map(|r| anchor_mm(r.anchor)).collect();
    let live = |r: &ClassInput| r.arm != Arm::Hq;
    inputs
        .iter()
        .enumerate()
        .map(|(i, r)| {
            if !live(r) {
                return None;
            }
            inputs
                .iter()
                .enumerate()
                .filter(|(_, e)| live(e) && e.team != r.team)
                .map(|(j, e)| {
                    let (dx, dy) = (pos[i].0 - pos[j].0, pos[i].1 - pos[j].1);
                    let d = isqrt((dx * dx + dy * dy).unsigned_abs());
                    let d = i128::try_from(d).unwrap_or(i128::MAX);
                    (d - i128::from(r.reach_mm) - i128::from(e.reach_mm)).max(0)
                })
                .min()
        })
        .collect()
}

/// Applies a cap: walks `order` (indices into `want`, already sorted) accumulating `fit`; the
/// first to exceed `cap` and all after it are set to `to`. A strict prefix.
fn apply_cap(
    want: &mut [FidelityClass],
    inputs: &[ClassInput],
    order: &[usize],
    cap: u32,
    to: FidelityClass,
) {
    let mut total: u64 = 0;
    let mut over = false;
    for &i in order {
        total += u64::from(inputs[i].fit);
        over = over || total > u64::from(cap);
        if over {
            want[i] = to;
        }
    }
}

/// The class of every input (same order as `inputs`) at `tick`. SPEC-008 section 4.1.
///
/// `fc` is the scenario class (a floor), `f0_cap` and `agent_cap` the soldier caps. The result
/// for a regiment does not depend on where it sits in the slice.
pub fn classify(
    table: &FidelityTable,
    inputs: &[ClassInput],
    fc: u8,
    f0_cap: u32,
    agent_cap: u32,
    tick: u64,
) -> Vec<FidelityClass> {
    let gap = gaps(inputs);
    let hyst = i128::from(table.globals.hysteresis_mm);
    let dwell = u64::from(table.globals.min_dwell_ticks);
    let mut want: Vec<FidelityClass> = inputs
        .iter()
        .zip(&gap)
        .map(|(r, g)| {
            let (Some(row), Some(g)) = (table.arm_row(r.arm), g) else {
                return F2;
            };
            let f0 = i128::from(row.f0_mm) + if r.class == F0 { hyst } else { 0 };
            let f1 = i128::from(row.f1_mm) + if r.class <= F1 { hyst } else { 0 };
            let geo = if *g <= f0 {
                F0
            } else if *g <= f1 {
                F1
            } else {
                F2
            };
            // A coarser class waits for the dwell; a finer one applies at once.
            let geo = if geo > r.class && tick.saturating_sub(r.since) < dwell {
                r.class
            } else {
                geo
            };
            geo.max(FidelityClass(fc.min(2)))
        })
        .collect();
    let key = |i: usize| (gap[i].unwrap_or(i128::MAX), inputs[i].id);
    let sorted = |want: &[FidelityClass], limit: FidelityClass| {
        let mut o: Vec<usize> = (0..inputs.len()).filter(|&i| want[i] <= limit).collect();
        o.sort_by_key(|&i| key(i));
        o
    };
    let order = sorted(&want, F0);
    apply_cap(&mut want, inputs, &order, f0_cap, F1);
    let order = sorted(&want, F1);
    apply_cap(&mut want, inputs, &order, agent_cap, F2);
    want
}
