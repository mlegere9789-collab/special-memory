//! Range tables (SPEC-005 section 3): the elevation and time of flight that bring a projectile
//! of one ammunition to the aim height at a given horizontal range.
//!
//! Built at load by 24-step bisection of the elevation in `[0, 15 degrees]` with the section 2
//! integrator, no wind and no ricochet. The projectile leaves from the aim height
//! ([`AIM_HEIGHT_MM`]) so a table entry is the elevation above the line of sight to a target
//! at that height.

use crate::flight::{substep, Kinematics};
use crate::{fx_ratio, AmmoRow, Vec3Fx};
use core_math::{Angle, Fx, Vec2Fx};

/// Aim height in millimetres: the height the projectile must reach at the target.
pub const AIM_HEIGHT_MM: u32 = 1200;

/// Distance between entries in metres.
pub const STEP_M: i32 = 25;

/// Bisection steps.
const BISECTION_STEPS: u32 = 24;

/// Longest flight simulated per probe, in sub-steps (100 s).
const MAX_SUBSTEPS: u32 = 20_000;

/// An entry counts as solved when it crosses the aim height within this many millimetres.
const SOLVED_TOLERANCE_MM: i64 = 25;

/// Raw size of one entry interval.
const STEP_RAW: i64 = (STEP_M as i64) << Fx::FRAC_BITS;

/// Elevation and time of flight against horizontal range for one ammunition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RangeTable {
    /// `(elevation, time of flight in sub-steps)` for range `index * 25 m`.
    entries: Vec<(Angle, u32)>,
    /// Number of leading entries that reach the aim height; later ones repeat the last of them.
    solved: usize,
}

/// Flies at `elevation` (no wind) until the horizontal range `range` and returns the height
/// there and the time of flight in sub-steps, both interpolated to the crossing. `None` if the
/// projectile reaches the ground, or the step limit, first.
fn probe(ammo: &AmmoRow, gravity: Fx, speed: Fx, elevation: Angle, range: Fx) -> Option<(Fx, Fx)> {
    let (c, s) = (elevation.cos(), elevation.sin());
    let aim = fx_ratio(i64::from(AIM_HEIGHT_MM), 1000);
    let mut st = Kinematics {
        pos: Vec3Fx::new(Fx::ZERO, Fx::ZERO, aim),
        vel: Vec3Fx::new(c * speed, Fx::ZERO, s * speed),
    };
    for n in 0..MAX_SUBSTEPS {
        let next = substep(st, ammo.drag_k, Vec2Fx::ZERO, gravity);
        if next.pos.z <= Fx::ZERO {
            return None;
        }
        if next.pos.x >= range {
            let f = (range - st.pos.x) / (next.pos.x - st.pos.x);
            let z = st.pos.z + (next.pos.z - st.pos.z) * f;
            return Some((z, Fx::from_int(n as i32) + f));
        }
        st = next;
    }
    None
}

impl RangeTable {
    /// Builds the table for `ammo` with entries at 0, 25, 50 ... metres up to `reach_mm`.
    pub fn build(ammo: &AmmoRow, gravity: Fx, reach_mm: u32) -> RangeTable {
        let speed = fx_ratio(i64::from(ammo.muzzle_mm_s), 1000);
        let aim = fx_ratio(i64::from(AIM_HEIGHT_MM), 1000);
        let count = (reach_mm / (STEP_M as u32 * 1000)) as usize + 1;
        let max_elev = Angle::from_degrees_int(15).raw();
        let mut entries: Vec<(Angle, u32)> = vec![(Angle::ZERO, 0)];
        let mut solved = 1;
        for i in 1..count {
            let range = Fx::from_int(STEP_M * i32::try_from(i).unwrap_or(i32::MAX));
            let mut lo = 0;
            let mut hi = max_elev;
            for _ in 0..BISECTION_STEPS {
                let mid = lo + (hi - lo) / 2;
                match probe(ammo, gravity, speed, Angle::from_raw(mid), range) {
                    Some((z, _)) if z >= aim => hi = mid,
                    _ => lo = mid,
                }
            }
            let elev = Angle::from_raw(lo + (hi - lo) / 2);
            let ok = probe(ammo, gravity, speed, elev, range).filter(|(z, _)| {
                i64::from(((*z - aim).abs() * Fx::from_int(1000)).to_int_round_half_up())
                    <= SOLVED_TOLERANCE_MM
            });
            match ok {
                Some((_, tof)) if solved == i => {
                    entries.push((elev, u32::try_from(tof.to_int_round_half_up()).unwrap_or(0)));
                    solved += 1;
                }
                _ => {
                    let last = entries[entries.len() - 1];
                    entries.push(last);
                }
            }
        }
        RangeTable { entries, solved }
    }

    /// Number of entries (ranges 0, 25 m, ...).
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when the table has no entries (never the case for a built table).
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Horizontal range in metres up to which entries reach the aim height; beyond it the
    /// table holds the largest solved elevation.
    pub fn solved_range(&self) -> Fx {
        Fx::from_int(STEP_M * i32::try_from(self.solved - 1).unwrap_or(0))
    }

    /// The entry at index `i` as `(elevation, time of flight in sub-steps)`.
    pub fn entry(&self, i: usize) -> Option<(Angle, u32)> {
        self.entries.get(i).copied()
    }

    /// Locates `range` (metres) between two entries: lower index, upper index and the raw
    /// distance past the lower entry. Negative ranges clamp to the first entry, ranges beyond
    /// the table to the last.
    fn locate(&self, range: Fx) -> (usize, usize, i64) {
        let last = self.entries.len() - 1;
        let raw = range.raw().max(0);
        let idx = usize::try_from(raw / STEP_RAW).unwrap_or(usize::MAX);
        if idx >= last {
            (last, last, 0)
        } else {
            (idx, idx + 1, raw % STEP_RAW)
        }
    }

    fn lerp(a: i64, b: i64, rem: i64) -> i64 {
        let d = i128::from(b - a) * i128::from(rem) / i128::from(STEP_RAW);
        a + i64::try_from(d).unwrap_or(0)
    }

    /// Elevation for horizontal range `range` in metres, linearly interpolated.
    pub fn elevation(&self, range: Fx) -> Angle {
        let (a, b, rem) = self.locate(range);
        let (ea, eb) = (
            i64::from(self.entries[a].0.raw()),
            i64::from(self.entries[b].0.raw()),
        );
        Angle::from_raw(u32::try_from(Self::lerp(ea, eb, rem)).unwrap_or(0))
    }

    /// Time of flight in sub-steps for horizontal range `range` in metres, linearly
    /// interpolated and rounded down.
    pub fn time_of_flight(&self, range: Fx) -> u32 {
        let (a, b, rem) = self.locate(range);
        let (ta, tb) = (i64::from(self.entries[a].1), i64::from(self.entries[b].1));
        u32::try_from(Self::lerp(ta, tb, rem)).unwrap_or(0)
    }
}
