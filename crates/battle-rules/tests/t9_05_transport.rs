//! T9-05: wind carries a puff at the rate the formula gives, in the right direction for every
//! sign, and calm air spreads it mirror-symmetrically (SPEC-009 section 4).

mod t93_support;

use battle_rules::SmokeGrid;
use t93_support::{at, grid, params, put};

/// Centroid of the smoke in metres (cell centres).
fn centroid(g: &SmokeGrid) -> (f64, f64) {
    let (mut sx, mut sy, mut s) = (0f64, 0f64, 0f64);
    for y in 0..g.height {
        for x in 0..g.width {
            let d = f64::from(at(g, x, y));
            sx += d * (8.0 * f64::from(x) + 4.0);
            sy += d * (8.0 * f64::from(y) + 4.0);
            s += d;
        }
    }
    (sx / s, sy / s)
}

fn puff(wx: i64, wy: i64, n: u32) -> ((f64, f64), (f64, f64)) {
    let p = params(&[]);
    let mut g = grid(81, 81);
    put(&mut g, 40, 40, 1_000_000);
    let start = centroid(&g);
    for _ in 0..n {
        g.update(&p, wx, wy, 0, &|_| 0);
    }
    (start, centroid(&g))
}

#[test]
fn four_metres_a_second_east_moves_the_centroid_four_times_point_two_n_metres() {
    let n = 40;
    let (s, e) = puff(4000, 0, n);
    let want = 4.0 * 0.2 * f64::from(n);
    let got = e.0 - s.0;
    assert!(
        (got - want).abs() <= 0.05 * want,
        "moved {got} m, want {want} m"
    );
    assert!((e.1 - s.1).abs() < 0.05, "no drift across the wind");
}

#[test]
fn all_four_wind_signs_move_the_puff_the_right_way() {
    for (wx, wy) in [(4000, 0), (-4000, 0), (0, 4000), (0, -4000)] {
        let (s, e) = puff(wx, wy, 20);
        let (dx, dy) = (e.0 - s.0, e.1 - s.1);
        assert_eq!(
            (dx > 1.0, dx < -1.0),
            (wx > 0, wx < 0),
            "wind ({wx},{wy}) moved ({dx},{dy})"
        );
        assert_eq!(
            (dy > 1.0, dy < -1.0),
            (wy > 0, wy < 0),
            "wind ({wx},{wy}) moved ({dx},{dy})"
        );
    }
}

#[test]
fn calm_air_spreads_mirror_symmetric_cell_for_cell() {
    let p = params(&[]);
    let mut g = grid(21, 21);
    put(&mut g, 10, 10, 900_000);
    for _ in 0..30 {
        g.update(&p, 0, 0, 0, &|_| 0);
    }
    assert!(at(&g, 11, 10) > 0);
    for y in 0..21 {
        for x in 0..21 {
            let d = at(&g, x, y);
            assert_eq!(d, at(&g, 20 - x, y), "x mirror at ({x},{y})");
            assert_eq!(d, at(&g, x, 20 - y), "y mirror at ({x},{y})");
            assert_eq!(d, at(&g, y, x), "diagonal mirror at ({x},{y})");
        }
    }
}

#[test]
fn smoke_in_woods_stays_where_it_is_longer() {
    let p = params(&[]);
    let run = |retain: u32| {
        let mut g = grid(41, 5);
        put(&mut g, 2, 2, 1_000_000);
        for _ in 0..30 {
            g.update(&p, 4000, 0, 0, &move |_| retain);
        }
        centroid(&g).0
    };
    assert!(run(400) < run(0));
}
