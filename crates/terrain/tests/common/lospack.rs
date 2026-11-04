//! Synthetic line-of-sight pack: smooth ridges and cover blocks as integer functions of global
//! coordinates, plus the committed obstacle list `qc/fixtures/ref_los_obstacles.csv`. The same
//! definition is restated in `qc/fixtures/ref_los.py`.
#![allow(dead_code)]

use super::{index_toml, TILES_X, TILES_Y};
use terrain::{ObstacleSeg, TerrainIndex, TerrainWorld, Tile, TileId, TILE_CELLS};

fn tri(v: i64, p: i64) -> i64 {
    (v % p).min(p - v % p)
}

pub fn vertex_cm(vx: i64, vy: i64) -> u16 {
    (3000 + 30 * tri(vx, 56) + 25 * tri(vy, 44) + ((vx * 7 + vy * 13) % 11) * 4) as u16
}

pub fn occ_dm(cx: i64, cy: i64) -> u8 {
    if (cx / 12 + cy / 9) % 6 == 0 {
        180
    } else if (cx * 5 + cy * 3) % 13 == 0 {
        12
    } else {
        0
    }
}

/// Segment flag bits as the compiler sets them from the obstacle kind table.
pub const FIRE: u8 = 1;
pub const SIGHT: u8 = 2;

pub fn read_csv(name: &str) -> Vec<Vec<i64>> {
    let path = format!("{}/../../qc/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(path).unwrap();
    text.lines()
        .skip(1)
        .map(|l| l.split(',').map(|s| s.parse().unwrap()).collect())
        .collect()
}

pub fn pack_obstacles() -> Vec<ObstacleSeg> {
    read_csv("ref_los_obstacles.csv")
        .iter()
        .map(|v| ObstacleSeg {
            id: v[0] as u32,
            kind: v[1] as u8,
            flags: v[2] as u8,
            height_mm: v[3] as u16,
            a: [v[4] as i32, v[5] as i32],
            b: [v[6] as i32, v[7] as i32],
        })
        .collect()
}

/// Tile `(tx, ty)` holding every obstacle whose bounding box touches it.
pub fn los_tile(tx: u16, ty: u16, segs: &[ObstacleSeg]) -> Tile {
    let n = i64::from(TILE_CELLS);
    let mut t = Tile::new(TileId { x: tx, y: ty });
    let (bx, by) = (i64::from(tx) * n, i64::from(ty) * n);
    for ly in 0..=n {
        for lx in 0..=n {
            t.heights_mut()[(ly * (n + 1) + lx) as usize] = vertex_cm(bx + lx, by + ly);
        }
    }
    for ly in 0..n {
        for lx in 0..n {
            t.occ_mut()[(ly * n + lx) as usize] = occ_dm(bx + lx, by + ly);
        }
    }
    let (x0, y0) = (i64::from(tx) * 512_000, i64::from(ty) * 512_000);
    let (x1, y1) = (x0 + 512_000, y0 + 512_000);
    let mine: Vec<ObstacleSeg> = segs
        .iter()
        .filter(|s| {
            let (lo_x, hi_x) = (i64::from(s.a[0].min(s.b[0])), i64::from(s.a[0].max(s.b[0])));
            let (lo_y, hi_y) = (i64::from(s.a[1].min(s.b[1])), i64::from(s.a[1].max(s.b[1])));
            hi_x >= x0 && lo_x <= x1 && hi_y >= y0 && lo_y <= y1
        })
        .copied()
        .collect();
    t.set_obstacles(mine).unwrap();
    t
}

pub fn los_tiles() -> Vec<Tile> {
    let segs = pack_obstacles();
    let mut v = Vec::new();
    for ty in 0..TILES_Y as u16 {
        for tx in 0..TILES_X as u16 {
            v.push(los_tile(tx, ty, &segs));
        }
    }
    v
}

pub fn los_index(tiles: &[Tile]) -> TerrainIndex {
    TerrainIndex::from_toml_str(&index_toml(tiles, TILES_X, TILES_Y)).unwrap()
}

/// World over the pack with exactly the tiles `keep` resident.
pub fn los_world_with(keep: &[TileId]) -> TerrainWorld {
    let tiles = los_tiles();
    let mut w = TerrainWorld::new(los_index(&tiles));
    for t in tiles.iter().filter(|t| keep.contains(&t.id())) {
        w.insert_tile(&t.encode()).unwrap();
    }
    w
}

pub fn los_world() -> TerrainWorld {
    let all: Vec<TileId> = los_tiles().iter().map(Tile::id).collect();
    los_world_with(&all)
}
