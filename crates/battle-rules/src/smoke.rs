//! The smoke field: a density on a grid of square cells. With the environment off (SPEC-005
//! section 8) every shot adds powder smoke to the cell of its muzzle and every tick each cell
//! loses a fixed share. With it on (SPEC-009 section 4) the source scales by the powder yield and
//! a periodic update carries the field with the wind, diffuses it, holds some of it back in
//! cover, decays it faster in rain and counts every loss, so that mass is exactly conserved.
//! Rendering is outside this module.

use crate::{fx_ratio, SmokeParams, TICK_HZ};
use core_math::{Fx, Vec2Fx};

/// Smoke units per gram of powder (placeholder, calibration target CT-005-8).
pub const SMOKE_UNITS_PER_G: u32 = 1;

/// Decay per tick in parts per million of the cell's density (placeholder, CT-005-8).
pub const SMOKE_DECAY_PPM: u32 = 5000;

/// Density at which a shooter's smoke factor reaches 1000 permille (placeholder, CT-005-8; the
/// value SPEC-009 uses for its reference units).
pub const SMOKE_REF_UNITS: u32 = 20_000;

/// Smoke units one shot adds: `powder_mg * SMOKE_UNITS_PER_G / 1000`, a quarter of that for a
/// misfire (both integer divisions).
pub fn shot_units(powder_mg: u32, misfire: bool) -> u32 {
    let full = (u64::from(powder_mg) * u64::from(SMOKE_UNITS_PER_G) / 1000) as u32;
    if misfire {
        full / 4
    } else {
        full
    }
}

/// Smoke density per cell. Cells are row-major, `index = y * width + x`, cell `(0, 0)` at `origin`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SmokeGrid {
    /// Cell side in millimetres.
    pub cell_mm: u32,
    /// Cells across.
    pub width: u32,
    /// Cells down.
    pub height: u32,
    /// Minimum corner in metres.
    pub origin: Vec2Fx,
    /// Smoke units per cell.
    pub density: Vec<u32>,
    /// Smoke units asked for by [`SmokeGrid::add_smoke`] (including the part lost to the cap)
    /// and set by tests; not hashed, it equals `total` plus the three loss counters.
    pub injected: u64,
    /// Smoke units that flowed over the edge of the grid (SPEC-009 section 4).
    pub lost_edge: u64,
    /// Smoke units removed by ceiling decay.
    pub lost_decay: u64,
    /// Smoke units refused by a full cell.
    pub lost_cap: u64,
}

impl SmokeGrid {
    /// A grid with no cells: smoke is ignored and every density reads zero.
    pub fn empty() -> SmokeGrid {
        SmokeGrid {
            cell_mm: 1,
            width: 0,
            height: 0,
            origin: Vec2Fx::ZERO,
            density: Vec::new(),
            injected: 0,
            lost_edge: 0,
            lost_decay: 0,
            lost_cap: 0,
        }
    }

    /// A clear grid of `width` x `height` cells; `None` for a zero cell size or dimension, or
    /// more than 2^26 cells.
    pub fn new(cell_mm: u32, width: u32, height: u32, origin: Vec2Fx) -> Option<SmokeGrid> {
        let cells = u64::from(width) * u64::from(height);
        if cell_mm == 0 || cells == 0 || cells > 1 << 26 {
            return None;
        }
        Some(SmokeGrid {
            cell_mm,
            width,
            height,
            origin,
            density: vec![0; cells as usize],
            injected: 0,
            lost_edge: 0,
            lost_decay: 0,
            lost_cap: 0,
        })
    }

    /// Index of the cell holding `pos`; positions outside take the nearest edge cell. `None`
    /// for an empty grid.
    pub fn cell_index(&self, pos: Vec2Fx) -> Option<usize> {
        if self.density.is_empty() {
            return None;
        }
        let axis = |p: Fx, o: Fx, n: u32| -> usize {
            let mm = i128::from((p - o).raw()) * 1000;
            let c = mm.div_euclid(i128::from(self.cell_mm) << Fx::FRAC_BITS);
            c.clamp(0, i128::from(n) - 1) as usize
        };
        let (x, y) = (
            axis(pos.x, self.origin.x, self.width),
            axis(pos.y, self.origin.y, self.height),
        );
        Some(y * self.width as usize + x)
    }

    /// Adds `units` to the cell holding `pos` (saturating); nothing for an empty grid.
    pub fn add(&mut self, pos: Vec2Fx, units: u32) {
        if let Some(i) = self.cell_index(pos) {
            self.density[i] = self.density[i].saturating_add(units);
        }
    }

    /// Smoke units in the cell holding `pos`; zero for an empty grid.
    pub fn cell_density(&self, pos: Vec2Fx) -> u32 {
        self.cell_index(pos).map_or(0, |i| self.density[i])
    }

    /// Smoke units in the cell that holds `pos` (SPEC-005 amendment A6); zero for an empty grid.
    pub fn density_at(&self, pos: Vec2Fx) -> u32 {
        self.cell_density(pos)
    }

    /// One tick of decay: every cell loses `ceil(d * SMOKE_DECAY_PPM / 10^6)`.
    pub fn decay(&mut self) {
        for d in &mut self.density {
            let loss = (u64::from(*d) * u64::from(SMOKE_DECAY_PPM)).div_ceil(1_000_000);
            *d -= loss as u32;
        }
    }
}

/// Parts per million in one whole.
const PPM: u64 = 1_000_000;

/// The cells to visit in one update: `x0..=x1` by `y0..=y1`.
#[derive(Clone, Copy)]
struct Rect {
    x0: u32,
    x1: u32,
    y0: u32,
    y1: u32,
}

/// What an update does to the cells it visits, fixed by the wind and the constants.
struct Flow {
    cx: u64,
    cy: u64,
    east: bool,
    north: bool,
    diffuse_ppm: u64,
    decay_ppm: u64,
}

impl SmokeGrid {
    /// The environment source (SPEC-009 section 4): adds `powder_mg * yield_pm / 1000` to the
    /// muzzle cell, a misfire `misfire_pm / 1000` of that (floors in this order), saturating at
    /// `cell_cap`; the excess goes to `lost_cap`. Nothing for an empty grid.
    pub fn add_smoke(&mut self, pos: Vec2Fx, powder_mg: u32, misfire: bool, p: &SmokeParams) {
        let Some(i) = self.cell_index(pos) else {
            return;
        };
        let mut units = u64::from(powder_mg) * u64::from(p.yield_pm) / 1000;
        if misfire {
            units = units * u64::from(p.misfire_pm) / 1000;
        }
        let room = u64::from(p.cell_cap.saturating_sub(self.density[i]));
        let added = units.min(room);
        self.density[i] += added as u32;
        self.injected += units;
        self.lost_cap += units - added;
    }

    /// Total smoke in the grid, in smoke units.
    pub fn total(&self) -> u64 {
        self.density.iter().map(|&d| u64::from(d)).sum()
    }

    /// Concentration of the cell holding `pos`: `min(1000, density * 1000 / ref_units)`.
    pub fn smoke_pm(&self, pos: Vec2Fx, ref_units: u32) -> u32 {
        let d = u64::from(self.cell_density(pos));
        (d * 1000 / u64::from(ref_units.max(1))).min(1000) as u32
    }

    /// The centre of cell `(x, y)` in metres.
    fn cell_centre(&self, x: u32, y: u32) -> Vec2Fx {
        let mm = |c: u32| i64::from(c) * i64::from(self.cell_mm) + i64::from(self.cell_mm) / 2;
        Vec2Fx::new(
            self.origin.x + fx_ratio(mm(x), 1000),
            self.origin.y + fx_ratio(mm(y), 1000),
        )
    }

    /// The bounding rectangle of the non-zero cells grown by one cell; `None` when clear.
    fn active_rect(&self) -> Option<Rect> {
        let w = self.width as usize;
        let mut r: Option<Rect> = None;
        for (y, row) in self.density.chunks_exact(w).enumerate() {
            let (Some(a), Some(b)) = (
                row.iter().position(|&d| d != 0),
                row.iter().rposition(|&d| d != 0),
            ) else {
                continue;
            };
            let (a, b, y) = (a as u32, b as u32, y as u32);
            r = Some(r.map_or(
                Rect {
                    x0: a,
                    x1: b,
                    y0: y,
                    y1: y,
                },
                |r| Rect {
                    x0: r.x0.min(a),
                    x1: r.x1.max(b),
                    y0: r.y0,
                    y1: y,
                },
            ));
        }
        r.map(|r| Rect {
            x0: r.x0.saturating_sub(1),
            x1: (r.x1 + 1).min(self.width - 1),
            y0: r.y0.saturating_sub(1),
            y1: (r.y1 + 1).min(self.height - 1),
        })
    }

    /// The periodic update (SPEC-009 section 4), a pure function of the density at its start:
    /// advection by the wind (`wind_*_mm_s`, the air's motion), diffusion to the four
    /// neighbours, retention in cover (`retain_pm` is asked for the centre of each cell, in
    /// permille), rain-scaled ceiling decay and edge loss. Only the bounding rectangle of the
    /// smoke, grown by one cell, is visited; the result equals the full-grid update.
    ///
    /// The loader keeps `4 * diffuse_ppm` plus the wind share within one cell, so no cell gives
    /// away more than it holds.
    pub fn update(
        &mut self,
        p: &SmokeParams,
        wind_x_mm_s: i64,
        wind_y_mm_s: i64,
        rain_pm: i32,
        retain_pm: &dyn Fn(Vec2Fx) -> u32,
    ) {
        self.update_with(
            p,
            wind_x_mm_s,
            wind_y_mm_s,
            rain_pm,
            retain_pm,
            false,
            false,
        );
    }

    /// [`SmokeGrid::update`] with the cells visited in reverse order and/or over the whole grid
    /// (test hook; the result is the same).
    #[doc(hidden)]
    #[allow(clippy::too_many_arguments)]
    pub fn update_with(
        &mut self,
        p: &SmokeParams,
        wind_x_mm_s: i64,
        wind_y_mm_s: i64,
        rain_pm: i32,
        retain_pm: &dyn Fn(Vec2Fx) -> u32,
        reverse: bool,
        full: bool,
    ) {
        if self.density.is_empty() {
            return;
        }
        let rect = if full {
            Some(Rect {
                x0: 0,
                x1: self.width - 1,
                y0: 0,
                y1: self.height - 1,
            })
        } else {
            self.active_rect()
        };
        let Some(r) = rect else {
            return;
        };
        let share = |v: i64| {
            v.unsigned_abs() * u64::from(p.period_ticks) * PPM
                / (u64::from(TICK_HZ) * u64::from(self.cell_mm))
        };
        let rain = u64::try_from(rain_pm.max(0)).unwrap_or(0);
        let rain_scale = 1000 + rain * u64::from(p.rain_decay_gain_pm) / 1000;
        let flow = Flow {
            cx: share(wind_x_mm_s),
            cy: share(wind_y_mm_s),
            east: wind_x_mm_s > 0,
            north: wind_y_mm_s > 0,
            diffuse_ppm: u64::from(p.diffuse_ppm),
            decay_ppm: (u64::from(p.decay_ppm) * rain_scale / 1000).min(PPM),
        };
        let (rw, rh) = ((r.x1 - r.x0 + 1) as usize, (r.y1 - r.y0 + 1) as usize);
        let mut dst = vec![0u64; rw * rh];
        let mut lost_edge = 0u64;
        let cells = rw * rh;
        for k in 0..cells {
            let k = if reverse { cells - 1 - k } else { k };
            let (x, y) = (r.x0 + (k % rw) as u32, r.y0 + (k / rw) as u32);
            let d = u64::from(self.density[(y * self.width + x) as usize]);
            if d == 0 {
                continue;
            }
            let out_pm = 1000 - u64::from(retain_pm(self.cell_centre(x, y)).min(1000));
            let fx = d * flow.cx * out_pm / 1_000_000_000;
            let fy = d * flow.cy * out_pm / 1_000_000_000;
            let fd = d * flow.diffuse_ppm * out_pm / 1_000_000_000;
            let at = |dst: &mut Vec<u64>, nx: i64, ny: i64, v: u64, edge: &mut u64| {
                let inside = (0..i64::from(self.width)).contains(&nx)
                    && (0..i64::from(self.height)).contains(&ny);
                if inside {
                    let (ix, iy) = (nx as u32 - r.x0, ny as u32 - r.y0);
                    dst[iy as usize * rw + ix as usize] += v;
                } else {
                    *edge += v;
                }
            };
            let (xi, yi) = (i64::from(x), i64::from(y));
            dst[(y - r.y0) as usize * rw + (x - r.x0) as usize] += d - fx - fy - 4 * fd;
            at(
                &mut dst,
                xi + if flow.east { 1 } else { -1 },
                yi,
                fx,
                &mut lost_edge,
            );
            at(
                &mut dst,
                xi,
                yi + if flow.north { 1 } else { -1 },
                fy,
                &mut lost_edge,
            );
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                at(&mut dst, xi + dx, yi + dy, fd, &mut lost_edge);
            }
        }
        let mut lost_decay = 0u64;
        for (k, v) in dst.iter().enumerate() {
            let loss = (v * flow.decay_ppm).div_ceil(PPM);
            lost_decay += loss;
            let (x, y) = (r.x0 + (k % rw) as u32, r.y0 + (k / rw) as u32);
            self.density[(y * self.width + x) as usize] =
                u32::try_from(v - loss).unwrap_or(u32::MAX);
        }
        self.lost_edge += lost_edge;
        self.lost_decay += lost_decay;
    }
}
