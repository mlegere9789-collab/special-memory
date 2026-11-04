//! Shared synthetic pack for the terrain tests: 3 x 2 tiles whose content is a pure integer
//! function of global coordinates, so tile seams agree by construction and the Python references
//! (`qc/fixtures/ref_height.py`) can recompute it independently.
#![allow(dead_code)]

pub mod lospack;

use core_math::{Fx, Vec2Fx};
use terrain::{TerrainIndex, TerrainWorld, Tile, TileId, CELL_MM, TILE_CELLS};

pub const TILES_X: u32 = 3;
pub const TILES_Y: u32 = 2;
pub const EXT_X_MM: i64 = TILES_X as i64 * 512_000;
pub const EXT_Y_MM: i64 = TILES_Y as i64 * 512_000;
pub const CELLS_X: i64 = TILES_X as i64 * 128;
pub const CELLS_Y: i64 = TILES_Y as i64 * 128;

/// Vertex height in centimetres at global vertex `(vx, vy)`.
pub fn vertex_cm(vx: i64, vy: i64) -> u16 {
    ((vx * 131 + vy * 197 + ((vx * vy) % 97) * 211 + ((vx ^ vy) % 64) * 53) % 9000) as u16
}

pub fn cell_class(cx: i64, cy: i64) -> u8 {
    ((cx * 7 + cy * 3) % 12) as u8
}

pub fn cell_occ(cx: i64, cy: i64) -> u8 {
    ((cx + cy * 5) % 200) as u8
}

pub fn cell_flags(cx: i64, cy: i64) -> u8 {
    ((cx ^ cy) & 0x1f) as u8
}

/// Millimetres to the position value whose floor conversion returns the same millimetres.
pub fn fx_mm(mm: i64) -> Fx {
    let raw = ((mm as i128) << 32) + 999;
    Fx::from_raw(raw.div_euclid(1000) as i64)
}

pub fn pos(x_mm: i64, y_mm: i64) -> Vec2Fx {
    Vec2Fx::new(fx_mm(x_mm), fx_mm(y_mm))
}

pub fn synth_tile(tx: u16, ty: u16) -> Tile {
    let mut t = Tile::new(TileId { x: tx, y: ty });
    let n = TILE_CELLS as i64;
    let (bx, by) = (i64::from(tx) * n, i64::from(ty) * n);
    for ly in 0..=n {
        for lx in 0..=n {
            t.heights_mut()[(ly * (n + 1) + lx) as usize] = vertex_cm(bx + lx, by + ly);
        }
    }
    for ly in 0..n {
        for lx in 0..n {
            let i = (ly * n + lx) as usize;
            t.classes_mut()[i] = cell_class(bx + lx, by + ly);
            t.occ_mut()[i] = cell_occ(bx + lx, by + ly);
            t.flags_mut()[i] = cell_flags(bx + lx, by + ly);
        }
    }
    t
}

pub fn synth_tiles() -> Vec<Tile> {
    let mut v = Vec::new();
    for ty in 0..TILES_Y as u16 {
        for tx in 0..TILES_X as u16 {
            v.push(synth_tile(tx, ty));
        }
    }
    v
}

/// Index text for tiles `tiles`, hashes taken from the tiles themselves.
pub fn index_toml(tiles: &[Tile], tiles_x: u32, tiles_y: u32) -> String {
    let mut s = String::new();
    s.push_str("schema = \"terrain-index/1\"\n");
    s.push_str(&format!("cell_mm = {CELL_MM}\ntile_cells = {TILE_CELLS}\n"));
    s.push_str(&format!("tiles_x = {tiles_x}\ntiles_y = {tiles_y}\n"));
    s.push_str("origin_easting_mm = 300000000\norigin_northing_mm = 4400000000\n");
    s.push_str("z_origin_cm = 0\ncrs = \"UTM 18N\"\n\n[legend]\n0 = \"open\"\n1 = \"road\"\n");
    for t in tiles {
        s.push_str(&format!(
            "\n[[tile]]\nx = {}\ny = {}\nhash_hex = \"{:016x}\"\n",
            t.id().x,
            t.id().y,
            t.hash()
        ));
    }
    s
}

pub fn synth_index() -> TerrainIndex {
    TerrainIndex::from_toml_str(&index_toml(&synth_tiles(), TILES_X, TILES_Y)).unwrap()
}

/// World with every synthetic tile resident.
pub fn synth_world() -> TerrainWorld {
    let mut w = TerrainWorld::new(synth_index());
    for t in synth_tiles() {
        w.insert_tile(&t.encode()).unwrap();
    }
    w
}

/// Seeded SplitMix64 used by tests that need cheap deterministic points.
pub struct Sm(pub u64);

impl Sm {
    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    pub fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}
