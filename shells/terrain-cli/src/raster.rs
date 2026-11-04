//! Compile stage 3: class and cover rasterisation (SPEC-014 section 5).
//!
//! Every feature becomes a paint operation. Operations apply in ascending
//! `(class priority, layer, id)`; a later operation overwrites class and cover of the cells it
//! takes. A cell is taken when its centre is inside the polygon (even-odd, half-open edges, minus
//! holes) or within the half width of a polyline (exact, round end caps).

use crate::geom::{inside_ring, near_polyline, Pt};
use crate::survey::{Survey, DEFAULT_OCC_DM, PRIORITY};
use terrain::CELL_MM;

/// Width of the marsh margin beside a creek channel: one cell (assumption, CT-014-3).
pub const CREEK_MARGIN_MM: i64 = CELL_MM;
const CLASS_MARSH: u8 = 5;
const CLASS_FORD: u8 = 6;
const CLASS_ROAD: u8 = 1;
const CLASS_BUILDING: u8 = 11;

/// Class and cover per cell over the whole extent, row 0 south.
pub struct Grid {
    pub cells_x: usize,
    pub cells_y: usize,
    pub class: Vec<u8>,
    pub occ: Vec<u8>,
}

enum Shape<'a> {
    Poly(&'a [Pt], &'a [Vec<Pt>]),
    Line(&'a [Pt], i64),
}

struct Op<'a> {
    class: u8,
    occ: u8,
    layer: u8,
    id: u32,
    shape: Shape<'a>,
}

fn bounds(pts: &[Pt], grow: i64) -> Option<(i64, i64, i64, i64)> {
    let x0 = pts.iter().map(|p| p[0]).min()?;
    let x1 = pts.iter().map(|p| p[0]).max()?;
    let y0 = pts.iter().map(|p| p[1]).min()?;
    let y1 = pts.iter().map(|p| p[1]).max()?;
    Some((x0 - grow, y0 - grow, x1 + grow, y1 + grow))
}

impl Grid {
    fn paint(&mut self, op: &Op) {
        let (pts, grow) = match op.shape {
            Shape::Poly(o, _) => (o, 0),
            Shape::Line(p, r) => (p, r),
        };
        let Some((x0, y0, x1, y1)) = bounds(pts, grow) else {
            return;
        };
        // Cell k has its centre at k * CELL_MM + CELL_MM / 2.
        let lo = |v: i64| (v - CELL_MM / 2).div_euclid(CELL_MM).max(0);
        let hi = |v: i64, n: usize| ((v - CELL_MM / 2).div_euclid(CELL_MM) + 1).min(n as i64 - 1);
        for cy in lo(y0)..=hi(y1, self.cells_y) {
            for cx in lo(x0)..=hi(x1, self.cells_x) {
                let c = [cx * CELL_MM + CELL_MM / 2, cy * CELL_MM + CELL_MM / 2];
                let taken = match op.shape {
                    Shape::Poly(o, holes) => {
                        inside_ring(o, c) && !holes.iter().any(|h| inside_ring(h, c))
                    }
                    Shape::Line(p, r) => near_polyline(p, c, r),
                };
                if taken {
                    let i = cy as usize * self.cells_x + cx as usize;
                    self.class[i] = op.class;
                    self.occ[i] = op.occ;
                }
            }
        }
    }
}

/// Rasterises the survey's polygons, roads, creeks and buildings.
pub fn rasterise(s: &Survey) -> Grid {
    let cells_x = s.tiles_x as usize * terrain::TILE_CELLS as usize;
    let cells_y = s.tiles_y as usize * terrain::TILE_CELLS as usize;
    let mut ops: Vec<Op> = Vec::new();
    let occ = |c: u8| DEFAULT_OCC_DM[c as usize];
    for p in &s.polygons {
        let o = p.occ_dm.unwrap_or(occ(p.class));
        ops.push(Op {
            class: p.class,
            occ: o,
            layer: 0,
            id: p.id,
            shape: Shape::Poly(&p.outer, &p.holes),
        });
    }
    for l in &s.lines {
        let line = |class: u8, r: i64| Op {
            class,
            occ: occ(class),
            layer: 1,
            id: l.id,
            shape: Shape::Line(&l.pts, r),
        };
        match l.kind {
            "road" => ops.push(line(CLASS_ROAD, l.half_width_mm)),
            "creek" => {
                ops.push(line(CLASS_MARSH, l.half_width_mm + CREEK_MARGIN_MM));
                ops.push(line(CLASS_FORD, l.half_width_mm));
            }
            _ => {}
        }
    }
    for b in &s.buildings {
        ops.push(Op {
            class: CLASS_BUILDING,
            occ: occ(CLASS_BUILDING),
            layer: 2,
            id: b.id,
            shape: Shape::Poly(&b.outer, &[]),
        });
    }
    let rank = |c: u8| PRIORITY.iter().position(|&p| p == c).unwrap_or(0);
    ops.sort_by_key(|o| (rank(o.class), o.layer, o.id));
    let mut g = Grid {
        cells_x,
        cells_y,
        class: vec![0; cells_x * cells_y],
        occ: vec![0; cells_x * cells_y],
    };
    for op in &ops {
        g.paint(op);
    }
    g
}
