//! T14-13 (obstacle half): `obstacles_in` and `los` answer identically for any residency that
//! covers the tiles they need, and name the lowest missing tile otherwise.
mod common;

use common::lospack::{los_tiles, los_world, los_world_with, pack_obstacles};
use common::{pos, Sm, EXT_X_MM, EXT_Y_MM};
use terrain::{LosMode, ObstacleSeg, TerrainError, TerrainWorld, Tile, TileId};

fn rect_query(
    w: &TerrainWorld,
    x0: i64,
    y0: i64,
    x1: i64,
    y1: i64,
) -> Result<Vec<ObstacleSeg>, TerrainError> {
    let mut out = vec![pack_obstacles()[0]]; // stale content must be replaced
    w.obstacles_in(pos(x0, y0), pos(x1, y1), &mut out)?;
    Ok(out)
}

fn overlaps(s: &ObstacleSeg, x0: i64, y0: i64, x1: i64, y1: i64) -> bool {
    let r = |a: i32, b: i32| (i64::from(a.min(b)), i64::from(a.max(b)));
    let ((sx0, sx1), (sy0, sy1)) = (r(s.a[0], s.b[0]), r(s.a[1], s.b[1]));
    sx1 >= x0 && sx0 <= x1 && sy1 >= y0 && sy0 <= y1
}

#[test]
fn obstacles_in_is_ascending_unique_and_matches_a_brute_force_scan() {
    let w = los_world();
    let all = pack_obstacles();
    let mut r = Sm(1301);
    let mut seen_multi_tile = false;
    for _ in 0..400 {
        let (x0, y0) = (
            r.below(EXT_X_MM as u64) as i64,
            r.below(EXT_Y_MM as u64) as i64,
        );
        let (x1, y1) = (x0 + r.below(700_000) as i64, y0 + r.below(500_000) as i64);
        let got = rect_query(&w, x0, y0, x1, y1).unwrap();
        assert!(
            got.windows(2).all(|p| p[0].id < p[1].id),
            "ascending, no duplicates"
        );
        let want: Vec<ObstacleSeg> = all
            .iter()
            .filter(|s| overlaps(s, x0, y0, x1, y1))
            .copied()
            .collect();
        assert_eq!(got, want, "rect {x0},{y0},{x1},{y1}");
        seen_multi_tile |= w.tiles_for_rect(pos(x0, y0), pos(x1, y1)).len() > 1 && !got.is_empty();
    }
    assert!(seen_multi_tile);
}

#[test]
fn a_segment_stored_in_several_tiles_is_returned_once() {
    let w = los_world();
    let everything = rect_query(&w, 0, 0, EXT_X_MM, EXT_Y_MM).unwrap();
    assert_eq!(everything, pack_obstacles());
    let stored: usize = los_tiles().iter().map(|t| t.obstacles().len()).sum();
    assert!(stored > everything.len(), "some segments span tiles");
}

#[test]
fn an_inverted_rectangle_finds_nothing() {
    let w = los_world();
    assert!(rect_query(&w, 900_000, 900_000, 100_000, 100_000)
        .unwrap()
        .is_empty());
}

#[test]
fn missing_tiles_report_the_lowest_missing_id() {
    let ids: Vec<TileId> = los_tiles().iter().map(Tile::id).collect();
    let w = los_world_with(&[ids[0], ids[2], ids[4]]);
    // Rectangle over tiles (0,0) (1,0) (2,0): (1,0) is the only gap.
    let r = rect_query(&w, 1000, 1000, 1_400_000, 2000);
    assert_eq!(r, Err(TerrainError::TileMissing(TileId { x: 1, y: 0 })));
    // Over all six: the lowest missing in (y, x) order is (1,0), not (1,1).
    let r = rect_query(&w, 0, 0, EXT_X_MM, EXT_Y_MM);
    assert_eq!(r, Err(TerrainError::TileMissing(TileId { x: 1, y: 0 })));
}

#[test]
fn answers_do_not_depend_on_which_extra_tiles_are_resident() {
    let full = los_world();
    let tiles = los_tiles();
    let mut r = Sm(1302);
    for round in 0..300 {
        let (x0, y0) = (
            r.below(EXT_X_MM as u64) as i64,
            r.below(EXT_Y_MM as u64) as i64,
        );
        let (x1, y1) = (x0 + r.below(300_000) as i64, y0 + r.below(300_000) as i64);
        let need = full.tiles_for_rect(pos(x0, y0), pos(x1, y1));
        // Residency is the needed tiles plus a seeded subset of the rest, in seeded order.
        let mut keep: Vec<TileId> = tiles
            .iter()
            .map(Tile::id)
            .filter(|t| need.contains(t) || r.below(2) == 0)
            .collect();
        let n = keep.len();
        for i in (1..n).rev() {
            keep.swap(i, r.below(i as u64 + 1) as usize);
        }
        let mut w = TerrainWorld::new(full.index().clone());
        // Insert in the shuffled order, then evict and re-insert one to vary the history.
        for id in &keep {
            let t = tiles.iter().find(|t| t.id() == *id).unwrap();
            w.insert_tile(&t.encode()).unwrap();
        }
        if let Some(id) = keep.first() {
            let t = tiles.iter().find(|t| t.id() == *id).unwrap();
            assert!(w.evict_tile(*id));
            w.insert_tile(&t.encode()).unwrap();
        }
        assert_eq!(
            rect_query(&w, x0, y0, x1, y1),
            rect_query(&full, x0, y0, x1, y1),
            "round {round}"
        );
        let (a, b) = (pos(x0, y0), pos(x1, y1));
        for mode in [LosMode::Fire, LosMode::Sight] {
            assert_eq!(
                w.los(a, 1700, b, 450, mode),
                full.los(a, 1700, b, 450, mode),
                "round {round}"
            );
        }
    }
}
