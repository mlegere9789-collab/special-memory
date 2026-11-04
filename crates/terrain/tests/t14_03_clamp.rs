//! T14-03 (clamp part): positions outside the extent take the nearest edge cell, for every
//! per-cell query. The `export-rows` and `terrain.toml` halves belong to other tickets.
mod common;

use common::*;

#[test]
fn class_occ_flags_clamp_to_the_nearest_edge_cell() {
    let w = synth_world();
    let (ex, ey) = (EXT_X_MM, EXT_Y_MM);
    let far = 9_000_000i64;
    let cases = [
        // (outside point, nearest inside point, a point that is inside the same edge cell)
        ((-1, 100_000), (0, 100_000)),
        ((-far, 100_000), (0, 100_000)),
        ((ex, 100_000), (ex - 1, 100_000)),
        ((ex + far, 100_000), (ex - 1, 100_000)),
        ((100_000, -1), (100_000, 0)),
        ((100_000, -far), (100_000, 0)),
        ((100_000, ey), (100_000, ey - 1)),
        ((100_000, ey + far), (100_000, ey - 1)),
        ((-far, -far), (0, 0)),
        ((ex + far, -far), (ex - 1, 0)),
        ((-far, ey + far), (0, ey - 1)),
        ((ex + far, ey + far), (ex - 1, ey - 1)),
    ];
    for ((ox, oy), (ix, iy)) in cases {
        let (o, i) = (pos(ox, oy), pos(ix, iy));
        assert_eq!(
            w.class_at(o).unwrap(),
            w.class_at(i).unwrap(),
            "class {ox},{oy}"
        );
        assert_eq!(w.occ_mm(o).unwrap(), w.occ_mm(i).unwrap(), "occ {ox},{oy}");
        assert_eq!(
            w.flags_at(o).unwrap(),
            w.flags_at(i).unwrap(),
            "flags {ox},{oy}"
        );
        assert_eq!(
            w.slope_pm(o).unwrap(),
            w.slope_pm(i).unwrap(),
            "slope {ox},{oy}"
        );
    }
}

#[test]
fn class_occ_flags_match_the_cell_functions_inside() {
    let w = synth_world();
    let mut s = Sm(3);
    for _ in 0..5000 {
        let (x, y) = (
            s.below(EXT_X_MM as u64) as i64,
            s.below(EXT_Y_MM as u64) as i64,
        );
        let (cx, cy) = (x / 4000, y / 4000);
        let p = pos(x, y);
        assert_eq!(w.class_at(p).unwrap(), cell_class(cx, cy));
        assert_eq!(w.occ_mm(p).unwrap(), 100 * i64::from(cell_occ(cx, cy)));
        assert_eq!(w.flags_at(p).unwrap(), cell_flags(cx, cy));
    }
}

#[test]
fn height_clamps_to_the_extent_corners_and_edges() {
    let w = synth_world();
    let (ex, ey) = (EXT_X_MM, EXT_Y_MM);
    let far = 7_000_000i64;
    for (ox, oy, ix, iy) in [
        (-far, -far, 0, 0),
        (ex + far, ey + far, ex, ey),
        (-1, 123_456, 0, 123_456),
        (ex + 1, 123_456, ex, 123_456),
        (123_456, -far, 123_456, 0),
        (123_456, ey + 1, 123_456, ey),
    ] {
        assert_eq!(
            w.height_mm(pos(ox, oy)).unwrap(),
            w.height_mm(pos(ix, iy)).unwrap()
        );
    }
    assert_eq!(
        w.height_mm(pos(ex, ey)).unwrap(),
        10 * i64::from(vertex_cm(CELLS_X, CELLS_Y))
    );
}
