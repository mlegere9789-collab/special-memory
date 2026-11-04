//! T14-02: `height_mm` at vertices, at cell centres, across seams, and against the independent
//! reference `qc/fixtures/ref_height.py` (committed output `qc/fixtures/ref_height.csv`).
mod common;

use common::*;

#[test]
fn height_at_every_vertex_is_ten_times_centimetres() {
    let w = synth_world();
    for vy in 0..=CELLS_Y {
        for vx in 0..=CELLS_X {
            let h = w.height_mm(pos(vx * 4000, vy * 4000)).unwrap();
            assert_eq!(h, 10 * i64::from(vertex_cm(vx, vy)), "vertex {vx},{vy}");
        }
    }
}

#[test]
fn height_at_every_cell_centre_is_the_floored_mean() {
    let w = synth_world();
    for cy in 0..CELLS_Y {
        for cx in 0..CELLS_X {
            let s: i64 = [(0, 0), (1, 0), (0, 1), (1, 1)]
                .iter()
                .map(|(dx, dy)| i64::from(vertex_cm(cx + dx, cy + dy)))
                .sum();
            let h = w
                .height_mm(pos(cx * 4000 + 2000, cy * 4000 + 2000))
                .unwrap();
            assert_eq!(h, (10 * s).div_euclid(4), "cell {cx},{cy}");
        }
    }
}

#[test]
fn heights_agree_on_both_sides_of_a_seam() {
    let tiles = synth_tiles();
    // Stored edges are duplicated in the neighbour.
    for ty in 0..TILES_Y as usize {
        for tx in 0..TILES_X as usize - 1 {
            let (l, r) = (&tiles[ty * 3 + tx], &tiles[ty * 3 + tx + 1]);
            for vy in 0..129 {
                assert_eq!(l.heights()[vy * 129 + 128], r.heights()[vy * 129]);
            }
        }
    }
    // Queries just below, on and just above a seam stay within the local slope of one another.
    let w = synth_world();
    for seam in [512_000i64, 1_024_000] {
        for y in [
            0i64, 1, 3999, 4000, 123_457, 511_999, 512_000, 700_001, 1_023_999,
        ] {
            let a = w.height_mm(pos(seam - 1, y)).unwrap();
            let b = w.height_mm(pos(seam, y)).unwrap();
            let c = w.height_mm(pos(seam + 1, y)).unwrap();
            // One millimetre moves the height by at most 10 * 9000 / 4000 rounded up.
            assert!(
                (a - b).abs() <= 23 && (b - c).abs() <= 23,
                "x seam {seam} y {y}"
            );
        }
    }
    for seam in [512_000i64] {
        for x in [0i64, 1, 3999, 123_457, 1_535_999, 1_536_000] {
            let a = w.height_mm(pos(x, seam - 1)).unwrap();
            let b = w.height_mm(pos(x, seam)).unwrap();
            assert!((a - b).abs() <= 23, "y seam {seam} x {x}");
        }
    }
}

#[test]
fn ten_thousand_points_match_the_reference() {
    let w = synth_world();
    let csv = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../qc/fixtures/ref_height.csv"
    ))
    .unwrap();
    let mut lines = csv.lines();
    assert_eq!(lines.next(), Some("x_mm,y_mm,height_mm"));
    let mut n = 0;
    for line in lines {
        let v: Vec<i64> = line.split(',').map(|s| s.parse().unwrap()).collect();
        assert_eq!(
            w.height_mm(pos(v[0], v[1])).unwrap(),
            v[2],
            "row {n}: {line}"
        );
        n += 1;
    }
    assert_eq!(n, 10_000);
}

#[test]
fn position_conversion_floors_toward_minus_infinity() {
    let w = synth_world();
    // Raw positions between two millimetres take the lower one.
    let one_mm = (1i64 << 32) / 1000;
    let p = |raw: i64| {
        core_math::Vec2Fx::new(core_math::Fx::from_raw(raw), core_math::Fx::from_raw(raw))
    };
    let at = |mm: i64| w.height_mm(pos(mm, mm)).unwrap();
    assert_eq!(
        w.height_mm(p(one_mm * 3_000 + one_mm / 2)).unwrap(),
        at(3_000)
    );
    assert_eq!(w.height_mm(p(-1)).unwrap(), at(0));
}
