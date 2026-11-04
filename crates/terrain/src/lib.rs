//! Battlefield terrain pack: tile codec, index and the height, class, cover, flag and slope
//! queries (SPEC-014 sections 4, 5 and 11). Obstacles and line of sight are added by a later
//! ticket.
//!
//! Everything is integer-only and deterministic: no I/O, no clocks, no floating point.
//! Positions are [`Vec2Fx`] in metres (the simulation's unit); the pack works in millimetres, and
//! [`fx_to_mm`] is the single conversion (floor of `raw * 1000 / 2^32`).

#![forbid(unsafe_code)]

mod hash;
mod index;
mod minitoml;
mod tile;
mod world;

pub use hash::fnv1a64;
pub use index::{Sector, Slab, TerrainIndex};
pub use tile::{ObstacleSeg, Tile, CELL_COUNT, HEIGHT_COUNT, HEIGHT_SIDE};
pub use world::TerrainWorld;

use core_math::Fx;

/// Edge of one terrain cell in millimetres.
pub const CELL_MM: i64 = 4000;
/// Cells along one tile edge.
pub const TILE_CELLS: u32 = 128;
/// Edge of one tile in millimetres (`TILE_CELLS * CELL_MM`).
pub const TILE_MM: i64 = 512_000;
/// Distance between line-of-sight samples in millimetres (CT-014-5).
pub const SIGHT_STEP_MM: i64 = 2000;

/// Address of a tile; orders by `(y, x)`, the order of the index and of every listing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TileId {
    /// Tile column, west to east.
    pub x: u16,
    /// Tile row, south to north.
    pub y: u16,
}

impl Ord for TileId {
    fn cmp(&self, other: &TileId) -> core::cmp::Ordering {
        (self.y, self.x).cmp(&(other.y, other.x))
    }
}

impl PartialOrd for TileId {
    fn partial_cmp(&self, other: &TileId) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Everything that can go wrong decoding, indexing or querying a pack.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TerrainError {
    /// Malformed tile bytes or a tile with inconsistent parts.
    Format(String),
    /// A body hash does not match its content or the index entry.
    Hash,
    /// The tile holding the queried cell is not resident.
    TileMissing(TileId),
    /// A tile lies outside the extent the index declares.
    OutOfExtent,
    /// Malformed or inconsistent `index.toml`, or a tile the index does not list.
    Index(String),
}

impl core::fmt::Display for TerrainError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            TerrainError::Format(m) => write!(f, "tile format: {m}"),
            TerrainError::Hash => write!(f, "hash mismatch"),
            TerrainError::TileMissing(t) => write!(f, "tile ({}, {}) is not resident", t.x, t.y),
            TerrainError::OutOfExtent => write!(f, "tile outside the extent"),
            TerrainError::Index(m) => write!(f, "index: {m}"),
        }
    }
}

impl std::error::Error for TerrainError {}

/// Millimetres of a position value: `floor(raw * 1000 / 2^32)`.
///
/// ```
/// use core_math::Fx;
/// assert_eq!(terrain::fx_to_mm(Fx::from_int(3)), 3000);
/// assert_eq!(terrain::fx_to_mm(Fx::from_raw(-1)), -1);
/// ```
pub fn fx_to_mm(v: Fx) -> i64 {
    // |raw * 1000| < 2^73, so the shifted value always fits an i64.
    ((i128::from(v.raw()) * 1000) >> 32) as i64
}

/// The smallest position value whose [`fx_to_mm`] is `mm` (saturating at the `Fx` range), so
/// `fx_to_mm(mm_to_fx(mm)) == mm` for every `mm` in the representable range.
///
/// ```
/// let p = terrain::mm_to_fx(1234);
/// assert_eq!(terrain::fx_to_mm(p), 1234);
/// assert_eq!(terrain::fx_to_mm(core_math::Fx::from_raw(p.raw() - 1)), 1233);
/// ```
pub fn mm_to_fx(mm: i64) -> Fx {
    let num = (i128::from(mm) << 32) + 999;
    let raw = num.div_euclid(1000);
    Fx::from_raw(raw.clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64)
}
