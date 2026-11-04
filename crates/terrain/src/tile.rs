//! The tile file `.hdt` (SPEC-014 section 4.1): five fixed sections and a trailing body hash.

use crate::{fnv1a64, TerrainError, TileId, CELL_MM, TILE_CELLS};
use core_math::Vec2Fx;

/// Vertices along one tile edge (cells plus one; edges are duplicated in neighbours).
pub const HEIGHT_SIDE: usize = TILE_CELLS as usize + 1;
/// Height samples per tile.
pub const HEIGHT_COUNT: usize = HEIGHT_SIDE * HEIGHT_SIDE;
/// Cells per tile.
pub const CELL_COUNT: usize = (TILE_CELLS * TILE_CELLS) as usize;

const MAGIC: &[u8; 4] = b"HDTL";
const VERSION: u16 = 1;
const SECTIONS: u8 = 5;
const HEADER_LEN: usize = 17;
const HASH_LEN: usize = 8;
const SEG_LEN: usize = 24;

/// One obstacle segment in world millimetres.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObstacleSeg {
    /// Stable segment id; records are stored ascending by it.
    pub id: u32,
    /// Obstacle kind (`data/rules/obstacles.toml`).
    pub kind: u8,
    /// Per-segment flags.
    pub flags: u8,
    /// Height above ground in millimetres.
    pub height_mm: u16,
    /// First endpoint `[x, y]`.
    pub a: [i32; 2],
    /// Second endpoint `[x, y]`.
    pub b: [i32; 2],
}

impl ObstacleSeg {
    /// The endpoints as position values (metres); [`crate::fx_to_mm`] returns the stored
    /// millimetres exactly.
    pub fn endpoints(&self) -> (Vec2Fx, Vec2Fx) {
        let p = |v: [i32; 2]| {
            Vec2Fx::new(
                crate::mm_to_fx(i64::from(v[0])),
                crate::mm_to_fx(i64::from(v[1])),
            )
        };
        (p(self.a), p(self.b))
    }
}

/// A decoded tile. The part lengths are fixed by the constructors, so `encode` cannot fail.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tile {
    id: TileId,
    heights: Vec<u16>,
    classes: Vec<u8>,
    occ: Vec<u8>,
    flags: Vec<u8>,
    obstacles: Vec<ObstacleSeg>,
}

fn format_err(msg: &str) -> TerrainError {
    TerrainError::Format(msg.to_string())
}

fn check_sorted(obstacles: &[ObstacleSeg]) -> Result<(), TerrainError> {
    if obstacles.windows(2).all(|w| w[0].id < w[1].id) {
        Ok(())
    } else {
        Err(format_err("obstacles are not strictly ascending by seg_id"))
    }
}

/// Bounds-checked little-endian reader; every read reports truncation instead of panicking.
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], TerrainError> {
        let end = self.at.checked_add(n).filter(|&e| e <= self.bytes.len());
        let end = end.ok_or_else(|| format_err("truncated"))?;
        let s = &self.bytes[self.at..end];
        self.at = end;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8, TerrainError> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, TerrainError> {
        self.take(2).map(|s| u16::from_le_bytes([s[0], s[1]]))
    }
    fn u32(&mut self) -> Result<u32, TerrainError> {
        self.take(4)
            .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    }
}

fn le32(s: &[u8]) -> [u8; 4] {
    [s[0], s[1], s[2], s[3]]
}

impl Tile {
    /// A tile with every height, class, cover and flag zero and no obstacles.
    pub fn new(id: TileId) -> Tile {
        Tile {
            id,
            heights: vec![0; HEIGHT_COUNT],
            classes: vec![0; CELL_COUNT],
            occ: vec![0; CELL_COUNT],
            flags: vec![0; CELL_COUNT],
            obstacles: Vec::new(),
        }
    }

    /// Builds a tile from its parts, checking every length and the obstacle order.
    pub fn from_parts(
        id: TileId,
        heights: Vec<u16>,
        classes: Vec<u8>,
        occ: Vec<u8>,
        flags: Vec<u8>,
        obstacles: Vec<ObstacleSeg>,
    ) -> Result<Tile, TerrainError> {
        if heights.len() != HEIGHT_COUNT
            || classes.len() != CELL_COUNT
            || occ.len() != CELL_COUNT
            || flags.len() != CELL_COUNT
        {
            return Err(format_err("wrong part length"));
        }
        check_sorted(&obstacles)?;
        Ok(Tile {
            id,
            heights,
            classes,
            occ,
            flags,
            obstacles,
        })
    }

    /// The tile address.
    pub fn id(&self) -> TileId {
        self.id
    }
    /// Vertex heights, `HEIGHT_SIDE` per row, row 0 south, centimetres above `z_origin_cm`.
    pub fn heights(&self) -> &[u16] {
        &self.heights
    }
    /// Mutable vertex heights (fixed length).
    pub fn heights_mut(&mut self) -> &mut [u16] {
        &mut self.heights
    }
    /// Terrain class id per cell, 128 per row, row 0 south.
    pub fn classes(&self) -> &[u8] {
        &self.classes
    }
    /// Mutable class ids (fixed length).
    pub fn classes_mut(&mut self) -> &mut [u8] {
        &mut self.classes
    }
    /// Height of sight-blocking cover above ground per cell, decimetres.
    pub fn occ(&self) -> &[u8] {
        &self.occ
    }
    /// Mutable cover heights (fixed length).
    pub fn occ_mut(&mut self) -> &mut [u8] {
        &mut self.occ
    }
    /// Cell flags: bit0 `NO_SURVEY`, bit1 `ADJ_RESTORE`, bit2 `ADJ_PLAY`, bit3 `POLISH`, bit4 `STEEP`.
    pub fn flags(&self) -> &[u8] {
        &self.flags
    }
    /// Mutable cell flags (fixed length).
    pub fn flags_mut(&mut self) -> &mut [u8] {
        &mut self.flags
    }
    /// Obstacle segments, strictly ascending by id.
    pub fn obstacles(&self) -> &[ObstacleSeg] {
        &self.obstacles
    }
    /// Replaces the obstacles; they must be strictly ascending by `id`.
    pub fn set_obstacles(&mut self, obstacles: Vec<ObstacleSeg>) -> Result<(), TerrainError> {
        check_sorted(&obstacles)?;
        self.obstacles = obstacles;
        Ok(())
    }

    /// The body hash stored in the encoded file.
    pub fn hash(&self) -> u64 {
        fnv1a64(&self.encode_body())
    }

    /// The `.hdt` bytes; equal tiles give byte-equal output.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = self.encode_body();
        let h = fnv1a64(&out);
        out.extend_from_slice(&h.to_le_bytes());
        out
    }

    fn encode_body(&self) -> Vec<u8> {
        let cap = HEADER_LEN
            + 5 * 5
            + HEIGHT_COUNT * 2
            + 3 * CELL_COUNT
            + 4
            + self.obstacles.len() * SEG_LEN;
        let mut o = Vec::with_capacity(cap + HASH_LEN);
        o.extend_from_slice(MAGIC);
        o.extend_from_slice(&VERSION.to_le_bytes());
        o.extend_from_slice(&self.id.x.to_le_bytes());
        o.extend_from_slice(&self.id.y.to_le_bytes());
        o.extend_from_slice(&(TILE_CELLS as u16).to_le_bytes());
        o.extend_from_slice(&(CELL_MM as u32).to_le_bytes());
        o.push(SECTIONS);
        let section = |o: &mut Vec<u8>, id: u8, len: usize| {
            o.push(id);
            o.extend_from_slice(&(len as u32).to_le_bytes());
        };
        section(&mut o, 1, HEIGHT_COUNT * 2);
        for h in &self.heights {
            o.extend_from_slice(&h.to_le_bytes());
        }
        for (id, part) in [(2, &self.classes), (3, &self.occ), (4, &self.flags)] {
            section(&mut o, id, CELL_COUNT);
            o.extend_from_slice(part);
        }
        section(&mut o, 5, 4 + self.obstacles.len() * SEG_LEN);
        o.extend_from_slice(&(self.obstacles.len() as u32).to_le_bytes());
        for s in &self.obstacles {
            o.extend_from_slice(&s.id.to_le_bytes());
            o.push(s.kind);
            o.push(s.flags);
            o.extend_from_slice(&s.height_mm.to_le_bytes());
            for v in [s.a[0], s.a[1], s.b[0], s.b[1]] {
                o.extend_from_slice(&v.to_le_bytes());
            }
        }
        o
    }

    /// Decodes and validates a tile file. Framing and header are checked first (every declared
    /// length against the remaining bytes, before any allocation), then the body hash, then the
    /// obstacle order. Never panics.
    pub fn decode(bytes: &[u8]) -> Result<Tile, TerrainError> {
        if bytes.len() < HEADER_LEN + HASH_LEN {
            return Err(format_err("truncated"));
        }
        let (body, tail) = bytes.split_at(bytes.len() - HASH_LEN);
        let mut r = Reader { bytes: body, at: 0 };
        if r.take(4)? != MAGIC {
            return Err(format_err("bad magic"));
        }
        if r.u16()? != VERSION {
            return Err(format_err("unknown version"));
        }
        let id = TileId {
            x: r.u16()?,
            y: r.u16()?,
        };
        if u32::from(r.u16()?) != TILE_CELLS || i64::from(r.u32()?) != CELL_MM {
            return Err(format_err("wrong cells or cell_mm"));
        }
        if r.u8()? != SECTIONS {
            return Err(format_err("wrong section count"));
        }
        let mut payloads: [&[u8]; 5] = [&[]; 5];
        for (i, slot) in payloads.iter_mut().enumerate() {
            if usize::from(r.u8()?) != i + 1 {
                return Err(format_err("missing, duplicate or misordered section"));
            }
            let len = r.u32()? as usize;
            *slot = r.take(len)?;
        }
        if r.at != body.len() {
            return Err(format_err("trailing bytes"));
        }
        let [h, c, o, f, ob] = payloads;
        if h.len() != HEIGHT_COUNT * 2 || [c, o, f].iter().any(|p| p.len() != CELL_COUNT) {
            return Err(format_err("wrong section length"));
        }
        let n = ob.get(..4).map(|s| u32::from_le_bytes(le32(s)));
        let n = n.ok_or_else(|| format_err("wrong section length"))?;
        if ob.len() as u64 != 4 + u64::from(n) * SEG_LEN as u64 {
            return Err(format_err("wrong section length"));
        }
        let stored = u64::from_le_bytes([
            tail[0], tail[1], tail[2], tail[3], tail[4], tail[5], tail[6], tail[7],
        ]);
        if fnv1a64(body) != stored {
            return Err(TerrainError::Hash);
        }
        let obstacles: Vec<ObstacleSeg> = ob[4..]
            .chunks_exact(SEG_LEN)
            .map(|s| {
                let i = |k: usize| i32::from_le_bytes(le32(&s[k..k + 4]));
                ObstacleSeg {
                    id: u32::from_le_bytes(le32(s)),
                    kind: s[4],
                    flags: s[5],
                    height_mm: u16::from_le_bytes([s[6], s[7]]),
                    a: [i(8), i(12)],
                    b: [i(16), i(20)],
                }
            })
            .collect();
        check_sorted(&obstacles)?;
        Ok(Tile {
            id,
            heights: h
                .chunks_exact(2)
                .map(|s| u16::from_le_bytes([s[0], s[1]]))
                .collect(),
            classes: c.to_vec(),
            occ: o.to_vec(),
            flags: f.to_vec(),
            obstacles,
        })
    }
}
