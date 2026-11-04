//! `hd-terrain check`: survey-level and line geometry rules of SPEC-014 section 3.
//!
//! Error names: `id_duplicate`, `voids_high`, `slab_uncovered`, `source_uncleared`,
//! `sources_unreadable`, `compile`, `line_short`, `road_dangling`, `creek_uphill`. The polygon
//! ring rules (`ring_*`, `hole_outside`, `group_overlap`) are a separate ticket.

use crate::compile::{build_pack, Pack};
use crate::geom::{near_polyline, Pt};
use crate::survey::{Line, Survey};
use core_math::Vec2Fx;
use std::fmt;
use std::path::Path;
use terrain::{mm_to_fx, TerrainIndex, TerrainWorld, TILE_MM};

/// Largest rise along a creek's flow between consecutive vertices, centimetres (CT-014-6).
pub const CREEK_RISE_TOL_CM: i64 = 100;
/// A road end this close to another road or the extent edge is joined, millimetres.
pub const ROAD_JOIN_MM: i64 = 1000;
/// Voids allowed in the elevation raster, parts per million.
pub const VOID_PPM_MAX: u64 = 1000;

/// One failed rule on one row.
pub struct Problem {
    pub name: &'static str,
    pub file: &'static str,
    pub id: u32,
    pub message: String,
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} {} id {}: {}",
            self.name, self.file, self.id, self.message
        )
    }
}

fn problem(name: &'static str, file: &'static str, id: u32, message: impl Into<String>) -> Problem {
    Problem {
        name,
        file,
        id,
        message: message.into(),
    }
}

fn duplicates(out: &mut Vec<Problem>, file: &'static str, mut ids: Vec<u32>) {
    ids.sort_unstable();
    for w in ids.windows(2).filter(|w| w[0] == w[1]) {
        out.push(problem(
            "id_duplicate",
            file,
            w[0],
            "id used by more than one row",
        ));
    }
}

fn dangling(s: &Survey, road: &Line, end: Pt, out: &mut Vec<Problem>) {
    let (w, h) = (
        i64::from(s.tiles_x) * TILE_MM,
        i64::from(s.tiles_y) * TILE_MM,
    );
    let at_edge = end[0] <= ROAD_JOIN_MM
        || end[1] <= ROAD_JOIN_MM
        || end[0] >= w - ROAD_JOIN_MM
        || end[1] >= h - ROAD_JOIN_MM;
    let joined = s
        .lines
        .iter()
        .any(|o| o.kind == "road" && o.id != road.id && near_polyline(&o.pts, end, ROAD_JOIN_MM));
    if !(at_edge || joined) {
        out.push(problem(
            "road_dangling",
            "linear.toml",
            road.id,
            format!("end ({}, {}) joins nothing", end[0], end[1]),
        ));
    }
}

fn line_rules(s: &Survey, world: Option<&TerrainWorld>, out: &mut Vec<Problem>) {
    for l in &s.lines {
        if l.pts.len() < 2 || l.pts.windows(2).any(|w| w[0] == w[1]) {
            out.push(problem(
                "line_short",
                "linear.toml",
                l.id,
                "needs 2 or more vertices and no zero-length segment",
            ));
            continue;
        }
        if l.kind == "road" && !l.dead_end {
            dangling(s, l, l.pts[0], out);
            dangling(s, l, l.pts[l.pts.len() - 1], out);
        }
        let (true, Some(w)) = (l.kind == "creek", world) else {
            continue;
        };
        let at = |p: Pt| w.height_mm(Vec2Fx::new(mm_to_fx(p[0]), mm_to_fx(p[1])));
        for pair in l.pts.windows(2) {
            let (Ok(a), Ok(b)) = (at(pair[0]), at(pair[1])) else {
                continue;
            };
            if b - a > CREEK_RISE_TOL_CM * 10 {
                out.push(problem(
                    "creek_uphill",
                    "linear.toml",
                    l.id,
                    format!("flow rises {} mm", b - a),
                ));
            }
        }
    }
}

fn raster_rules(s: &Survey, out: &mut Vec<Problem>) {
    let total = s.dem.len() as u64;
    let voids = s.dem.iter().filter(|&&p| p == i32::MIN).count() as u64;
    if voids * 1_000_000 > VOID_PPM_MAX * total {
        out.push(problem(
            "voids_high",
            "dem_cm.i32",
            0,
            format!("{voids} void posts of {total}"),
        ));
    }
    for (n, sl) in s.slabs.iter().enumerate() {
        // Posts 0 ..= 512 per tile along each axis.
        let span = |a: i64, b: i64, limit: usize| {
            (a * 512) as usize..=(((b + 1) * 512) as usize).min(limit - 1)
        };
        let (xs, ys) = (
            span(sl.x0, sl.x1, s.width_posts),
            span(sl.y0, sl.y1, s.height_posts),
        );
        let hole = ys
            .clone()
            .any(|j| xs.clone().any(|i| s.coverage[j * s.width_posts + i] == 0));
        if hole {
            out.push(problem(
                "slab_uncovered",
                "coverage.u8",
                n as u32 + 1,
                format!("slab `{}` has a post with source 0", sl.id),
            ));
        }
    }
}

fn source_rules(path: &Path, out: &mut Vec<Problem>) {
    let doc = std::fs::read_to_string(path)
        .map_err(|e| e.to_string())
        .and_then(|t| t.parse::<toml::Table>().map_err(|e| e.to_string()));
    let doc = match doc {
        Ok(d) => d,
        Err(e) => return out.push(problem("sources_unreadable", "sources.toml", 0, e)),
    };
    let rows = doc
        .get("source")
        .and_then(|v| v.as_array())
        .map_or(&[][..], |a| a.as_slice());
    for r in rows {
        let id = r.get("id").and_then(|v| v.as_integer()).unwrap_or(0) as u32;
        if r.get("licence_status").and_then(|v| v.as_str()) != Some("cleared") {
            out.push(problem(
                "source_uncleared",
                "sources.toml",
                id,
                "licence_status is not cleared",
            ));
        }
    }
}

/// Runs every rule and also returns the compiled pack when it could be built.
pub fn check_pack(s: &Survey, sources: Option<&Path>) -> (Vec<Problem>, Option<Pack>) {
    let mut out = Vec::new();
    duplicates(
        &mut out,
        "landcover.toml",
        s.polygons.iter().map(|p| p.id).collect(),
    );
    duplicates(
        &mut out,
        "linear.toml",
        s.lines.iter().map(|l| l.id).collect(),
    );
    duplicates(
        &mut out,
        "buildings.toml",
        s.buildings.iter().map(|b| b.id).collect(),
    );
    duplicates(&mut out, "names.toml", s.name_ids.clone());
    raster_rules(s, &mut out);
    let pack = match build_pack(s) {
        Ok(p) => Some(p),
        Err(e) => {
            out.push(problem("compile", "dem_cm.i32", 0, e));
            None
        }
    };
    let world = pack.as_ref().and_then(|p| {
        let mut w = TerrainWorld::new(TerrainIndex::from_toml_str(&p.index_text).ok()?);
        p.tiles
            .iter()
            .try_for_each(|t| w.insert_tile(&t.encode()).map(|_| ()))
            .ok()?;
        Some(w)
    });
    line_rules(s, world.as_ref(), &mut out);
    if let Some(p) = sources {
        source_rules(p, &mut out);
    }
    (out, pack)
}

/// Runs every rule; an empty list means the survey passes.
pub fn check(s: &Survey, sources: Option<&Path>) -> Vec<Problem> {
    check_pack(s, sources).0
}
