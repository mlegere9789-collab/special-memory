//! Uniform spatial grid built with counting-sort buckets.

use crate::store::{AgentId, AgentStore};
use core_math::{Fx, Vec2Fx};
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

/// Largest number of cells a [`SpatialGrid`] may have (2^26).
pub const MAX_GRID_CELLS: u64 = 1 << 26;

/// Why a [`SpatialGrid`] could not be created.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GridError {
    /// `width_cells` or `height_cells` is zero.
    ZeroDimension,
    /// The cell size is zero or negative.
    NonPositiveCellSize,
    /// `width * height` overflows or exceeds [`MAX_GRID_CELLS`]; `cells` is the product
    /// (`u64::MAX` if the product itself overflowed).
    TooManyCells {
        /// The requested number of cells.
        cells: u64,
    },
}

impl fmt::Display for GridError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GridError::ZeroDimension => write!(f, "grid width and height must be at least 1 cell"),
            GridError::NonPositiveCellSize => write!(f, "grid cell size must be positive"),
            GridError::TooManyCells { cells } => {
                write!(
                    f,
                    "grid of {cells} cells exceeds the limit {MAX_GRID_CELLS}"
                )
            }
        }
    }
}

impl std::error::Error for GridError {}

/// Squared distance between two points as an exact integer in raw units
/// (Q64.64, i.e. the product of two raw Q32.32 values).
///
/// Each axis difference is at most 2^64 - 1 in magnitude, so a single square
/// always fits in `u128`; the sum saturates at `u128::MAX`, which is larger
/// than any legal `radius^2` (at most 2^126).
pub fn distance_sq_raw(a: Vec2Fx, b: Vec2Fx) -> u128 {
    let dx = u128::from(a.x.raw().abs_diff(b.x.raw()));
    let dy = u128::from(a.y.raw().abs_diff(b.y.raw()));
    (dx * dx).saturating_add(dy * dy)
}

/// 256-bit unsigned integer as `(high, low)`; tuple order is numeric order.
type U256 = (u128, u128);

const LOW64: u128 = (1 << 64) - 1;

/// Exact product of two `u128` values.
fn mul_wide(a: u128, b: u128) -> U256 {
    let (a0, a1, b0, b1) = (a & LOW64, a >> 64, b & LOW64, b >> 64);
    let (ll, lh, hl, hh) = (a0 * b0, a0 * b1, a1 * b0, a1 * b1);
    let mid = (ll >> 64) + (lh & LOW64) + (hl & LOW64);
    let hi = hh + (lh >> 64) + (hl >> 64) + (mid >> 64);
    (hi, (ll & LOW64) | ((mid & LOW64) << 64))
}

/// `a + b`; callers keep the sum below 2^256.
fn add_wide(a: U256, b: U256) -> U256 {
    let (lo, carry) = a.1.overflowing_add(b.1);
    (a.0 + b.0 + u128::from(carry), lo)
}

/// `a - b` for `a >= b`.
fn sub_wide(a: U256, b: U256) -> U256 {
    let (lo, borrow) = a.1.overflowing_sub(b.1);
    (a.0 - b.0 - u128::from(borrow), lo)
}

/// A sign and a magnitude: `(negative, |value|)`.
type Signed = (bool, U256);

fn smul(a: i128, b: i128) -> Signed {
    (
        (a < 0) != (b < 0),
        mul_wide(a.unsigned_abs(), b.unsigned_abs()),
    )
}

/// `x + y`, with a zero result always non-negative.
fn ssum(x: Signed, y: Signed) -> Signed {
    if x.0 == y.0 {
        (x.0, add_wide(x.1, y.1))
    } else if x.1 >= y.1 {
        (x.0 && x.1 != y.1, sub_wide(x.1, y.1))
    } else {
        (y.0, sub_wide(y.1, x.1))
    }
}

/// `floor(sqrt(n))` for `n < 2^130`.
fn isqrt_wide(n: U256) -> u128 {
    let (mut lo, mut hi) = (0u128, 1u128 << 65);
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        if mul_wide(mid, mid) <= n {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    lo
}

/// `floor(n / m)` for `m < 2^127` and a quotient below 2^128.
fn div_wide(n: U256, m: u128) -> u128 {
    if n.0 == 0 {
        return n.1 / m;
    }
    let (mut rem, mut q) = (0u128, 0u128);
    for bit in (0..256).rev() {
        let b = if bit >= 128 {
            n.0 >> (bit - 128)
        } else {
            n.1 >> bit
        } & 1;
        rem = (rem << 1) | b;
        q <<= 1;
        if rem >= m {
            rem -= m;
            q |= 1;
        }
    }
    q
}

/// `p0.x + dx * (y - p0.y) / dy` rounded down (`up == false`) or up, for `y` within the
/// segment's vertical extent and `dy != 0`.
fn x_at(p0: (i128, i128), d: (i128, i128), y: i128, up: bool) -> i128 {
    let n = d.0.unsigned_abs() * (y - p0.1).unsigned_abs();
    let den = d.1.unsigned_abs();
    let (q, r) = (n / den, n % den);
    let neg = (d.0 < 0) ^ (y < p0.1) ^ (d.1 < 0);
    let off = if neg {
        -((q + u128::from(!up && r > 0)) as i128)
    } else {
        (q + u128::from(up && r > 0)) as i128
    };
    p0.0 + off
}

/// A capsule query prepared once: exact integer geometry of `p0..p1` and `half_width`.
struct Capsule {
    p0: (i128, i128),
    p1: Vec2Fx,
    p0v: Vec2Fx,
    d: (i128, i128),
    h: i128,
    h2: u128,
    l2: U256,
    len: u128,
}

impl Capsule {
    fn new(p0: Vec2Fx, p1: Vec2Fx, half_width: i64) -> Capsule {
        let a = (i128::from(p0.x.raw()), i128::from(p0.y.raw()));
        let d = (i128::from(p1.x.raw()) - a.0, i128::from(p1.y.raw()) - a.1);
        let l2 = add_wide(smul(d.0, d.0).1, smul(d.1, d.1).1);
        let h = i128::from(half_width);
        Capsule {
            p0: a,
            p1,
            p0v: p0,
            d,
            h,
            h2: (h * h) as u128,
            l2,
            len: isqrt_wide(l2),
        }
    }

    /// `Some(s)` when `pos` is within the half-width of the segment (SPEC-002 amendment A2).
    fn hit(&self, pos: Vec2Fx) -> Option<u128> {
        let w = (
            i128::from(pos.x.raw()) - self.p0.0,
            i128::from(pos.y.raw()) - self.p0.1,
        );
        // Cheap exact rejection against the capsule's bounding box.
        let (dx, dy) = self.d;
        if w.0 < dx.min(0) - self.h
            || w.0 > dx.max(0) + self.h
            || w.1 < dy.min(0) - self.h
            || w.1 > dy.max(0) + self.h
        {
            return None;
        }
        let dot = ssum(smul(w.0, dx), smul(w.1, dy));
        if dot.0 || dot.1 == (0, 0) {
            return (distance_sq_raw(pos, self.p0v) <= self.h2).then_some(0);
        }
        if dot.1 >= self.l2 {
            return (distance_sq_raw(pos, self.p1) <= self.h2).then_some(self.len);
        }
        let (neg, mag) = smul(w.1, dx);
        let cross = ssum(smul(w.0, dy), (!neg, mag));
        if cross.1 .0 != 0 {
            return None;
        }
        let lhs = mul_wide(cross.1 .1, cross.1 .1);
        let rhs = add_wide(mul_wide(self.h2, self.l2.1), (self.h2 * self.l2.0, 0));
        (lhs <= rhs).then(|| div_wide(dot.1, self.len).min(self.len))
    }
}

#[derive(Clone, Copy, Debug)]
struct Entry {
    id: AgentId,
    pos: Vec2Fx,
}

/// A uniform grid over the plane, rebuilt from an [`AgentStore`] each tick.
///
/// Agents outside the grid are clamped into the border cells, so every alive
/// agent is always indexed. Within each cell, agents are stored in ascending
/// slot order, which makes the structure independent of insertion history.
/// Query results are sorted by `(distance_sq, slot index)`.
#[derive(Debug)]
pub struct SpatialGrid {
    cell_size: i64,
    width: u32,
    height: u32,
    origin: Vec2Fx,
    /// `cells + 1` offsets into `entries`.
    cell_start: Vec<u32>,
    entries: Vec<Entry>,
    scratch_cell: Vec<u32>,
    scratch_cursor: Vec<u32>,
    cells_touched: AtomicU64,
}

impl Clone for SpatialGrid {
    fn clone(&self) -> SpatialGrid {
        SpatialGrid {
            cell_size: self.cell_size,
            width: self.width,
            height: self.height,
            origin: self.origin,
            cell_start: self.cell_start.clone(),
            entries: self.entries.clone(),
            scratch_cell: Vec::new(),
            scratch_cursor: Vec::new(),
            cells_touched: AtomicU64::new(self.cells_touched.load(Ordering::Relaxed)),
        }
    }
}

impl SpatialGrid {
    /// Creates an empty grid of `width_cells` x `height_cells` cells of side
    /// `cell_size`, whose minimum corner is `origin`.
    ///
    /// # Errors
    /// Returns [`GridError::ZeroDimension`] for a zero width or height,
    /// [`GridError::NonPositiveCellSize`] for a cell size of zero or less, and
    /// [`GridError::TooManyCells`] when `width * height` overflows (checked) or exceeds
    /// [`MAX_GRID_CELLS`] (2^26).
    pub fn new(
        cell_size: Fx,
        width_cells: u32,
        height_cells: u32,
        origin: Vec2Fx,
    ) -> Result<SpatialGrid, GridError> {
        if width_cells == 0 || height_cells == 0 {
            return Err(GridError::ZeroDimension);
        }
        if cell_size.raw() <= 0 {
            return Err(GridError::NonPositiveCellSize);
        }
        let cells = u64::from(width_cells).saturating_mul(u64::from(height_cells));
        if cells > MAX_GRID_CELLS {
            return Err(GridError::TooManyCells { cells });
        }
        Ok(SpatialGrid {
            cell_size: cell_size.raw(),
            width: width_cells,
            height: height_cells,
            origin,
            cell_start: vec![0; cells as usize + 1],
            entries: Vec::new(),
            scratch_cell: Vec::new(),
            scratch_cursor: Vec::new(),
            cells_touched: AtomicU64::new(0),
        })
    }

    /// Grid width in cells.
    pub fn width_cells(&self) -> u32 {
        self.width
    }

    /// Grid height in cells.
    pub fn height_cells(&self) -> u32 {
        self.height
    }

    /// Number of agents currently indexed.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` if no agent is indexed.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Cell column (or row) for a raw coordinate, clamped to `0..n`.
    fn axis_cell(&self, value: i128, origin: i64, n: u32) -> usize {
        let c = (value - i128::from(origin)).div_euclid(i128::from(self.cell_size));
        c.clamp(0, i128::from(n) - 1) as usize
    }

    fn cell_of(&self, p: Vec2Fx) -> usize {
        let cx = self.axis_cell(i128::from(p.x.raw()), self.origin.x.raw(), self.width);
        let cy = self.axis_cell(i128::from(p.y.raw()), self.origin.y.raw(), self.height);
        cy * self.width as usize + cx
    }

    /// Rebuilds the index from every alive agent in `store` in O(n + cells).
    ///
    /// The result depends only on the alive agents' slots and positions. Queries return a
    /// snapshot of the store as it stood at the last `rebuild`: agents spawned, moved or
    /// despawned afterwards are not reflected until the next rebuild (use
    /// [`SpatialGrid::query_radius_alive`] / [`SpatialGrid::nearest_alive`] to drop ids that are
    /// no longer alive).
    pub fn rebuild(&mut self, store: &AgentStore) {
        let cells = self.width as usize * self.height as usize;
        self.scratch_cell.clear();
        self.cell_start.clear();
        self.cell_start.resize(cells + 1, 0);

        for (i, &alive) in store.alive.iter().enumerate() {
            if alive {
                let c = self.cell_of(store.pos[i]);
                self.scratch_cell.push(c as u32);
                self.cell_start[c + 1] += 1;
            }
        }
        for c in 0..cells {
            self.cell_start[c + 1] += self.cell_start[c];
        }
        self.scratch_cursor.clear();
        self.scratch_cursor
            .extend_from_slice(&self.cell_start[..cells]);

        let dummy = Entry {
            id: AgentId {
                index: 0,
                generation: 0,
            },
            pos: Vec2Fx::default(),
        };
        self.entries.clear();
        self.entries.resize(self.scratch_cell.len(), dummy);

        let mut k = 0usize;
        for (i, &alive) in store.alive.iter().enumerate() {
            if alive {
                let c = self.scratch_cell[k] as usize;
                k += 1;
                let slot = self.scratch_cursor[c] as usize;
                self.scratch_cursor[c] += 1;
                self.entries[slot] = Entry {
                    id: AgentId {
                        index: i as u32,
                        generation: store.generation[i],
                    },
                    pos: store.pos[i],
                };
            }
        }
    }

    /// Inclusive cell ranges `(x0, x1, y0, y1)` covering the circle's bounding box.
    fn range(&self, center: Vec2Fx, radius: i64) -> (usize, usize, usize, usize) {
        let r = i128::from(radius);
        let cx = i128::from(center.x.raw());
        let cy = i128::from(center.y.raw());
        (
            self.axis_cell(cx - r, self.origin.x.raw(), self.width),
            self.axis_cell(cx + r, self.origin.x.raw(), self.width),
            self.axis_cell(cy - r, self.origin.y.raw(), self.height),
            self.axis_cell(cy + r, self.origin.y.raw(), self.height),
        )
    }

    /// Calls `f` for every entry in the cells overlapping the query circle.
    fn for_candidates(&self, center: Vec2Fx, radius: i64, mut f: impl FnMut(&Entry)) {
        let (x0, x1, y0, y1) = self.range(center, radius);
        let w = self.width as usize;
        let mut touched = 0u64;
        for cy in y0..=y1 {
            for cx in x0..=x1 {
                touched += 1;
                let c = cy * w + cx;
                let (s, e) = (self.cell_start[c] as usize, self.cell_start[c + 1] as usize);
                for entry in &self.entries[s..e] {
                    f(entry);
                }
            }
        }
        self.cells_touched.fetch_add(touched, Ordering::Relaxed);
    }

    /// Clears `out` and fills it with every indexed agent whose squared
    /// distance to `center` is at most `radius^2`, sorted by
    /// `(distance_sq, slot index)`. A negative radius yields no agents.
    ///
    /// The result is a snapshot of the last [`SpatialGrid::rebuild`]: it may contain ids that
    /// have been despawned since, and positions are those recorded at the rebuild.
    pub fn query_radius(&self, center: Vec2Fx, radius: Fx, out: &mut Vec<AgentId>) {
        out.clear();
        let r = radius.raw();
        if r < 0 {
            return;
        }
        let r2 = (r as u128) * (r as u128);
        let mut hits: Vec<(u128, u32, AgentId)> = Vec::new();
        self.for_candidates(center, r, |e| {
            let d = distance_sq_raw(e.pos, center);
            if d <= r2 {
                hits.push((d, e.id.index, e.id));
            }
        });
        hits.sort_unstable_by_key(|h| (h.0, h.1));
        out.extend(hits.into_iter().map(|h| h.2));
    }

    /// Calls `f` for every entry in the cells that can hold a point of the capsule: per cell row,
    /// the segment is clipped to the row's band widened by the half-width, and the columns that
    /// clip spans (widened likewise) are scanned. Border cells stand for the clamped region
    /// beyond the grid, so the outermost rows and columns reach to infinity.
    fn for_capsule_candidates(&self, c: &Capsule, mut f: impl FnMut(&Entry)) {
        let oy = i128::from(self.origin.y.raw());
        let cs = i128::from(self.cell_size);
        let (ax, ay) = c.p0;
        let (bx, by) = (ax + c.d.0, ay + c.d.1);
        let (ymin, ymax) = (ay.min(by), ay.max(by));
        let (row0, row1) = (
            self.axis_cell(ymin - c.h, self.origin.y.raw(), self.height),
            self.axis_cell(ymax + c.h, self.origin.y.raw(), self.height),
        );
        let w = self.width as usize;
        let mut touched = 0u64;
        for cy in row0..=row1 {
            let band_lo = if cy == 0 {
                i128::MIN / 2
            } else {
                oy + cy as i128 * cs - c.h
            };
            let band_hi = if cy + 1 == self.height as usize {
                i128::MAX / 2
            } else {
                oy + (cy as i128 + 1) * cs + c.h
            };
            let (ya, yb) = (band_lo.max(ymin), band_hi.min(ymax));
            if ya > yb {
                continue;
            }
            let (xlo, xhi) = if c.d.1 == 0 {
                (ax.min(bx), ax.max(bx))
            } else {
                (
                    x_at(c.p0, c.d, ya, false).min(x_at(c.p0, c.d, yb, false)),
                    x_at(c.p0, c.d, ya, true).max(x_at(c.p0, c.d, yb, true)),
                )
            };
            let c0 = self.axis_cell(xlo - c.h, self.origin.x.raw(), self.width);
            let c1 = self.axis_cell(xhi + c.h, self.origin.x.raw(), self.width);
            touched += (c1 - c0 + 1) as u64;
            for cx in c0..=c1 {
                let i = cy * w + cx;
                let (s, e) = (self.cell_start[i] as usize, self.cell_start[i + 1] as usize);
                for entry in &self.entries[s..e] {
                    f(entry);
                }
            }
        }
        self.cells_touched.fetch_add(touched, Ordering::Relaxed);
    }

    /// Clears `out` and fills it with every indexed agent within `half_width` of the closed
    /// segment `p0..p1`, sorted by `(s, slot index)` where `s` is the raw distance along the
    /// segment of the agent's projection, clamped to the segment (SPEC-002 amendment A2). A
    /// negative `half_width` yields no agents; `p0 == p1` is a circle. Exact integer arithmetic
    /// over the whole `i64` coordinate range; only cells touching the capsule are visited.
    ///
    /// Like [`SpatialGrid::query_radius`], the result is a snapshot of the last
    /// [`SpatialGrid::rebuild`].
    pub fn query_segment(&self, p0: Vec2Fx, p1: Vec2Fx, half_width: Fx, out: &mut Vec<AgentId>) {
        out.clear();
        if half_width.raw() < 0 {
            return;
        }
        let capsule = Capsule::new(p0, p1, half_width.raw());
        let mut hits: Vec<(u128, u32, AgentId)> = Vec::new();
        self.for_capsule_candidates(&capsule, |e| {
            if let Some(s) = capsule.hit(e.pos) {
                hits.push((s, e.id.index, e.id));
            }
        });
        hits.sort_unstable_by_key(|h| (h.0, h.1));
        out.extend(hits.into_iter().map(|h| h.2));
    }

    /// The closest indexed agent within `radius` of `center`, with the same
    /// `(distance_sq, slot index)` tie-break as [`SpatialGrid::query_radius`].
    ///
    /// Like [`SpatialGrid::query_radius`] it reflects the last [`SpatialGrid::rebuild`] and may
    /// return an id that has been despawned since.
    pub fn nearest(&self, center: Vec2Fx, radius: Fx) -> Option<AgentId> {
        let r = radius.raw();
        if r < 0 {
            return None;
        }
        let r2 = (r as u128) * (r as u128);
        let mut best: Option<(u128, u32, AgentId)> = None;
        self.for_candidates(center, r, |e| {
            let d = distance_sq_raw(e.pos, center);
            if d <= r2 && best.is_none_or(|b| (d, e.id.index) < (b.0, b.1)) {
                best = Some((d, e.id.index, e.id));
            }
        });
        best.map(|b| b.2)
    }

    /// Like [`SpatialGrid::query_radius`], but ids that `store` no longer considers alive (not
    /// alive, or a slot reused under a newer generation) are left out. Positions and distances
    /// still come from the last [`SpatialGrid::rebuild`].
    pub fn query_radius_alive(
        &self,
        store: &AgentStore,
        center: Vec2Fx,
        radius: Fx,
        out: &mut Vec<AgentId>,
    ) {
        out.clear();
        let r = radius.raw();
        if r < 0 {
            return;
        }
        let r2 = (r as u128) * (r as u128);
        let mut hits: Vec<(u128, u32, AgentId)> = Vec::new();
        self.for_candidates(center, r, |e| {
            let d = distance_sq_raw(e.pos, center);
            if d <= r2 && store.is_alive(e.id) {
                hits.push((d, e.id.index, e.id));
            }
        });
        hits.sort_unstable_by_key(|h| (h.0, h.1));
        out.extend(hits.into_iter().map(|h| h.2));
    }

    /// Like [`SpatialGrid::nearest`], but only considers ids that `store` still considers alive,
    /// so a despawned nearest agent yields the next nearest live one rather than itself.
    pub fn nearest_alive(&self, store: &AgentStore, center: Vec2Fx, radius: Fx) -> Option<AgentId> {
        let r = radius.raw();
        if r < 0 {
            return None;
        }
        let r2 = (r as u128) * (r as u128);
        let mut best: Option<(u128, u32, AgentId)> = None;
        self.for_candidates(center, r, |e| {
            let d = distance_sq_raw(e.pos, center);
            if d <= r2 && store.is_alive(e.id) && best.is_none_or(|b| (d, e.id.index) < (b.0, b.1))
            {
                best = Some((d, e.id.index, e.id));
            }
        });
        best.map(|b| b.2)
    }

    /// Total number of grid cells visited by queries since construction or the
    /// last [`SpatialGrid::reset_cells_touched`].
    pub fn cells_touched(&self) -> u64 {
        self.cells_touched.load(Ordering::Relaxed)
    }

    /// Resets the [`SpatialGrid::cells_touched`] counter to zero.
    pub fn reset_cells_touched(&self) {
        self.cells_touched.store(0, Ordering::Relaxed);
    }
}
