//! Relief and line of sight (SPEC-009 section 5): the elevation grid with bilinear ground
//! height, the sight query with relief, obstacle and smoke terms, eye and aim heights from
//! posture, and signal quality. Heights are millimetres above local ground; ballistics keep the
//! flat datum of SPEC-005 (decision D-02), so relief changes sight only.

use crate::{
    exp_table, fx_ratio, DamageTable, DataError, ObstacleLayer, Posture, SightParams, SmokeGrid,
    SmokeParams, WeatherNow,
};
use core_math::{Fx, Vec2Fx};

/// Largest cell count of an elevation grid (as the smoke grid).
const MAX_CELLS: usize = 1 << 26;
/// Largest height magnitude of a cell, in decimetres.
const MAX_DM: i64 = 10_000;
/// Share of the posture height that a target is aimed at, permille.
const AIM_PM: i32 = 600;

/// Ground heights on the terrain cell grid. Cells are row-major, `index = y * width + x`, row 0
/// at the south; a cell's height is the ground height at its centre.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ElevationGrid {
    /// Cell side in millimetres.
    pub cell_mm: u32,
    /// Cells across.
    pub width: u32,
    /// Cells down.
    pub height: u32,
    /// Minimum corner in metres.
    pub origin: Vec2Fx,
    /// Ground height of each cell centre in millimetres.
    pub h_mm: Vec<i32>,
}

/// Millimetres in a position length: `floor(raw * 1000 / 2^32)`.
pub fn to_mm(v: Fx) -> i64 {
    (i128::from(v.raw()) * 1000).div_euclid(1 << Fx::FRAC_BITS) as i64
}

/// Cell indices and the weight (in `den` units) between two neighbouring cell centres along one
/// axis; positions beyond the outermost centres take the edge value.
fn axis(d: Fx, n: u32, den: i128) -> (usize, usize, i128) {
    let u = i128::from(d.raw()) * 1000 - den / 2;
    let last = n as usize - 1;
    if u <= 0 || last == 0 {
        return (0, 0, 0);
    }
    let i = u.div_euclid(den);
    if i >= last as i128 {
        return (last, last, 0);
    }
    (i as usize, i as usize + 1, u - i * den)
}

impl ElevationGrid {
    /// A scenario without relief: flat ground at height 0 everywhere.
    pub fn flat() -> ElevationGrid {
        ElevationGrid {
            cell_mm: 1,
            width: 0,
            height: 0,
            origin: Vec2Fx::ZERO,
            h_mm: Vec::new(),
        }
    }

    /// Builds a grid from rows of integer decimetres, row 0 at the south, starting at
    /// `(origin_x_mm, origin_y_mm)`.
    ///
    /// Errors: [`DataError::Range`] for a cell size outside 1 to 1,000,000 mm, a height beyond
    /// 10,000 decimetres or too many cells; [`DataError::Parse`] for no cells or unequal rows.
    pub fn from_rows<R: AsRef<[i32]>>(
        cell_mm: i64,
        origin_x_mm: i64,
        origin_y_mm: i64,
        rows: &[R],
    ) -> Result<ElevationGrid, DataError> {
        if !(1..=1_000_000).contains(&cell_mm) {
            return Err(DataError::Range(format!(
                "relief cell_mm = {cell_mm} outside 1..=1000000"
            )));
        }
        let width = rows.first().map_or(0, |r| r.as_ref().len());
        if rows.is_empty() || width == 0 {
            return Err(DataError::Parse("relief has no cells".to_owned()));
        }
        if rows.len().saturating_mul(width) > MAX_CELLS {
            return Err(DataError::Range("relief has too many cells".to_owned()));
        }
        let mut h_mm = Vec::with_capacity(rows.len() * width);
        for (y, row) in rows.iter().enumerate() {
            if row.as_ref().len() != width {
                return Err(DataError::Parse(format!(
                    "relief row {y} has a different length"
                )));
            }
            for &dm in row.as_ref() {
                if i64::from(dm).abs() > MAX_DM {
                    return Err(DataError::Range(format!(
                        "relief row {y}: {dm} decimetres is beyond {MAX_DM}"
                    )));
                }
                h_mm.push(dm * 100);
            }
        }
        Ok(ElevationGrid {
            cell_mm: cell_mm as u32,
            width: width as u32,
            height: rows.len() as u32,
            origin: Vec2Fx::new(fx_ratio(origin_x_mm, 1000), fx_ratio(origin_y_mm, 1000)),
            h_mm,
        })
    }

    /// Ground height at `p` in millimetres: bilinear between the four nearest cell centres in
    /// exact integers (`i128`, floor division), clamped at the edges. 0 for a flat grid.
    pub fn ground_mm(&self, p: Vec2Fx) -> i32 {
        if self.h_mm.is_empty() {
            return 0;
        }
        let den = i128::from(self.cell_mm) << Fx::FRAC_BITS;
        let (x0, x1, fx) = axis(p.x - self.origin.x, self.width, den);
        let (y0, y1, fy) = axis(p.y - self.origin.y, self.height, den);
        let w = self.width as usize;
        let h = |x: usize, y: usize| i128::from(self.h_mm[y * w + x]);
        let south = h(x0, y0) * (den - fx) + h(x1, y0) * fx;
        let north = h(x0, y1) * (den - fx) + h(x1, y1) * fx;
        ((south * (den - fy) + north * fy).div_euclid(den * den)) as i32
    }
}

/// Everything a sight query reads: the three layers and the thresholds of `[sight]` and `[smoke]`.
#[derive(Clone, Copy, Debug)]
pub struct SightWorld<'a> {
    /// Ground heights.
    pub relief: &'a ElevationGrid,
    /// Walls and earthworks.
    pub obstacles: &'a ObstacleLayer,
    /// Smoke density.
    pub smoke: &'a SmokeGrid,
    /// Extinction constant of the optical depth (`[smoke] ext_k`).
    pub ext_k: u32,
    /// Clearance below which a ray is blocked, mm (`[sight] clear_margin_mm`).
    pub clear_margin_mm: u32,
    /// Transmission below which a target is not visible, permille (`[sight] min_transmit_pm`).
    pub min_transmit_pm: u32,
}

impl<'a> SightWorld<'a> {
    /// The world with its constants taken from the loaded tables.
    pub fn new(
        relief: &'a ElevationGrid,
        obstacles: &'a ObstacleLayer,
        smoke: &'a SmokeGrid,
        smoke_params: &SmokeParams,
        sight_params: &SightParams,
    ) -> SightWorld<'a> {
        SightWorld {
            relief,
            obstacles,
            smoke,
            ext_k: smoke_params.ext_k,
            clear_margin_mm: sight_params.clear_margin_mm,
            min_transmit_pm: sight_params.min_transmit_pm,
        }
    }
}

/// The answer of a sight query.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sight {
    /// Smallest height of the ray above the ground or an obstacle top, mm (`i64::MAX` when
    /// nothing was sampled).
    pub clearance_mm: i64,
    /// Share of light that gets through the smoke, permille.
    pub transmit_pm: u16,
    /// Clear of the ground and obstacles by the margin, and not hidden by smoke.
    pub visible: bool,
}

fn isqrt(n: u128) -> u128 {
    if n < 2 {
        return n;
    }
    let mut x = 1u128 << (128 - n.leading_zeros()).div_ceil(2);
    loop {
        let y = (x + n / x) >> 1;
        if y >= x {
            return x;
        }
        x = y;
    }
}

/// Distance between two positions in millimetres: `isqrt(dx^2 + dy^2)` of the floored axis
/// differences (taken on magnitudes, so the answer does not depend on the order).
pub fn distance_mm(a: Vec2Fx, b: Vec2Fx) -> i64 {
    let dx = i128::from(to_mm((b.x - a.x).abs()));
    let dy = i128::from(to_mm((b.y - a.y).abs()));
    isqrt((dx * dx + dy * dy) as u128) as i64
}

fn raw2(v: Vec2Fx) -> (i128, i128) {
    (i128::from(v.x.raw()), i128::from(v.y.raw()))
}

fn cross(a: (i128, i128), b: (i128, i128)) -> i128 {
    a.0 * b.1 - a.1 * b.0
}

/// The segment of a query: start, raw direction, length in mm.
struct Ray {
    a: Vec2Fx,
    d: (i128, i128),
    len: i128,
}

impl Ray {
    /// `a + d * s / len`, truncating.
    fn at(&self, s: i128) -> Vec2Fx {
        let f = |p: Fx, d: i128| Fx::from_raw((i128::from(p.raw()) + d * s / self.len) as i64);
        Vec2Fx::new(f(self.a.x, self.d.0), f(self.a.y, self.d.1))
    }
}

/// Samples of one query: the smallest relief clearance and the optical depth.
struct Scan {
    clearance: i64,
    tau_micro: u64,
}

fn scan(w: &SightWorld<'_>, a: Vec2Fx, za: i64, b: Vec2Fx, zb: i64, step_mm: u32) -> Scan {
    let len = i128::from(distance_mm(a, b));
    let mut out = Scan {
        clearance: i64::MAX,
        tau_micro: 0,
    };
    if len == 0 {
        return out;
    }
    let ray = Ray {
        a,
        d: raw2(b - a),
        len,
    };
    let step = i128::from(step_mm.max(1));
    let (mut prev, mut s) = (0i128, step);
    loop {
        let end = s.min(len);
        if s < len {
            let z = za + (i128::from(zb - za) * s).div_euclid(len) as i64;
            let p = ray.at(s);
            out.clearance = out.clearance.min(z - i64::from(w.relief.ground_mm(p)));
        }
        let mid = ray.at((prev + end) / 2);
        let density = w.smoke.cell_index(mid).map_or(0, |i| w.smoke.density[i]);
        let part = u64::from(density) * (end - prev) as u64 * u64::from(w.ext_k) / 1000;
        out.tau_micro = out.tau_micro.saturating_add(part);
        if end >= len {
            return out;
        }
        (prev, s) = (end, s + step);
    }
}

/// Smallest clearance of the ray over every obstacle segment it crosses (endpoints inclusive,
/// exact `i128` cross products), `i64::MAX` when it crosses none.
fn obstacle_clearance(w: &SightWorld<'_>, a: Vec2Fx, za: i64, b: Vec2Fx, zb: i64) -> i64 {
    let r = raw2(b - a);
    let mut best = i64::MAX;
    for o in &w.obstacles.segments {
        let (a0, e) = (raw2(o.a - a), raw2(o.b - o.a));
        let (mut den, mut tn, mut un) = (cross(r, e), cross(a0, e), cross(a0, r));
        if den == 0 {
            continue;
        }
        if den < 0 {
            (den, tn, un) = (-den, -tn, -un);
        }
        if tn < 0 || tn > den || un < 0 || un > den {
            continue;
        }
        let z = za + (i128::from(zb - za) * tn).div_euclid(den) as i64;
        let t_q32 = (tn << Fx::FRAC_BITS) / den;
        let f = |p: Fx, d: i128| {
            Fx::from_raw((i128::from(p.raw()) + ((d * t_q32) >> Fx::FRAC_BITS)) as i64)
        };
        let x = Vec2Fx::new(f(a.x, r.0), f(a.y, r.1));
        let top = i64::from(w.relief.ground_mm(x)) + i64::from(o.height_mm);
        best = best.min(z - top);
    }
    best
}

fn end_heights(w: &SightWorld<'_>, a: Vec2Fx, eye_a: i32, b: Vec2Fx, eye_b: i32) -> (i64, i64) {
    (
        i64::from(w.relief.ground_mm(a)) + i64::from(eye_a),
        i64::from(w.relief.ground_mm(b)) + i64::from(eye_b),
    )
}

/// Optical depth of the smoke between two points times 10^6, sampled every `step_mm`
/// (SPEC-009 section 5 term 3): per interval `density(cell of its midpoint) * length * ext_k /
/// 1000`, the last interval cut at the target.
pub fn optical_depth_micro(w: &SightWorld<'_>, a: Vec2Fx, b: Vec2Fx, step_mm: u32) -> u64 {
    scan(w, a, 0, b, 0, step_mm).tau_micro
}

/// Can `a` (eye `eye_a` mm above its ground) see `b` (aim point `eye_b` mm above its ground)?
/// Samples every `step_mm` along the line; see SPEC-009 section 5 for the three terms.
pub fn sight(
    w: &SightWorld<'_>,
    a: Vec2Fx,
    eye_a: i32,
    b: Vec2Fx,
    eye_b: i32,
    step_mm: u32,
) -> Sight {
    let (za, zb) = end_heights(w, a, eye_a, b, eye_b);
    let s = scan(w, a, za, b, zb, step_mm);
    let clearance_mm = s.clearance.min(obstacle_clearance(w, a, za, b, zb));
    let transmit_pm = (exp_table(s.tau_micro) / 1000) as u16;
    Sight {
        clearance_mm,
        transmit_pm,
        visible: clearance_mm >= i64::from(w.clear_margin_mm)
            && u32::from(transmit_pm) >= w.min_transmit_pm,
    }
}

/// Signal quality between two points: the transmission when the ray has non-negative clearance
/// and the distance is within `clear_vis_mm * light_pm / 1000`, else 0.
pub fn signal_quality_pm(
    w: &SightWorld<'_>,
    a: Vec2Fx,
    eye_a: i32,
    b: Vec2Fx,
    eye_b: i32,
    step_mm: u32,
    weather: &WeatherNow,
) -> u16 {
    let reach = i64::from(weather.clear_vis_mm) * i64::from(weather.light_pm) / 1000;
    if distance_mm(a, b) > reach {
        return 0;
    }
    let s = sight(w, a, eye_a, b, eye_b, step_mm);
    if s.clearance_mm >= 0 {
        s.transmit_pm
    } else {
        0
    }
}

/// Eye (muzzle) height of a shooter by posture, mm above ground.
pub fn eye_mm(damage: &DamageTable, posture: Posture) -> i32 {
    damage.posture(posture).muzzle_height_mm as i32
}

/// Height at which a target of this posture is aimed at: 0.6 of its height (standing 1020 mm).
pub fn aim_mm(damage: &DamageTable, posture: Posture) -> i32 {
    (damage.posture(posture).height_mm as i32) * AIM_PM / 1000
}
