//! Sector states (SPEC-008 section 7, states only).
//!
//! The map is cut into square sectors of `sector_mm`, indexed `sy * width + sx`. At each
//! boundary [`SectorMap::update`] marks a sector **Hot** when it, or one within the Chebyshev
//! margin, overlaps the box of a present class 0 or 1 regiment, **Warm** when it overlaps a
//! class 2 regiment, else **Cold**. A Hot sector leaves Hot only while its smoke total is 0. The
//! states are memory-layout advice only (ADR 0010 D-01): the terrain grid and the smoke grid stay
//! whole-map and authoritative, so no state changes a result.

use crate::fidelity::anchor_mm;
use crate::{FidelityClass, FidelityTable, MapSpec, F2};
use core_math::Vec2Fx;

/// How much of the simulation a sector holds in memory.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SectorState {
    /// No present regiment nearby.
    Cold,
    /// Overlapped by an aggregate (class 2) regiment only.
    Warm,
    /// Near a class 0 or 1 regiment, or still holding smoke.
    Hot,
}

impl SectorState {
    /// Stable numeric tag (hash format): cold 0, warm 1, hot 2.
    pub const fn tag(self) -> u8 {
        self as u8
    }
}

/// A present regiment as the sectors see it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SectorBody {
    /// Anchor, metres.
    pub anchor: Vec2Fx,
    /// Reach of the formation, mm; the box is the anchor plus and minus it.
    pub reach_mm: u32,
    /// Class after classification.
    pub class: FidelityClass,
}

/// The sector-aligned box of Hot sectors: inclusive sector coordinates and the same box in map
/// millimetres (world coordinates, half open at the high ends, clipped to the map).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HotWindow {
    /// West-most Hot sector column.
    pub sx0: u32,
    /// South-most Hot sector row.
    pub sy0: u32,
    /// East-most Hot sector column.
    pub sx1: u32,
    /// North-most Hot sector row.
    pub sy1: u32,
    /// West edge, mm.
    pub x0_mm: i64,
    /// South edge, mm.
    pub y0_mm: i64,
    /// East edge, mm.
    pub x1_mm: i64,
    /// North edge, mm.
    pub y1_mm: i64,
}

/// Per-sector states of a map.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SectorMap {
    map: MapSpec,
    sector_mm: i64,
    margin: i64,
    width: u32,
    height: u32,
    states: Vec<SectorState>,
}

fn count(extent_mm: i64, sector_mm: i64) -> u32 {
    u32::try_from((extent_mm + sector_mm - 1) / sector_mm).unwrap_or(u32::MAX)
}

impl SectorMap {
    /// A map of all-Cold sectors, sized from `map` and the table's `sector_mm`.
    pub fn new(map: &MapSpec, table: &FidelityTable) -> SectorMap {
        let sector_mm = i64::from(table.globals.sector_mm);
        let (width, height) = (
            count(map.width_mm, sector_mm),
            count(map.height_mm, sector_mm),
        );
        SectorMap {
            map: *map,
            sector_mm,
            margin: i64::from(table.globals.hot_margin_sectors),
            width,
            height,
            states: vec![SectorState::Cold; width as usize * height as usize],
        }
    }

    /// Sectors across.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Sectors up.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Index of sector `(sx, sy)`: `sy * width + sx`.
    pub fn index(&self, sx: u32, sy: u32) -> usize {
        sy as usize * self.width as usize + sx as usize
    }

    /// State of the sector at `index`; an index off the map is Cold.
    pub fn state(&self, index: usize) -> SectorState {
        self.states.get(index).copied().unwrap_or(SectorState::Cold)
    }

    /// Every state in index order (the order a hash writes them).
    pub fn states(&self) -> &[SectorState] {
        &self.states
    }

    /// Inclusive sector range `(lo, hi)` along one axis overlapped by the closed box
    /// `[centre - reach, centre + reach]` (map-relative mm), clamped to `0..count`.
    fn span(&self, centre: i128, reach: i128, count: u32) -> (i64, i64) {
        let last = i128::from(count) - 1;
        let sector = i128::from(self.sector_mm);
        let lo = (centre - reach).div_euclid(sector).clamp(0, last);
        let hi = (centre + reach).div_euclid(sector).clamp(0, last);
        (
            i64::try_from(lo).unwrap_or(0),
            i64::try_from(hi).unwrap_or(0),
        )
    }

    /// Recomputes every state at a boundary from the present regiments.
    ///
    /// `smoke_total(index)` is the smoke total of a sector (any unit, zero meaning clear); it is
    /// asked only for sectors that were Hot, would not be Hot now, and so would leave.
    pub fn update(&mut self, bodies: &[SectorBody], smoke_total: impl Fn(usize) -> u64) {
        let (w, h) = (i64::from(self.width), i64::from(self.height));
        let mut next = vec![SectorState::Cold; self.states.len()];
        let at = |sx: i64, sy: i64| (sy * w + sx) as usize;
        for b in bodies {
            let (x, y) = anchor_mm(b.anchor);
            let (x, y) = (
                x - i128::from(self.map.origin_x_mm),
                y - i128::from(self.map.origin_y_mm),
            );
            let reach = i128::from(b.reach_mm);
            let ((x0, x1), (y0, y1)) = (
                self.span(x, reach, self.width),
                self.span(y, reach, self.height),
            );
            if b.class >= F2 {
                for sy in y0..=y1 {
                    for sx in x0..=x1 {
                        let cell = &mut next[at(sx, sy)];
                        *cell = (*cell).max(SectorState::Warm);
                    }
                }
            } else {
                let m = self.margin;
                for sy in (y0 - m).max(0)..=(y1 + m).min(h - 1) {
                    for sx in (x0 - m).max(0)..=(x1 + m).min(w - 1) {
                        next[at(sx, sy)] = SectorState::Hot;
                    }
                }
            }
        }
        for (i, cell) in next.iter_mut().enumerate() {
            if *cell != SectorState::Hot && self.states[i] == SectorState::Hot && smoke_total(i) > 0
            {
                *cell = SectorState::Hot;
            }
        }
        self.states = next;
    }

    /// The sector-aligned box of all Hot sectors, or `None` when none is Hot.
    pub fn hot_window(&self) -> Option<HotWindow> {
        let mut win: Option<(u32, u32, u32, u32)> = None;
        for (i, s) in self.states.iter().enumerate() {
            if *s != SectorState::Hot {
                continue;
            }
            let (sx, sy) = (
                u32::try_from(i % self.width as usize).unwrap_or(0),
                u32::try_from(i / self.width as usize).unwrap_or(0),
            );
            win = Some(match win {
                None => (sx, sy, sx, sy),
                Some((a, b, c, d)) => (a.min(sx), b.min(sy), c.max(sx), d.max(sy)),
            });
        }
        win.map(|(sx0, sy0, sx1, sy1)| HotWindow {
            sx0,
            sy0,
            sx1,
            sy1,
            x0_mm: self.map.origin_x_mm + i64::from(sx0) * self.sector_mm,
            y0_mm: self.map.origin_y_mm + i64::from(sy0) * self.sector_mm,
            x1_mm: self.map.origin_x_mm
                + ((i64::from(sx1) + 1) * self.sector_mm).min(self.map.width_mm),
            y1_mm: self.map.origin_y_mm
                + ((i64::from(sy1) + 1) * self.sector_mm).min(self.map.height_mm),
        })
    }
}
