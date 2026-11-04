//! The pack index `index.toml` (SPEC-014 section 4.2) and its content hash.

use crate::minitoml::{self, Table, Val};
use crate::{fnv1a64, TerrainError, TileId, CELL_MM, TILE_CELLS};
use std::collections::BTreeMap;

const SCHEMA: &str = "terrain-index/1";

/// A named tile set for polish and review.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sector {
    /// Sector name.
    pub id: String,
    /// Member tiles in file order.
    pub tiles: Vec<TileId>,
}

/// A named rectangle of whole tiles (inclusive tile coordinates as written in the file).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Slab {
    /// Slab name.
    pub id: String,
    /// First tile column.
    pub x0: i64,
    /// First tile row.
    pub y0: i64,
    /// Last tile column.
    pub x1: i64,
    /// Last tile row.
    pub y1: i64,
}

/// The parsed index: extent, vertical origin, legend and the body hash of every tile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerrainIndex {
    tiles_x: u32,
    tiles_y: u32,
    origin_easting_mm: i64,
    origin_northing_mm: i64,
    z_origin_cm: i64,
    crs: String,
    legend: BTreeMap<u8, String>,
    tiles: BTreeMap<TileId, u64>,
    sectors: Vec<Sector>,
    slabs: Vec<Slab>,
}

fn bad(msg: impl Into<String>) -> TerrainError {
    TerrainError::Index(msg.into())
}

fn int(t: &mut Table, key: &str) -> Result<i64, TerrainError> {
    match t.remove(key) {
        Some(Val::Int(v)) => Ok(v),
        Some(_) => Err(bad(format!("`{key}` must be an integer"))),
        None => Err(bad(format!("missing `{key}`"))),
    }
}

fn text(t: &mut Table, key: &str) -> Result<String, TerrainError> {
    match t.remove(key) {
        Some(Val::Str(v)) => Ok(v),
        Some(_) => Err(bad(format!("`{key}` must be a string"))),
        None => Err(bad(format!("missing `{key}`"))),
    }
}

fn no_extra(t: &Table, what: &str) -> Result<(), TerrainError> {
    match t.keys().next() {
        Some(k) => Err(bad(format!("unknown key `{k}` in {what}"))),
        None => Ok(()),
    }
}

fn ranged(v: i64, lo: i64, hi: i64, what: &str) -> Result<i64, TerrainError> {
    if (lo..=hi).contains(&v) {
        Ok(v)
    } else {
        Err(bad(format!("`{what}` out of range")))
    }
}

fn tile_in(x: i64, y: i64, nx: u32, ny: u32) -> Result<TileId, TerrainError> {
    if (0..i64::from(nx)).contains(&x) && (0..i64::from(ny)).contains(&y) {
        Ok(TileId {
            x: x as u16,
            y: y as u16,
        })
    } else {
        Err(bad("tile outside the extent"))
    }
}

fn hex_u64(s: &str) -> Result<u64, TerrainError> {
    if s.is_empty() || s.len() > 16 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(bad("`hash_hex` must be 1 to 16 hex digits"));
    }
    u64::from_str_radix(s, 16).map_err(|_| bad("`hash_hex` must be 1 to 16 hex digits"))
}

impl TerrainIndex {
    /// Parses and validates an index. Floats, unknown keys, duplicate or out-of-extent tiles, a
    /// `cell_mm` other than 4000 and a `tile_cells` other than 128 are refused.
    pub fn from_toml_str(text_in: &str) -> Result<TerrainIndex, TerrainError> {
        let mut doc = minitoml::parse(text_in)?;
        let r = &mut doc.root;
        if text(r, "schema")? != SCHEMA {
            return Err(bad("unknown schema"));
        }
        if int(r, "cell_mm")? != CELL_MM || int(r, "tile_cells")? != i64::from(TILE_CELLS) {
            return Err(bad("cell_mm must be 4000 and tile_cells 128"));
        }
        let tiles_x = ranged(int(r, "tiles_x")?, 1, 65_536, "tiles_x")? as u32;
        let tiles_y = ranged(int(r, "tiles_y")?, 1, 65_536, "tiles_y")? as u32;
        let origin_easting_mm = int(r, "origin_easting_mm")?;
        let origin_northing_mm = int(r, "origin_northing_mm")?;
        let z_origin_cm = int(r, "z_origin_cm")?;
        let crs = text(r, "crs")?;
        no_extra(r, "the index")?;

        let mut legend = BTreeMap::new();
        let leg = doc
            .tables
            .remove("legend")
            .ok_or_else(|| bad("missing [legend]"))?;
        for (k, v) in leg {
            let id = k
                .parse::<u8>()
                .map_err(|_| bad("legend key must be a class id 0 to 255"))?;
            match v {
                Val::Str(name) => legend.insert(id, name),
                _ => return Err(bad("legend names are strings")),
            };
        }
        if let Some(name) = doc.tables.keys().next() {
            return Err(bad(format!("unknown table [{name}]")));
        }

        let (mut tiles, mut sectors, mut slabs) = (BTreeMap::new(), Vec::new(), Vec::new());
        for (name, mut row) in doc.arrays {
            match name.as_str() {
                "tile" => {
                    let id = tile_in(int(&mut row, "x")?, int(&mut row, "y")?, tiles_x, tiles_y)?;
                    let hash = hex_u64(&text(&mut row, "hash_hex")?)?;
                    no_extra(&row, "[[tile]]")?;
                    if tiles.insert(id, hash).is_some() {
                        return Err(bad("duplicate tile"));
                    }
                }
                "sector" => {
                    let id = text(&mut row, "id")?;
                    let list = match row.remove("tiles") {
                        Some(Val::Strs(v)) => v,
                        _ => return Err(bad("`tiles` must be an array of \"x,y\" strings")),
                    };
                    no_extra(&row, "[[sector]]")?;
                    let mut ids = Vec::new();
                    for s in list {
                        let (x, y) = s
                            .split_once(',')
                            .ok_or_else(|| bad("sector tile must be \"x,y\""))?;
                        let n = |v: &str| {
                            v.trim()
                                .parse::<i64>()
                                .map_err(|_| bad("sector tile must be \"x,y\""))
                        };
                        ids.push(tile_in(n(x)?, n(y)?, tiles_x, tiles_y)?);
                    }
                    sectors.push(Sector { id, tiles: ids });
                }
                "slab" => {
                    let id = text(&mut row, "id")?;
                    let (x0, y0) = (int(&mut row, "x0")?, int(&mut row, "y0")?);
                    let (x1, y1) = (int(&mut row, "x1")?, int(&mut row, "y1")?);
                    no_extra(&row, "[[slab]]")?;
                    slabs.push(Slab { id, x0, y0, x1, y1 });
                }
                other => return Err(bad(format!("unknown table [[{other}]]"))),
            }
        }
        Ok(TerrainIndex {
            tiles_x,
            tiles_y,
            origin_easting_mm,
            origin_northing_mm,
            z_origin_cm,
            crs,
            legend,
            tiles,
            sectors,
            slabs,
        })
    }

    /// FNV-1a 64 over `cell_mm`, `tiles_x`, `tiles_y` (each `u64` little-endian), `z_origin_cm`
    /// (`i64` little-endian) and the tile hashes (`u64` little-endian) in ascending `(y, x)`.
    pub fn content_hash(&self) -> u64 {
        let mut b = Vec::with_capacity(32 + 8 * self.tiles.len());
        for v in [
            CELL_MM as u64,
            u64::from(self.tiles_x),
            u64::from(self.tiles_y),
        ] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.extend_from_slice(&self.z_origin_cm.to_le_bytes());
        for h in self.tiles.values() {
            b.extend_from_slice(&h.to_le_bytes());
        }
        fnv1a64(&b)
    }

    /// The body hash the index records for `id`, if it lists that tile.
    pub fn tile_hash(&self, id: TileId) -> Option<u64> {
        self.tiles.get(&id).copied()
    }

    /// Cell edge in millimetres (always 4000).
    pub fn cell_mm(&self) -> i64 {
        CELL_MM
    }
    /// Cells per tile edge (always 128).
    pub fn tile_cells(&self) -> u32 {
        TILE_CELLS
    }
    /// Tiles west to east.
    pub fn tiles_x(&self) -> u32 {
        self.tiles_x
    }
    /// Tiles south to north.
    pub fn tiles_y(&self) -> u32 {
        self.tiles_y
    }
    /// Easting of the south-west corner of tile (0, 0), millimetres.
    pub fn origin_easting_mm(&self) -> i64 {
        self.origin_easting_mm
    }
    /// Northing of the south-west corner of tile (0, 0), millimetres.
    pub fn origin_northing_mm(&self) -> i64 {
        self.origin_northing_mm
    }
    /// Elevation of height value 0, centimetres.
    pub fn z_origin_cm(&self) -> i64 {
        self.z_origin_cm
    }
    /// Coordinate reference system text.
    pub fn crs(&self) -> &str {
        &self.crs
    }
    /// Class id to name.
    pub fn legend(&self) -> &BTreeMap<u8, String> {
        &self.legend
    }
    /// Sectors in file order.
    pub fn sectors(&self) -> &[Sector] {
        &self.sectors
    }
    /// Slabs in file order.
    pub fn slabs(&self) -> &[Slab] {
        &self.slabs
    }
}
