//! Exact integer geometry on millimetre points (`i128` products; no floating point).

/// A point `[x, y]` in world millimetres.
pub type Pt = [i64; 2];

fn sub(a: Pt, b: Pt) -> [i128; 2] {
    [
        i128::from(a[0]) - i128::from(b[0]),
        i128::from(a[1]) - i128::from(b[1]),
    ]
}

/// Squared distance between two points.
pub fn dist2(a: Pt, b: Pt) -> i128 {
    let d = sub(a, b);
    d[0] * d[0] + d[1] * d[1]
}

/// Even-odd containment of `(px, py)` in a ring (vertices listed without repeating the first).
///
/// An edge counts when `lo.y <= py < hi.y`, and a point exactly on it counts as to its left, so
/// the lower and western boundaries belong to the ring and the upper and eastern ones do not.
pub fn inside_ring(ring: &[Pt], p: Pt) -> bool {
    let mut inside = false;
    for k in 0..ring.len() {
        let (a, b) = (ring[k], ring[(k + 1) % ring.len()]);
        if (a[1] <= p[1]) == (b[1] <= p[1]) {
            continue;
        }
        let (lo, hi) = if a[1] < b[1] { (a, b) } else { (b, a) };
        let (e, q) = (sub(hi, lo), sub(p, lo));
        if e[0] * q[1] - e[1] * q[0] > 0 {
            inside = !inside;
        }
    }
    inside
}

/// Whether the exact distance from `p` to segment `a`-`b` is at most `r`: `cross^2 <= r^2 len^2`
/// along the segment and squared endpoint distance past its ends.
pub fn near_segment(a: Pt, b: Pt, p: Pt, r: i64) -> bool {
    let r2 = i128::from(r) * i128::from(r);
    let (e, q) = (sub(b, a), sub(p, a));
    let len2 = e[0] * e[0] + e[1] * e[1];
    let dot = e[0] * q[0] + e[1] * q[1];
    if len2 == 0 || dot <= 0 {
        return dist2(p, a) <= r2;
    }
    if dot >= len2 {
        return dist2(p, b) <= r2;
    }
    let cross = e[0] * q[1] - e[1] * q[0];
    cross * cross <= r2 * len2
}

/// Whether `p` is within `r` of any segment of the polyline.
pub fn near_polyline(pts: &[Pt], p: Pt, r: i64) -> bool {
    pts.windows(2).any(|w| near_segment(w[0], w[1], p, r))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn square_boundary_convention() {
        let sq = [[0, 0], [10, 0], [10, 10], [0, 10]];
        assert!(inside_ring(&sq, [5, 5]));
        assert!(inside_ring(&sq, [0, 5]) && inside_ring(&sq, [5, 0]));
        assert!(!inside_ring(&sq, [10, 5]) && !inside_ring(&sq, [5, 10]));
        assert!(!inside_ring(&sq, [-1, 5]));
    }

    #[test]
    fn distance_is_exact_at_the_limit() {
        assert!(near_segment([0, 0], [10, 0], [5, 3], 3));
        assert!(!near_segment([0, 0], [10, 0], [5, 4], 3));
        assert!(
            near_segment([0, 0], [10, 0], [-2, 2], 3) && !near_segment([0, 0], [10, 0], [-3, 3], 3)
        );
    }
}
