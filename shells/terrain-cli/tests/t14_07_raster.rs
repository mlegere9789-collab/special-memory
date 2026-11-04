//! T14-07: hand-computed cell sets for a square, a triangle, a ring with a hole and a 6 m road;
//! compile stage 1 heights and flags on the synthetic plane; shuffled rows give a byte-equal pack.
//!
//! Cell `(cx, cy)` has its centre at `(4000 cx + 2000, 4000 cy + 2000)` mm.
mod common;

use common::*;

fn rect(x0: usize, x1: usize, y0: usize, y1: usize) -> Vec<(usize, usize)> {
    let mut v = Vec::new();
    for y in y0..=y1 {
        for x in x0..=x1 {
            v.push((x, y));
        }
    }
    v
}

fn in_class(
    pack: &hd_terrain::compile::Pack,
    id: u8,
    within: &[(usize, usize)],
) -> Vec<(usize, usize)> {
    let mut v: Vec<_> = cells_of(pack, id)
        .into_iter()
        .filter(|c| within.contains(c))
        .collect();
    v.sort_by_key(|&(x, y)| (y, x));
    v
}

#[test]
fn square_triangle_ring_road_creek_and_building_cells() {
    let pack = build(&make_survey("ras_all", &[]));
    // Square 80..96 m: cells 20..=23 in both axes, cover overridden to 12 dm.
    let sq = rect(20, 23, 20, 23);
    assert_eq!(cells_of(&pack, 8), sq);
    assert!(sq.iter().all(|&(x, y)| occ(&pack, x, y) == 12));
    // Right triangle with 18 m legs: cells (i, j) from (40, 20) with i + j <= 3 (ten cells).
    let tri: Vec<_> = rect(40, 43, 20, 23)
        .into_iter()
        .filter(|&(x, y)| (x - 40) + (y - 20) <= 3)
        .collect();
    assert_eq!(tri.len(), 10);
    assert_eq!(in_class(&pack, 3, &rect(30, 50, 15, 30)), tri);
    assert_eq!(occ(&pack, 40, 20), 180);
    // Ring 240..280 m by 80..120 m (cells 60..=69, 20..=29) minus the hole (cells 63..=66, 23..=26).
    let hole = rect(63, 66, 23, 26);
    let ring: Vec<_> = rect(60, 69, 20, 29)
        .into_iter()
        .filter(|c| !hole.contains(c))
        .collect();
    assert_eq!(ring.len(), 84);
    assert_eq!(cells_of(&pack, 10), ring);
    assert_eq!(class(&pack, 64, 24), 0);
    // Road 6 m wide from (20, 200) m to (120, 200) m: rows 49 and 50, columns 4..=30 with round
    // end caps (the corner cells at distance sqrt(8) m are in, the next column is out); the woods
    // polygon underneath (cells 15..=19, 45..=54) loses to the higher-priority road.
    assert_eq!(cells_of(&pack, 1), rect(4, 30, 49, 50));
    assert_eq!(class(&pack, 16, 49), 1);
    assert_eq!(occ(&pack, 16, 49), 0);
    assert_eq!(class(&pack, 16, 48), 3);
    assert_eq!(occ(&pack, 16, 48), 180);
    // Creek along x = 300 m, half width 2 m: ford in columns 74 and 75, marsh one cell either side.
    for cy in [60usize, 70, 74] {
        let row: Vec<u8> = (72..78).map(|cx| class(&pack, cx, cy)).collect();
        assert_eq!(row, vec![0, 5, 6, 6, 5, 0], "row {cy}");
    }
    assert_eq!(occ(&pack, 73, 60), 10);
    // Building 320..336 m: sixteen cells of class 11 with 70 dm cover.
    assert_eq!(cells_of(&pack, 11), rect(80, 83, 80, 83));
    assert_eq!(occ(&pack, 81, 81), 70);
}

#[test]
fn vertex_heights_origin_and_no_survey_flags() {
    let pack = build(&make_survey("ras_heights", &[]));
    let t = &pack.tiles[0];
    // Interior vertex (vx, vy): mean of posts 4v-2..=4v+1 of 5i + 3j is 20 vx + 12 vy - 4 cm.
    // Corner (0, 0) sees posts 0..=1 in both axes: mean 4 cm, the lowest vertex, so z_origin = 4.
    assert!(pack.index_text.contains("z_origin_cm = 4\n"));
    assert_eq!(
        t.heights()[10 * 129 + 10],
        (20 * 10 + 12 * 10 - 4 - 4) as u16
    );
    assert_eq!(t.heights()[0], 0);
    // Edge vertices with at least 8 valid posts are surveyed; the three corners with fewer than 8
    // flag their cell NO_SURVEY (bit 0).
    let flagged: Vec<usize> = (0..128 * 128).filter(|&i| t.flags()[i] & 1 != 0).collect();
    assert_eq!(flagged, vec![0, 127, 127 * 128]);
    assert!(t.flags().iter().all(|&f| f & !1 == 0));
}

fn reverse_rows(text: &str, marker: &str) -> String {
    let mut parts = text.split(marker);
    let head = parts.next().unwrap();
    let mut rows: Vec<&str> = parts.collect();
    rows.reverse();
    rows.iter().fold(head.to_string(), |a, r| a + marker + r)
}

#[test]
fn shuffled_rows_give_a_byte_equal_pack() {
    let base = build(&make_survey("ras_order_a", &[]));
    let rev = |file: &str, marker: &str| {
        (
            file.to_string(),
            reverse_rows(&fixture_text("synthetic", file), marker),
        )
    };
    let pairs = [
        rev("landcover.toml", "[[polygon]]"),
        rev("linear.toml", "[[line]]"),
    ];
    let replace: Vec<(&str, String)> = pairs.iter().map(|(n, t)| (n.as_str(), t.clone())).collect();
    assert_ne!(replace[0].1, fixture_text("synthetic", "landcover.toml"));
    let shuffled = build(&make_survey("ras_order_b", &replace));
    assert_eq!(base.content_hash, shuffled.content_hash);
    assert_eq!(base.index_text, shuffled.index_text);
    assert_eq!(base.tiles[0].encode(), shuffled.tiles[0].encode());
}
