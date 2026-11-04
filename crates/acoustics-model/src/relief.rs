//! Terrain along a path: the elevation lookup, the knife-edge barrier and the foliage loss
//! (SPEC-012 section 6 items 4b and 4c).
//!
//! Positions are millimetres, heights above the local ground in millimetres, losses cdB. All
//! arithmetic is integer; divisions truncate toward zero unless a function says otherwise.

use crate::intmath::{cdb_of_power, isqrt_u128};
use crate::tables::AcousticTables;
use crate::weather::to_mm;
use core_math::Vec2Fx;

/// A read-only grid of ground elevation and terrain class, row-major with row 0 at the origin.
/// Until the single elevation source is chosen (decision D-02) a scene supplies it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Relief {
    /// South-west corner of cell (0, 0), metres.
    pub origin: Vec2Fx,
    /// Cell edge, mm.
    pub cell_mm: i32,
    /// Cells along x.
    pub width: u32,
    /// Cells along y.
    pub height: u32,
    /// Ground elevation per cell, mm.
    pub elev_mm: Vec<i32>,
    /// Terrain class per cell (SPEC-004 section 5).
    pub class: Vec<u8>,
}

impl Relief {
    /// Index of the cell holding `p_mm`: `floor((p - origin) / cell_mm)` per axis, clamped to
    /// the grid. `None` for an empty grid or a non-positive cell.
    fn cell_index(&self, p_mm: [i64; 2]) -> Option<usize> {
        if self.cell_mm <= 0 || self.width == 0 || self.height == 0 {
            return None;
        }
        let cell = i64::from(self.cell_mm);
        let axis = |p: i64, o: Vec2Fx, y: bool, n: u32| {
            let o = to_mm(if y { o.y } else { o.x });
            (p - o).div_euclid(cell).clamp(0, i64::from(n) - 1)
        };
        let ix = axis(p_mm[0], self.origin, false, self.width);
        let iy = axis(p_mm[1], self.origin, true, self.height);
        usize::try_from(iy * i64::from(self.width) + ix).ok()
    }

    /// Ground elevation at `p_mm`, mm (0 when the grid has no such cell).
    pub fn elevation_mm(&self, p_mm: [i64; 2]) -> i64 {
        let e = self.cell_index(p_mm).and_then(|i| self.elev_mm.get(i));
        e.map_or(0, |&v| i64::from(v))
    }

    /// Terrain class at `p_mm` (0 when the grid has no such cell).
    pub fn class_at(&self, p_mm: [i64; 2]) -> u8 {
        let c = self.cell_index(p_mm).and_then(|i| self.class.get(i));
        c.map_or(0, |&v| v)
    }
}

/// The samples of a path: `n` segments, interior samples `i = 1..n-1`.
struct Profile {
    n: i64,
    seg_mm: i64,
    zs: i64,
    zl: i64,
    s: [i64; 2],
    l: [i64; 2],
}

impl Profile {
    fn new(
        t: &AcousticTables,
        r: &Relief,
        s: [i64; 2],
        hs: i64,
        l: [i64; 2],
        hr: i64,
        d: i64,
    ) -> Self {
        let g = t.globals();
        let step = g.profile_step_mm.max(1);
        let n = ((d + step - 1) / step).clamp(1, i64::from(g.max_samples).max(1));
        Profile {
            n,
            seg_mm: d / n,
            zs: r.elevation_mm(s) + hs,
            zl: r.elevation_mm(l) + hr,
            s,
            l,
        }
    }

    /// Position of sample `i`: `S + (L - S) * i / n`.
    fn point(&self, i: i64) -> [i64; 2] {
        [0, 1].map(|k| self.s[k] + (self.l[k] - self.s[k]) * i / self.n)
    }

    /// Height of the straight line of sight above datum at sample `i`.
    fn line(&self, i: i64) -> i64 {
        self.zs + (self.zl - self.zs) * i / self.n
    }
}

/// `E(a)` in Q8 mm: `sqrt(a^2 + h^2)`.
fn edge_q8(a: i64, h: i64) -> i128 {
    let (a, h) = (i128::from(a), i128::from(h));
    isqrt_u128(((a * a + h * h) << 16) as u128) as i128
}

/// Terrain loss per band for a path of length `d_mm` from `s_mm` (source height `hs_mm`) to
/// `l_mm` (listener height `hr_mm`) at sound speed `c_mm_s`: the knife-edge barrier loss of the
/// dominant sample plus the foliage loss of all samples below a canopy.
pub fn terrain_loss_cdb(
    t: &AcousticTables,
    relief: &Relief,
    c_mm_s: i64,
    path: ([i64; 2], i64, [i64; 2], i64),
    d_mm: i64,
) -> [i32; 3] {
    let (s_mm, hs, l_mm, hr) = path;
    let p = Profile::new(t, relief, s_mm, hs, l_mm, hr, d_mm);
    let mut best: Option<(i64, i64)> = None; // (h, i), ties to the smaller i
    let mut foliage: i128 = 0;
    for i in 1..p.n {
        let pt = p.point(i);
        let e = relief.elevation_mm(pt);
        let line = p.line(i);
        let h = e - line;
        if best.map_or(h > 0, |(bh, _)| h > bh) {
            best = Some((h, i));
        }
        if let Some(row) = t.ground(relief.class_at(pt)) {
            if line - e < row.canopy_mm {
                foliage += i128::from(row.foliage_cdb_per_100m) * i128::from(p.seg_mm);
            }
        }
    }
    let barrier = |f_hz: i64| -> i64 {
        let Some((h, i)) = best else { return 0 };
        let s = d_mm * i / p.n;
        let delta = (edge_q8(s, h) + edge_q8(d_mm - s, h) - (i128::from(d_mm) << 8)).max(0);
        let lambda = i128::from((c_mm_s / f_hz.max(1)).max(1));
        let n_q16 = delta * 512 / lambda;
        let power = (3 * 65_536 + 20 * n_q16).min(1 << 100) as u128;
        i64::from(cdb_of_power(power)).min(t.globals().barrier_cap_cdb)
    };
    let mut out = [0i32; 3];
    for (b, o) in out.iter_mut().enumerate() {
        let band = &t.bands()[b];
        let fol = foliage * i128::from(band.foliage_pm) / (100_000 * 1000);
        *o = (barrier(band.f_hz) + fol as i64) as i32;
    }
    out
}
