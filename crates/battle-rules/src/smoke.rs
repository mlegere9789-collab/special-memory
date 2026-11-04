//! The smoke source (SPEC-005 section 8): a density field on a grid of square cells. Every shot
//! adds powder smoke to the cell of its muzzle; every tick each cell loses a fixed share.
//! Wind transport and rendering are outside this module.

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

    /// One tick of decay: every cell loses `ceil(d * SMOKE_DECAY_PPM / 10^6)`.
    pub fn decay(&mut self) {
        for d in &mut self.density {
            let loss = (u64::from(*d) * u64::from(SMOKE_DECAY_PPM)).div_ceil(1_000_000);
            *d -= loss as u32;
        }
    }
}
