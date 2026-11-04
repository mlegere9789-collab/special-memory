//! SPEC-002 Amendment A2: `SpatialGrid::query_segment` against a brute-force reference (T5-07).

use core_math::{Fx, Vec2Fx};
use proptest::prelude::*;
use sim_state::{AgentId, AgentInit, AgentStore, SpatialGrid};

fn v(x: i64, y: i64) -> Vec2Fx {
    Vec2Fx {
        x: Fx::from_raw(x),
        y: Fx::from_raw(y),
    }
}

fn isqrt(n: i128) -> i128 {
    let (mut lo, mut hi) = (0i128, 1i128 << 64);
    while lo < hi {
        let mid = (lo + hi + 1) / 2;
        if mid * mid <= n {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    lo
}

/// Brute force over every alive agent; exact in `i128` for coordinates within +-2^30 raw.
fn brute(store: &AgentStore, p0: Vec2Fx, p1: Vec2Fx, hw: Fx) -> Vec<AgentId> {
    let h = i128::from(hw.raw());
    if h < 0 {
        return Vec::new();
    }
    let (ax, ay) = (i128::from(p0.x.raw()), i128::from(p0.y.raw()));
    let (dx, dy) = (i128::from(p1.x.raw()) - ax, i128::from(p1.y.raw()) - ay);
    let l2 = dx * dx + dy * dy;
    let len = isqrt(l2);
    let mut hits: Vec<(i128, u32, AgentId)> = Vec::new();
    for id in store.iter_alive() {
        let p = store.pos(id).unwrap();
        let (wx, wy) = (i128::from(p.x.raw()) - ax, i128::from(p.y.raw()) - ay);
        let dot = wx * dx + wy * dy;
        let (s, hit) = if dot <= 0 {
            (0, wx * wx + wy * wy <= h * h)
        } else if dot >= l2 {
            let (ex, ey) = (wx - dx, wy - dy);
            (len, ex * ex + ey * ey <= h * h)
        } else {
            let cross = wx * dy - wy * dx;
            ((dot / len).min(len), cross * cross <= h * h * l2)
        };
        if hit {
            hits.push((s, id.index, id));
        }
    }
    hits.sort_by_key(|k| (k.0, k.1));
    hits.into_iter().map(|k| k.2).collect()
}

const CELL: i64 = 1 << 26;
const ORIGIN: i64 = -3 << 26;

fn small_grid() -> SpatialGrid {
    SpatialGrid::new(Fx::from_raw(CELL), 8, 6, v(ORIGIN, ORIGIN)).unwrap()
}

/// Coordinates on cell borders (+-1), anywhere in the +-2^30 domain (mostly outside the grid), or tiny.
fn coord() -> impl Strategy<Value = i64> {
    prop_oneof![
        ((-4i64..=12), (-1i64..=1)).prop_map(|(k, d)| ORIGIN + k * CELL + d),
        -(1i64 << 30)..(1i64 << 30),
        -3i64..=3,
    ]
}

fn pos() -> impl Strategy<Value = Vec2Fx> {
    (coord(), coord()).prop_map(|(x, y)| v(x, y))
}

fn fill(store: &mut AgentStore, positions: &[Vec2Fx]) -> Vec<AgentId> {
    positions
        .iter()
        .map(|&p| {
            store.spawn(AgentInit {
                pos: p,
                ..AgentInit::default()
            })
        })
        .collect()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]

    #[test]
    fn t5_07_segment_matches_brute_force(
        positions in prop::collection::vec(pos(), 0..120),
        despawn in prop::collection::vec(any::<bool>(), 0..120),
        segs in prop::collection::vec((pos(), pos(), any::<bool>(), 0i64..(1 << 27)), 1..8),
    ) {
        let mut store = AgentStore::new();
        let ids = fill(&mut store, &positions);
        for (id, kill) in ids.iter().zip(despawn.iter()) {
            if *kill {
                store.despawn(*id);
            }
        }
        let mut grid = small_grid();
        grid.rebuild(&store);
        let mut out = Vec::new();
        for (p0, p1, degenerate, hw) in segs {
            let p1 = if degenerate { p0 } else { p1 };
            grid.query_segment(p0, p1, Fx::from_raw(hw), &mut out);
            prop_assert_eq!(&out, &brute(&store, p0, p1, Fx::from_raw(hw)));
        }
    }
}

#[test]
fn zero_length_segment_equals_query_radius() {
    let mut store = AgentStore::new();
    let pts: Vec<Vec2Fx> = (0..60i64)
        .map(|i| v((i * 37 % 11 - 5) * CELL / 3, (i * 53 % 13 - 6) * CELL / 3))
        .collect();
    fill(&mut store, &pts);
    let mut grid = small_grid();
    grid.rebuild(&store);
    let (mut a, mut b) = (Vec::new(), Vec::new());
    for c in [
        v(0, 0),
        v(ORIGIN, ORIGIN),
        v(CELL, -CELL),
        v(9 * CELL, 9 * CELL),
    ] {
        grid.query_segment(c, c, Fx::from_raw(CELL), &mut a);
        grid.query_radius(c, Fx::from_raw(CELL), &mut b);
        // Both are sorted by distance (s is 0 for every hit, so the segment order is by index).
        let mut by_index = b.clone();
        by_index.sort_by_key(|id| id.index);
        assert_eq!(a, by_index);
    }
}

#[test]
fn segment_entirely_outside_the_grid() {
    let mut store = AgentStore::new();
    // The grid spans x in [-3, 5) cells; everything below lies outside it.
    let far = ORIGIN - 10 * CELL;
    let ids = fill(
        &mut store,
        &[
            v(far, 0),
            v(far + 5, 100),
            v(far + 5, 101),
            v(far - 100, 0),
            v(0, 0),
            v(far, 5 * CELL),
        ],
    );
    let mut grid = small_grid();
    grid.rebuild(&store);
    let mut out = Vec::new();
    let (p0, p1) = (v(far - 50, -3), v(far + 50, 3));
    grid.query_segment(p0, p1, Fx::from_raw(100), &mut out);
    assert_eq!(out, brute(&store, p0, p1, Fx::from_raw(100)));
    assert_eq!(out, vec![ids[3], ids[0], ids[1]]);
    // A segment far from every agent finds none.
    grid.query_segment(
        v(far, -9 * CELL),
        v(far + CELL, -9 * CELL),
        Fx::from_raw(100),
        &mut out,
    );
    assert!(out.is_empty());
}

#[test]
fn negative_half_width_is_empty() {
    let mut store = AgentStore::new();
    fill(&mut store, &[v(0, 0)]);
    let mut grid = small_grid();
    grid.rebuild(&store);
    let mut out = vec![store.ids_alive()[0]];
    grid.query_segment(v(0, 0), v(10, 10), Fx::from_raw(-1), &mut out);
    assert!(out.is_empty());
    grid.query_segment(v(0, 0), v(10, 10), Fx::ZERO, &mut out);
    assert_eq!(out.len(), 1);
}

#[test]
fn saturating_coordinates() {
    let (lo, hi) = (i64::MIN, i64::MAX);
    let mut store = AgentStore::new();
    // Horizontal segment across the whole i64 range, half-width 100.
    let ids = fill(
        &mut store,
        &[
            v(hi, 0),
            v(0, 5),
            v(lo, 0),
            v(lo, -100),
            v(lo, -101),
            v(hi, 101),
            v(0, 101),
            v(hi, hi),
        ],
    );
    let mut grid = small_grid();
    grid.rebuild(&store);
    let mut out = Vec::new();
    grid.query_segment(v(lo, 0), v(hi, 0), Fx::from_raw(100), &mut out);
    // s: lo -> 0 (ties by index: slot 2 before slot 3), 0 -> 2^63, hi -> 2^64 - 1.
    assert_eq!(out, vec![ids[2], ids[3], ids[1], ids[0]]);
    // The reverse direction orders the other way round.
    grid.query_segment(v(hi, 0), v(lo, 0), Fx::from_raw(100), &mut out);
    assert_eq!(out, vec![ids[0], ids[1], ids[2], ids[3]]);

    // Full diagonal: perpendicular distance of (0, y) is about y / sqrt(2).
    let mut store = AgentStore::new();
    let ids = fill(
        &mut store,
        &[
            v(hi, hi),
            v(0, 1000),
            v(0, 100),
            v(0, 0),
            v(lo, lo),
            v(lo, hi),
        ],
    );
    let mut grid = small_grid();
    grid.rebuild(&store);
    grid.query_segment(v(lo, lo), v(hi, hi), Fx::from_raw(100), &mut out);
    assert_eq!(out, vec![ids[4], ids[3], ids[2], ids[0]]);
    // The largest half-width and a reversed saturating segment do not overflow.
    grid.query_segment(v(hi, hi), v(lo, lo), Fx::MAX, &mut out);
    assert_eq!(out.len(), 5);
}

#[test]
fn long_diagonal_touches_few_cells() {
    let u = 1i64 << 32;
    let mut grid = SpatialGrid::new(Fx::from_raw(u), 100, 100, v(0, 0)).unwrap();
    grid.rebuild(&AgentStore::new());
    let mut out = Vec::new();
    grid.reset_cells_touched();
    grid.query_segment(
        v(0, 0),
        v(99 * u, 99 * u),
        Fx::from_raw(2 * u / 5),
        &mut out,
    );
    let diag = grid.cells_touched();
    assert!(diag < 500, "diagonal touched {diag} of 10000 cells");
    grid.reset_cells_touched();
    grid.query_segment(v(10 * u, 10 * u), v(10 * u, 10 * u), Fx::ZERO, &mut out);
    assert_eq!(grid.cells_touched(), 1);
}

#[test]
#[ignore = "micro benchmark; run with --ignored --nocapture"]
fn bench_query_segment_15m() {
    let mut store = AgentStore::new();
    for i in 0..20_000i64 {
        store.spawn(AgentInit {
            pos: v(((i * 7919) % 1000) << 32, ((i * 104_729) % 1000) << 32),
            ..AgentInit::default()
        });
    }
    let mut grid = SpatialGrid::new(Fx::from_raw(16 << 32), 64, 64, v(0, 0)).unwrap();
    grid.rebuild(&store);
    let mut out = Vec::new();
    let mut total = 0usize;
    let t = std::time::Instant::now();
    for i in 0..1_000i64 {
        let p0 = v((i * 977 % 1000) << 32, (i * 31 % 1000) << 32);
        let p1 = v(p0.x.raw() + (9 << 32), p0.y.raw() + (12 << 32));
        grid.query_segment(p0, p1, Fx::from_raw(700 << 22), &mut out);
        total += out.len();
    }
    println!(
        "1000 segments of 15 m, 20000 agents: {:?} total, {total} hits",
        t.elapsed()
    );
}
