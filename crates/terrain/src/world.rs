//! `TerrainWorld`: the resident tiles and the per-point queries (SPEC-014 sections 5, 8, 11).
//!
//! Every query reads one tile, the one holding the cell it lands in, so residency and the order
//! of insertion and eviction never change an answer; a non-resident tile is an error.

use crate::{
    fx_to_mm, TerrainError, TerrainIndex, Tile, TileId, CELL_MM, HEIGHT_SIDE, TILE_CELLS, TILE_MM,
};
use core_math::Vec2Fx;
use std::collections::BTreeMap;

/// The pack index plus the tiles currently resident.
#[derive(Debug)]
pub struct TerrainWorld {
    index: TerrainIndex,
    tiles: BTreeMap<TileId, Tile>,
}

/// A position resolved to a cell and the offsets inside it.
struct Spot<'a> {
    tile: &'a Tile,
    lx: usize,
    ly: usize,
    fx: i64,
    fy: i64,
}

impl TerrainWorld {
    /// An empty world over `index`.
    pub fn new(index: TerrainIndex) -> TerrainWorld {
        TerrainWorld {
            index,
            tiles: BTreeMap::new(),
        }
    }

    /// The index.
    pub fn index(&self) -> &TerrainIndex {
        &self.index
    }

    /// Decodes `bytes` and makes the tile resident (replacing a resident copy). The tile must lie
    /// inside the extent ([`TerrainError::OutOfExtent`]), be listed by the index
    /// ([`TerrainError::Index`]) and match the listed hash ([`TerrainError::Hash`]).
    pub fn insert_tile(&mut self, bytes: &[u8]) -> Result<TileId, TerrainError> {
        let tile = Tile::decode(bytes)?;
        let id = tile.id();
        if u32::from(id.x) >= self.index.tiles_x() || u32::from(id.y) >= self.index.tiles_y() {
            return Err(TerrainError::OutOfExtent);
        }
        match self.index.tile_hash(id) {
            None => {
                return Err(TerrainError::Index(format!(
                    "tile ({}, {}) is not in the index",
                    id.x, id.y
                )))
            }
            Some(h) if h != tile.hash() => return Err(TerrainError::Hash),
            Some(_) => {}
        }
        self.tiles.insert(id, tile);
        Ok(id)
    }

    /// Drops a resident tile; false if it was not resident.
    pub fn evict_tile(&mut self, id: TileId) -> bool {
        self.tiles.remove(&id).is_some()
    }

    /// Whether `id` is resident.
    pub fn is_resident(&self, id: TileId) -> bool {
        self.tiles.contains_key(&id)
    }

    /// Resident tiles in ascending `(y, x)`.
    pub fn resident_ids(&self) -> Vec<TileId> {
        self.tiles.keys().copied().collect()
    }

    fn extent_mm(&self) -> (i64, i64) {
        (
            i64::from(self.index.tiles_x()) * TILE_MM,
            i64::from(self.index.tiles_y()) * TILE_MM,
        )
    }

    /// Tiles whose area meets the rectangle `min..=max` after clamping to the extent, ascending
    /// `(y, x)`. Residency is not consulted. An inverted rectangle selects nothing.
    pub fn tiles_for_rect(&self, min: Vec2Fx, max: Vec2Fx) -> Vec<TileId> {
        if min.x > max.x || min.y > max.y {
            return Vec::new();
        }
        let span = |lo: i64, hi: i64, n: u32| {
            let t = |v: i64| v.div_euclid(TILE_MM).clamp(0, i64::from(n) - 1) as u16;
            (t(lo), t(hi))
        };
        let (x0, x1) = span(fx_to_mm(min.x), fx_to_mm(max.x), self.index.tiles_x());
        let (y0, y1) = span(fx_to_mm(min.y), fx_to_mm(max.y), self.index.tiles_y());
        let mut out = Vec::with_capacity((usize::from(x1 - x0) + 1) * (usize::from(y1 - y0) + 1));
        for y in y0..=y1 {
            for x in x0..=x1 {
                out.push(TileId { x, y });
            }
        }
        out
    }

    /// Clamps `p` to the extent, finds its cell and the tile that holds it.
    fn locate(&self, p: Vec2Fx) -> Result<Spot<'_>, TerrainError> {
        let (ex, ey) = self.extent_mm();
        let (x, y) = (fx_to_mm(p.x).clamp(0, ex), fx_to_mm(p.y).clamp(0, ey));
        let cells = i64::from(TILE_CELLS);
        let cx = (x / CELL_MM).min(i64::from(self.index.tiles_x()) * cells - 1);
        let cy = (y / CELL_MM).min(i64::from(self.index.tiles_y()) * cells - 1);
        let id = TileId {
            x: (cx / cells) as u16,
            y: (cy / cells) as u16,
        };
        let tile = self.tiles.get(&id).ok_or(TerrainError::TileMissing(id))?;
        Ok(Spot {
            tile,
            lx: (cx % cells) as usize,
            ly: (cy % cells) as usize,
            fx: x - cx * CELL_MM,
            fy: y - cy * CELL_MM,
        })
    }

    fn corners(s: &Spot<'_>) -> [i64; 4] {
        let at = |dx: usize, dy: usize| {
            i64::from(s.tile.heights()[(s.ly + dy) * HEIGHT_SIDE + s.lx + dx])
        };
        [at(0, 0), at(1, 0), at(0, 1), at(1, 1)]
    }

    /// Ground height in millimetres above `z_origin_cm`: bilinear in the cell, floored.
    /// Positions outside the extent clamp to it.
    pub fn height_mm(&self, p: Vec2Fx) -> Result<i64, TerrainError> {
        let s = self.locate(p)?;
        let [h00, h10, h01, h11] = Self::corners(&s).map(i128::from);
        let (c, fx, fy) = (i128::from(CELL_MM), i128::from(s.fx), i128::from(s.fy));
        let sum =
            h00 * (c - fx) * (c - fy) + h10 * fx * (c - fy) + h01 * (c - fx) * fy + h11 * fx * fy;
        Ok((10 * sum).div_euclid(c * c) as i64)
    }

    /// Terrain class id of the cell holding `p` (clamped to the extent).
    pub fn class_at(&self, p: Vec2Fx) -> Result<u8, TerrainError> {
        let s = self.locate(p)?;
        Ok(s.tile.classes()[s.ly * TILE_CELLS as usize + s.lx])
    }

    /// Height of sight-blocking cover above ground in millimetres (`occ_dm * 100`).
    pub fn occ_mm(&self, p: Vec2Fx) -> Result<i64, TerrainError> {
        let s = self.locate(p)?;
        Ok(100 * i64::from(s.tile.occ()[s.ly * TILE_CELLS as usize + s.lx]))
    }

    /// Flag bits of the cell holding `p`.
    pub fn flags_at(&self, p: Vec2Fx) -> Result<u8, TerrainError> {
        let s = self.locate(p)?;
        Ok(s.tile.flags()[s.ly * TILE_CELLS as usize + s.lx])
    }

    /// Slope in permille of the cell holding `p`: with centimetre corner heights,
    /// `gx = ((h10 + h11) - (h00 + h01)) / 2` and `gy = ((h01 + h11) - (h00 + h10)) / 2`
    /// (truncating), result `isqrt(gx^2 + gy^2) * 10 * 1000 / 4000`.
    pub fn slope_pm(&self, p: Vec2Fx) -> Result<i64, TerrainError> {
        let s = self.locate(p)?;
        let [h00, h10, h01, h11] = Self::corners(&s);
        let gx = ((h10 + h11) - (h00 + h01)) / 2;
        let gy = ((h01 + h11) - (h00 + h10)) / 2;
        let g = (gx * gx + gy * gy) as u64;
        Ok(i64::try_from(g.isqrt()).unwrap_or(i64::MAX) * 10 * 1000 / CELL_MM)
    }
}
