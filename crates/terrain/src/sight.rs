//! Obstacle queries and line of sight (SPEC-014 sections 5 and 11).
//!
//! Both read only the tiles under the bounding box of what they are asked about, so a missing
//! tile is reported (lowest id first) and any extra resident tile changes nothing.
//!
//! Segment flags: the compiler sets [`SEG_FIRE_BLOCKING`] and [`SEG_SIGHT_BLOCKING`] from the
//! `fire_blocking` and `sight_blocking` columns of `data/rules/obstacles.toml`, so this crate
//! needs no kind table.

use crate::{fx_to_mm, ObstacleSeg, TerrainError, TerrainWorld};
use core_math::Vec2Fx;

/// Segment flag bit: stops direct fire.
pub const SEG_FIRE_BLOCKING: u8 = 1;
/// Segment flag bit: stops sight.
pub const SEG_SIGHT_BLOCKING: u8 = 2;

/// Largest coordinate difference or eye height `los` accepts, millimetres (keeps the sample
/// arithmetic inside `i64`).
const LOS_LIMIT_MM: i64 = 1 << 31;

/// Which obstacles and cover a line of sight respects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LosMode {
    /// Ground and `fire_blocking` obstacles.
    Fire,
    /// Ground, cover blocks and `sight_blocking` obstacles.
    Sight,
}

/// What stopped a line of sight.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blocker {
    /// Nothing.
    None,
    /// The ground itself.
    Ground,
    /// Cover (woods, buildings) in a cell, in `Sight` mode.
    Occluder,
    /// The obstacle segment with this id.
    Obstacle(u32),
}

/// Result of [`TerrainWorld::los`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LosResult {
    /// True when nothing blocks.
    pub clear: bool,
    /// Distance from `from` to the nearest blocker in millimetres; 0 when clear.
    pub blocked_at_mm: i64,
    /// The nearest blocker.
    pub blocker: Blocker,
}

fn cross(ax: i128, ay: i128, bx: i128, by: i128) -> i128 {
    ax * by - ay * bx
}

/// Where the ray `f -> t` first meets the closed segment `seg`, as `(num, den)` with
/// `0 <= num <= den`, `den > 0`: the ray point is `f + (t - f) * num / den`. A collinear overlap
/// meets at its first shared point; a zero-length ray meets nothing.
fn ray_hit(f: (i64, i64), t: (i64, i64), seg: &ObstacleSeg) -> Option<(i128, i128)> {
    let (rx, ry) = (i128::from(t.0 - f.0), i128::from(t.1 - f.1));
    let (sx, sy) = (
        i128::from(seg.b[0]) - i128::from(seg.a[0]),
        i128::from(seg.b[1]) - i128::from(seg.a[1]),
    );
    if rx == 0 && ry == 0 {
        return None;
    }
    let qx = i128::from(seg.a[0]) - i128::from(f.0);
    let qy = i128::from(seg.a[1]) - i128::from(f.1);
    let den = cross(rx, ry, sx, sy);
    if den == 0 {
        if cross(qx, qy, rx, ry) != 0 {
            return None;
        }
        let qa = qx * rx + qy * ry;
        let qb = (qx + sx) * rx + (qy + sy) * ry;
        let (lo, hi) = (qa.min(qb).max(0), qa.max(qb).min(rx * rx + ry * ry));
        return (lo <= hi).then_some((lo, rx * rx + ry * ry));
    }
    let (tn, un, den) = (cross(qx, qy, sx, sy), cross(qx, qy, rx, ry), den);
    let (tn, un, den) = if den < 0 {
        (-tn, -un, -den)
    } else {
        (tn, un, den)
    };
    ((0..=den).contains(&tn) && (0..=den).contains(&un)).then_some((tn, den))
}

impl TerrainWorld {
    /// Fills `out` (cleared first) with the obstacle segments whose bounding box meets the
    /// rectangle `min..=max`, ascending by `seg_id`, each once however many tiles store it.
    /// Every tile under the rectangle must be resident; otherwise the lowest missing id is
    /// returned as [`TerrainError::TileMissing`].
    pub fn obstacles_in(
        &self,
        min: Vec2Fx,
        max: Vec2Fx,
        out: &mut Vec<ObstacleSeg>,
    ) -> Result<(), TerrainError> {
        self.obstacles_in_mm(
            (fx_to_mm(min.x), fx_to_mm(min.y)),
            (fx_to_mm(max.x), fx_to_mm(max.y)),
            out,
        )
    }

    fn obstacles_in_mm(
        &self,
        min: (i64, i64),
        max: (i64, i64),
        out: &mut Vec<ObstacleSeg>,
    ) -> Result<(), TerrainError> {
        out.clear();
        let ids = self.tiles_for_rect_mm(min, max);
        if let Some(missing) = ids.iter().find(|id| !self.is_resident(**id)) {
            return Err(TerrainError::TileMissing(*missing));
        }
        for id in ids {
            for s in self.tiles[&id].obstacles() {
                let (x0, x1) = (i64::from(s.a[0].min(s.b[0])), i64::from(s.a[0].max(s.b[0])));
                let (y0, y1) = (i64::from(s.a[1].min(s.b[1])), i64::from(s.a[1].max(s.b[1])));
                if x1 >= min.0 && x0 <= max.0 && y1 >= min.1 && y0 <= max.1 {
                    out.push(*s);
                }
            }
        }
        out.sort_by_key(|s| s.id);
        out.dedup_by_key(|s| s.id);
        Ok(())
    }

    /// Line of sight from `from` at `from_z_mm` above local ground to `to` at `to_z_mm`.
    ///
    /// Every tile under the bounding box of the two points must be resident (else the lowest
    /// missing id is returned). Coordinate differences and eye heights beyond 2^31 mm are
    /// refused with [`TerrainError::OutOfExtent`].
    pub fn los(
        &self,
        from: Vec2Fx,
        from_z_mm: i64,
        to: Vec2Fx,
        to_z_mm: i64,
        mode: LosMode,
    ) -> Result<LosResult, TerrainError> {
        let f = (fx_to_mm(from.x), fx_to_mm(from.y));
        let t = (fx_to_mm(to.x), fx_to_mm(to.y));
        let (dx, dy) = (t.0 - f.0, t.1 - f.1);
        if [dx, dy, from_z_mm, to_z_mm]
            .iter()
            .any(|v| v.abs() > LOS_LIMIT_MM)
        {
            return Err(TerrainError::OutOfExtent);
        }
        let bit = match mode {
            LosMode::Fire => SEG_FIRE_BLOCKING,
            LosMode::Sight => SEG_SIGHT_BLOCKING,
        };
        let mut segs = Vec::new();
        self.obstacles_in_mm(
            (f.0.min(t.0), f.1.min(t.1)),
            (f.0.max(t.0), f.1.max(t.1)),
            &mut segs,
        )?;
        segs.retain(|s| s.flags & bit != 0);

        let (sf, st) = (self.sample_mm(f.0, f.1)?, self.sample_mm(t.0, t.1)?);
        let (a0, a1) = (sf.height + from_z_mm, st.height + to_z_mm);
        let d = (dx as i128 * dx as i128 + dy as i128 * dy as i128).isqrt() as i64;
        let n = ((d + crate::SIGHT_STEP_MM - 1) / crate::SIGHT_STEP_MM).max(1);

        // First blocking sample: (k, blocker).
        let mut ground = None;
        for k in 1..n {
            let (px, py) = (f.0 + (dx * k).div_euclid(n), f.1 + (dy * k).div_euclid(n));
            let ray = a0 + ((a1 - a0) * k).div_euclid(n);
            let s = self.sample_mm(px, py)?;
            if s.height > ray {
                ground = Some((k, Blocker::Ground));
                break;
            }
            if mode == LosMode::Sight
                && s.cell != sf.cell
                && s.cell != st.cell
                && s.height + s.occ > ray
            {
                ground = Some((k, Blocker::Occluder));
                break;
            }
        }

        // Nearest blocking obstacle: (num, den, id); ascending ids keep the lowest on a tie.
        let mut best: Option<(i128, i128, u32)> = None;
        for s in &segs {
            let Some((tn, td)) = ray_hit(f, t, s) else {
                continue;
            };
            let at = |v: i64| i128::from(v);
            let ix = f.0 + (at(dx) * tn).div_euclid(td) as i64;
            let iy = f.1 + (at(dy) * tn).div_euclid(td) as i64;
            let ray = at(a0) + (at(a1 - a0) * tn).div_euclid(td);
            if ray <= at(self.height_at_mm(ix, iy)?) + at(i64::from(s.height_mm))
                && best.is_none_or(|(bn, bd, _)| tn * bd < bn * td)
            {
                best = Some((tn, td, s.id));
            }
        }

        let hit = match (best, ground) {
            (Some((tn, td, id)), g)
                if g.is_none_or(|(k, _)| tn * i128::from(n) <= i128::from(k) * td) =>
            {
                Some((
                    (i128::from(d) * tn).div_euclid(td) as i64,
                    Blocker::Obstacle(id),
                ))
            }
            (_, Some((k, b))) => Some(((k * d) / n, b)),
            _ => None,
        };
        Ok(match hit {
            Some((at_mm, blocker)) => LosResult {
                clear: false,
                blocked_at_mm: at_mm,
                blocker,
            },
            None => LosResult {
                clear: true,
                blocked_at_mm: 0,
                blocker: Blocker::None,
            },
        })
    }
}
