//! Compile stages 1, 3 and 5 (SPEC-014 section 5): vertex heights, classes and cover, then tiles,
//! index and hashes. Stages 2 and 4 (adjustments, obstacle segments) are separate tickets, so the
//! obstacle section of every tile is empty.

use crate::raster::{rasterise, Grid};
use crate::survey::{Survey, CLASS_NAMES};
use std::fmt::Write as _;
use std::path::Path;
use terrain::{TerrainIndex, Tile, TileId, TILE_CELLS};

/// Tile flag bit 0: a corner of the cell has too few valid posts.
pub const NO_SURVEY: u8 = 1;
/// Fewest valid posts a vertex needs to count as surveyed.
const MIN_POSTS: i64 = 8;

/// A compiled pack in memory: tiles ascending by `(y, x)`, the index text and its content hash.
pub struct Pack {
    pub tiles: Vec<Tile>,
    pub index_text: String,
    pub content_hash: u64,
}

struct Vertices {
    nx: usize,
    cm: Vec<Option<i64>>,
    no_survey: Vec<bool>,
}

/// Stage 1. Vertex `(vx, vy)` sits at `(4 vx, 4 vy)` m and takes the mean of the valid posts
/// `4v - 2 ..= 4v + 1` per axis, rounded half up as `floor((2 sum + n) / 2n)`; `(sum + 8) / 16`
/// when all sixteen are valid. A vertex with fewer than eight valid posts is `NO_SURVEY`.
fn vertices(s: &Survey) -> Vertices {
    let cells = TILE_CELLS as usize;
    let (nx, ny) = (
        s.tiles_x as usize * cells + 1,
        s.tiles_y as usize * cells + 1,
    );
    let mut v = Vertices {
        nx,
        cm: Vec::with_capacity(nx * ny),
        no_survey: Vec::with_capacity(nx * ny),
    };
    for vy in 0..ny as i64 {
        for vx in 0..nx as i64 {
            let (mut sum, mut n) = (0i64, 0i64);
            for j in (4 * vy - 2)..=(4 * vy + 1) {
                for i in (4 * vx - 2)..=(4 * vx + 1) {
                    if i < 0 || j < 0 || i >= s.width_posts as i64 || j >= s.height_posts as i64 {
                        continue;
                    }
                    let post = s.dem[j as usize * s.width_posts + i as usize];
                    if post != i32::MIN {
                        sum += i64::from(post);
                        n += 1;
                    }
                }
            }
            v.cm.push((n > 0).then(|| (2 * sum + n).div_euclid(2 * n)));
            v.no_survey.push(n < MIN_POSTS);
        }
    }
    v
}

fn index_text(s: &Survey, z_origin_cm: i64, tiles: &[Tile]) -> String {
    let mut t = String::from("schema = \"terrain-index/1\"\ncell_mm = 4000\ntile_cells = 128\n");
    let _ = writeln!(t, "tiles_x = {}\ntiles_y = {}", s.tiles_x, s.tiles_y);
    let _ = writeln!(
        t,
        "origin_easting_mm = {}\norigin_northing_mm = {}",
        s.origin_easting_mm, s.origin_northing_mm
    );
    let _ = writeln!(
        t,
        "z_origin_cm = {z_origin_cm}\ncrs = \"{}\"\n\n[legend]",
        s.crs.replace('\\', "\\\\").replace('"', "\\\"")
    );
    for (id, name) in CLASS_NAMES.iter().enumerate() {
        let _ = writeln!(t, "{id} = \"{name}\"");
    }
    for tile in tiles {
        let _ = writeln!(
            t,
            "\n[[tile]]\nx = {}\ny = {}\nhash_hex = \"{:016x}\"",
            tile.id().x,
            tile.id().y,
            tile.hash()
        );
    }
    for sl in &s.slabs {
        let _ = writeln!(
            t,
            "\n[[slab]]\nid = \"{}\"\nx0 = {}\ny0 = {}\nx1 = {}\ny1 = {}",
            sl.id, sl.x0, sl.y0, sl.x1, sl.y1
        );
    }
    t
}

fn tile_at(id: TileId, v: &Vertices, z0: i64, g: &Grid) -> Result<Tile, String> {
    let n = TILE_CELLS as usize;
    let (bx, by) = (id.x as usize * n, id.y as usize * n);
    let vertex = |lx: usize, ly: usize| (by + ly) * v.nx + bx + lx;
    let mut heights = Vec::with_capacity((n + 1) * (n + 1));
    for ly in 0..=n {
        for lx in 0..=n {
            heights.push(v.cm[vertex(lx, ly)].map_or(0, |h| (h - z0) as u16));
        }
    }
    let (mut classes, mut occ, mut flags) = (Vec::new(), Vec::new(), Vec::new());
    for ly in 0..n {
        for lx in 0..n {
            let c = (by + ly) * g.cells_x + bx + lx;
            classes.push(g.class[c]);
            occ.push(g.occ[c]);
            let bad = [(0, 0), (1, 0), (0, 1), (1, 1)]
                .iter()
                .any(|&(dx, dy)| v.no_survey[vertex(lx + dx, ly + dy)]);
            flags.push(if bad { NO_SURVEY } else { 0 });
        }
    }
    Tile::from_parts(id, heights, classes, occ, flags, Vec::new()).map_err(|e| e.to_string())
}

/// Builds the whole pack. Fails when no vertex has any valid post or the relief exceeds 65,535 cm.
pub fn build_pack(s: &Survey) -> Result<Pack, String> {
    let v = vertices(s);
    let known = || v.cm.iter().flatten();
    let (Some(&lo), Some(&hi)) = (known().min(), known().max()) else {
        return Err("no vertex has a valid elevation post".into());
    };
    if hi - lo > i64::from(u16::MAX) {
        return Err(format!("relief {} cm exceeds 65535 cm", hi - lo));
    }
    let grid = rasterise(s);
    let mut tiles = Vec::new();
    for y in 0..s.tiles_y {
        for x in 0..s.tiles_x {
            tiles.push(tile_at(
                TileId {
                    x: x as u16,
                    y: y as u16,
                },
                &v,
                lo,
                &grid,
            )?);
        }
    }
    let index_text = index_text(s, lo, &tiles);
    let content_hash = TerrainIndex::from_toml_str(&index_text)
        .map_err(|e| e.to_string())?
        .content_hash();
    Ok(Pack {
        tiles,
        index_text,
        content_hash,
    })
}

impl Pack {
    /// Writes `index.toml` and `tiles/<x>_<y>.hdt` under `out`.
    pub fn write(&self, out: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(out.join("tiles"))?;
        std::fs::write(out.join("index.toml"), &self.index_text)?;
        for t in &self.tiles {
            std::fs::write(
                out.join("tiles")
                    .join(format!("{}_{}.hdt", t.id().x, t.id().y)),
                t.encode(),
            )?;
        }
        Ok(())
    }
}
