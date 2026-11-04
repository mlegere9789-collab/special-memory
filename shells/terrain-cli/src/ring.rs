//! Exact polygon ring rules of SPEC-014 section 3: ring shape, hole placement and overlap of
//! polygons. Integer arithmetic only (`i128` products).
//!
//! Overlap is decided on interiors. Each edge of one polygon is cut at every point where it meets
//! an edge of the other (a rational parameter `n / d`); the point between two neighbouring cuts
//! (their mediant, which has a small denominator) is then located against the other polygon. A
//! piece strictly inside it, or lying along one of its edges in the same direction (interior on
//! the same side, given the winding rule), means the interiors overlap. Coordinates are taken to
//! lie within 2^28 mm (268 km), far beyond any survey extent, so every product fits `i128`.

use crate::geom::{inside_ring, Pt};

type V = [i128; 2];

fn sub(a: Pt, b: Pt) -> V {
    [
        i128::from(a[0]) - i128::from(b[0]),
        i128::from(a[1]) - i128::from(b[1]),
    ]
}

fn cross(a: V, b: V) -> i128 {
    a[0] * b[1] - a[1] * b[0]
}

fn dot(a: V, b: V) -> i128 {
    a[0] * b[0] + a[1] * b[1]
}

/// Twice the signed area: positive for a counter-clockwise ring.
pub fn area2(ring: &[Pt]) -> i128 {
    (0..ring.len())
        .map(|k| {
            let (a, b) = (ring[k], ring[(k + 1) % ring.len()]);
            i128::from(a[0]) * i128::from(b[1]) - i128::from(b[0]) * i128::from(a[1])
        })
        .sum()
}

fn edges(ring: &[Pt]) -> impl Iterator<Item = (Pt, Pt)> + '_ {
    (0..ring.len()).map(move |k| (ring[k], ring[(k + 1) % ring.len()]))
}

/// Whether the closed segments `a`-`b` and `c`-`d` share any point.
fn touch(a: Pt, b: Pt, c: Pt, d: Pt) -> bool {
    let s = |p: Pt, q: Pt, r: Pt| cross(sub(q, p), sub(r, p)).signum();
    let on = |p: Pt, q: Pt, r: Pt| {
        r[0] >= p[0].min(q[0])
            && r[0] <= p[0].max(q[0])
            && r[1] >= p[1].min(q[1])
            && r[1] <= p[1].max(q[1])
    };
    let (o1, o2, o3, o4) = (s(a, b, c), s(a, b, d), s(c, d, a), s(c, d, b));
    (o1 * o2 < 0 && o3 * o4 < 0)
        || (o1 == 0 && on(a, b, c))
        || (o2 == 0 && on(a, b, d))
        || (o3 == 0 && on(c, d, a))
        || (o4 == 0 && on(c, d, b))
}

/// The first rule a ring breaks, as an error name and message. `ccw` is the required winding
/// (outer rings counter-clockwise, holes clockwise).
pub fn ring_error(ring: &[Pt], ccw: bool) -> Option<(&'static str, String)> {
    let n = ring.len();
    if n < 3 {
        return Some((
            "ring_short",
            format!("{n} vertices; a ring needs 3 or more"),
        ));
    }
    if let Some((a, _)) = edges(ring).find(|(a, b)| a == b) {
        let msg = format!("zero-length edge at ({}, {})", a[0], a[1]);
        return Some(("edge_zero", msg));
    }
    let area = area2(ring);
    if area == 0 {
        return Some(("ring_flat", "the ring encloses no area".into()));
    }
    for i in 0..n {
        let (a, b) = (ring[i], ring[(i + 1) % n]);
        let c = ring[(i + 2) % n];
        let (e, f) = (sub(b, a), sub(c, b));
        if cross(e, f) == 0 && dot(e, f) < 0 {
            return Some((
                "ring_self_touch",
                format!("edge folds back at ({}, {})", b[0], b[1]),
            ));
        }
        for j in i + 2..n {
            if i == 0 && j == n - 1 {
                continue;
            }
            if touch(a, b, ring[j], ring[(j + 1) % n]) {
                return Some((
                    "ring_self_touch",
                    format!("edges {} and {} cross or touch", i + 1, j + 1),
                ));
            }
        }
    }
    if (area > 0) != ccw {
        let want = if ccw {
            "counter-clockwise"
        } else {
            "clockwise"
        };
        return Some(("ring_winding", format!("must be listed {want}")));
    }
    None
}

/// Whether `hole` fails to lie strictly inside `outer` (both already valid rings).
pub fn hole_outside(outer: &[Pt], hole: &[Pt]) -> bool {
    let meets = edges(hole).any(|(a, b)| edges(outer).any(|(c, d)| touch(a, b, c, d)));
    meets || !inside_ring(outer, hole[0])
}

/// A polygon as its outer ring followed by its holes.
pub type Rings<'a> = Vec<&'a [Pt]>;

enum Spot {
    Inside,
    Outside,
    /// On this edge of the polygon.
    On(Pt, Pt),
}

/// Locates the point `(x / d, y / d)`, `d > 0`, against a polygon (even-odd over all rings).
fn locate(poly: &[&[Pt]], x: i128, y: i128, d: i128) -> Spot {
    let mut inside = false;
    for ring in poly {
        for (a, b) in edges(ring) {
            let (ax, ay) = (i128::from(a[0]) * d, i128::from(a[1]) * d);
            let (bx, by) = (i128::from(b[0]) * d, i128::from(b[1]) * d);
            let (e, q) = ([bx - ax, by - ay], [x - ax, y - ay]);
            if cross(e, q) == 0
                && x >= ax.min(bx)
                && x <= ax.max(bx)
                && y >= ay.min(by)
                && y <= ay.max(by)
            {
                return Spot::On(a, b);
            }
            if (ay <= y) != (by <= y) {
                let (lo, hi) = if ay < by { (a, b) } else { (b, a) };
                let (lo_x, lo_y) = (i128::from(lo[0]) * d, i128::from(lo[1]) * d);
                let e = [i128::from(hi[0]) * d - lo_x, i128::from(hi[1]) * d - lo_y];
                if cross(e, [x - lo_x, y - lo_y]) > 0 {
                    inside = !inside;
                }
            }
        }
    }
    if inside {
        Spot::Inside
    } else {
        Spot::Outside
    }
}

/// Cut parameters `(n, d)`, `d > 0`, of the edge `p`-`q` against the edge `c`-`d`.
fn cuts(p: Pt, q: Pt, c: Pt, d: Pt, out: &mut Vec<(i128, i128)>) {
    let (e, f, w) = (sub(q, p), sub(d, c), sub(c, p));
    let den = cross(e, f);
    if den == 0 {
        // Parallel: a collinear edge cuts at its end points.
        if cross(e, w) == 0 {
            let len2 = dot(e, e);
            for r in [c, d] {
                out.push((dot(sub(r, p), e), len2));
            }
        }
        return;
    }
    let sign = den.signum();
    let (n, u) = (cross(w, f) * sign, cross(w, e) * sign);
    let den = den * sign;
    if u >= 0 && u <= den {
        out.push((n, den));
    }
}

/// Whether some part of the boundary of `a` lies strictly inside `b` or along an edge of `b` in
/// the same direction.
fn boundary_in(a: &[&[Pt]], b: &[&[Pt]]) -> bool {
    for ring in a {
        for (p, q) in edges(ring) {
            let mut ts: Vec<(i128, i128)> = vec![(0, 1), (1, 1)];
            for other in b {
                for (c, d) in edges(other) {
                    cuts(p, q, c, d, &mut ts);
                }
            }
            ts.retain(|&(n, d)| n >= 0 && n <= d);
            ts.sort_by(|x, y| (x.0 * y.1).cmp(&(y.0 * x.1)));
            ts.dedup_by(|x, y| x.0 * y.1 == y.0 * x.1);
            let e = sub(q, p);
            for w in ts.windows(2) {
                let (n, d) = (w[0].0 + w[1].0, w[0].1 + w[1].1);
                let x = i128::from(p[0]) * d + e[0] * n;
                let y = i128::from(p[1]) * d + e[1] * n;
                match locate(b, x, y, d) {
                    Spot::Inside => return true,
                    Spot::On(c, dd) if dot(e, sub(dd, c)) > 0 => return true,
                    _ => {}
                }
            }
        }
    }
    false
}

/// Whether the interiors of two polygons (outer ring, then holes) share any area. Polygons that
/// meet only along edges or at corners do not overlap.
pub fn overlap(a: &[&[Pt]], b: &[&[Pt]]) -> bool {
    boundary_in(a, b) || boundary_in(b, a)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn touch_covers_crossing_end_contact_and_overlap() {
        assert!(touch([0, 0], [10, 10], [0, 10], [10, 0]));
        assert!(touch([0, 0], [10, 0], [10, 0], [20, 5]));
        assert!(touch([0, 0], [10, 0], [5, 0], [15, 0]));
        assert!(!touch([0, 0], [10, 0], [11, 0], [20, 0]));
        assert!(!touch([0, 0], [10, 0], [0, 1], [10, 1]));
    }

    #[test]
    fn cuts_are_exact_rational_parameters() {
        let mut v = Vec::new();
        cuts([0, 0], [30, 0], [10, -5], [10, 5], &mut v);
        assert_eq!(v, vec![(100, 300)]);
        v.clear();
        cuts([0, 0], [30, 0], [40, -5], [40, 5], &mut v);
        assert_eq!(v, vec![(400, 300)]);
    }

    #[test]
    fn area_sign_gives_winding() {
        let sq = [[0, 0], [4, 0], [4, 4], [0, 4]];
        assert_eq!(area2(&sq), 32);
        assert!(ring_error(&sq, true).is_none());
        assert_eq!(ring_error(&sq, false).map(|e| e.0), Some("ring_winding"));
    }
}
