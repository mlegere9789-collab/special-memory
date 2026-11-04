//! Terrain grid unit tests: nearest-edge clamp and blocking terrain (SPEC-004 section 5).

use battle_rules::{fx_ratio, DataError, TerrainGrid, TerrainTable};
use core_math::{Fx, Vec2Fx};

const TERRAIN: &str = include_str!("../../../data/rules/terrain.toml");

const LEGEND: [(char, u8); 4] = [('.', 0), ('r', 1), ('w', 3), ('~', 7)];

fn table() -> TerrainTable {
    TerrainTable::from_toml_str(TERRAIN).unwrap()
}

fn p(x: i32, y: i32) -> Vec2Fx {
    Vec2Fx::new(Fx::from_int(x), Fx::from_int(y))
}

/// A position in hundredths of a metre: `pc(-1, -1)` is (-0.01, -0.01) m.
fn pc(x: i64, y: i64) -> Vec2Fx {
    Vec2Fx::new(fx_ratio(x, 100), fx_ratio(y, 100))
}

/// A position in quarter metres (exact in fixed point): `q(63, 16)` is (15.75, 4.0) m.
fn q(x: i64, y: i64) -> Vec2Fx {
    Vec2Fx::new(fx_ratio(x, 4), fx_ratio(y, 4))
}

/// 4 columns by 3 rows of 8 m cells; row 0 is the southern row.
fn grid() -> TerrainGrid {
    TerrainGrid::from_rows(8000, 0, 0, &LEGEND, &["..w~", ".w..", "~..r"], &table()).unwrap()
}

#[test]
fn class_at_reads_cells_with_row_zero_south() {
    let g = grid();
    assert_eq!((g.width(), g.height()), (4, 3));
    assert_eq!(g.class_at(p(4, 4)), 0);
    assert_eq!(g.class_at(p(20, 4)), 3);
    assert_eq!(g.class_at(p(28, 4)), 7);
    assert_eq!(g.class_at(p(12, 12)), 3);
    assert_eq!(g.class_at(p(4, 20)), 7);
    assert_eq!(g.class_at(p(28, 20)), 1);
    // Cell borders belong to the cell on their positive side.
    assert_eq!(g.class_at(p(16, 0)), 3);
    assert_eq!(g.class_at(p(15, 0)), 0);
}

#[test]
fn positions_outside_the_grid_take_the_nearest_edge_cell() {
    let g = grid();
    // South-west corner and just below zero (floor, not truncation toward zero).
    assert_eq!(g.class_at(p(-100, -100)), 0);
    assert_eq!(g.class_at(pc(-1, -1)), 0);
    // East of the grid: column 3. North of the grid: row 2.
    assert_eq!(g.class_at(p(1000, 4)), 7);
    assert_eq!(g.class_at(p(1000, 12)), 0);
    assert_eq!(g.class_at(p(4, 1000)), 7);
    assert_eq!(g.class_at(p(1000, 1000)), 1);
    // West edge at a middle row, south edge at a middle column.
    assert_eq!(g.class_at(p(-50, 12)), 0);
    assert_eq!(g.class_at(p(12, -50)), 0);
    assert_eq!(g.class_at(p(20, -50)), 3);
    // Extreme positions do not overflow.
    assert_eq!(g.class_at(Vec2Fx::new(Fx::MIN, Fx::MIN)), 0);
    assert_eq!(g.class_at(Vec2Fx::new(Fx::MAX, Fx::MAX)), 1);
}

#[test]
fn origin_and_cell_size_are_honoured() {
    let g =
        TerrainGrid::from_rows(4000, 100_000, -50_000, &LEGEND, &["w~", ".r"], &table()).unwrap();
    // Origin (100, -50) m, 4 m cells.
    assert_eq!(g.class_at(p(101, -49)), 3);
    assert_eq!(g.class_at(p(105, -49)), 7);
    assert_eq!(g.class_at(p(101, -45)), 0);
    assert_eq!(g.class_at(p(105, -45)), 1);
    // North-west of the origin side: column 0 (clamped), row 1 (clamped).
    assert_eq!(g.class_at(p(0, 0)), 0);
    assert_eq!(g.class_at(p(-500, -500)), 3);
}

#[test]
fn from_rows_rejects_bad_input() {
    let t = table();
    let rows: [&str; 0] = [];
    assert!(matches!(
        TerrainGrid::from_rows(8000, 0, 0, &LEGEND, &rows, &t),
        Err(DataError::Parse(_))
    ));
    assert!(matches!(
        TerrainGrid::from_rows(8000, 0, 0, &LEGEND, &["..", "."], &t),
        Err(DataError::Parse(_))
    ));
    assert!(matches!(
        TerrainGrid::from_rows(8000, 0, 0, &LEGEND, &["..", ".?"], &t),
        Err(DataError::Parse(_))
    ));
    assert!(matches!(
        TerrainGrid::from_rows(0, 0, 0, &LEGEND, &["."], &t),
        Err(DataError::Range(_))
    ));
    // A legend entry naming a class the table does not hold.
    assert!(matches!(
        TerrainGrid::from_rows(8000, 0, 0, &[('x', 42)], &["x"], &t),
        Err(DataError::Range(_))
    ));
    // Duplicate legend character.
    assert!(matches!(
        TerrainGrid::from_rows(8000, 0, 0, &[('.', 0), ('.', 1)], &["."], &t),
        Err(DataError::Parse(_))
    ));
}

/// Water at column 2 of the southern row: x in 16..24, y in 0..8.
fn water() -> TerrainGrid {
    TerrainGrid::from_rows(8000, 0, 0, &LEGEND, &["..~.", "...."], &table()).unwrap()
}

#[test]
fn free_movement_is_applied_in_full() {
    let g = water();
    assert_eq!(g.apply_move(q(20, 16), q(1, 2)), q(21, 18));
    assert_eq!(g.apply_move(q(20, 16), Vec2Fx::ZERO), q(20, 16));
}

#[test]
fn blocked_axis_is_dropped_and_the_other_axis_slides() {
    let g = water();
    // x step would enter the water (x = 16.25); the y step stays free.
    assert_eq!(g.apply_move(q(63, 16), q(2, 1)), q(63, 17));
    // Only an x step: nothing moves.
    assert_eq!(g.apply_move(q(63, 16), q(2, 0)), q(63, 16));
    // From the north the y step (to 7.75) is blocked and the x step slides.
    assert_eq!(g.apply_move(q(80, 33), q(1, -2)), q(81, 33));
    // From the west with the y step crossing a free border while x is blocked.
    assert_eq!(g.apply_move(q(63, 31), q(2, 2)), q(63, 33));
}

#[test]
fn a_soldier_inside_blocking_terrain_may_leave_but_not_roam() {
    let g = water();
    // Inside the water cell: steps that stay inside are refused on both axes.
    assert_eq!(g.apply_move(q(80, 16), q(1, 0)), q(80, 16));
    assert_eq!(g.apply_move(q(80, 16), q(1, 1)), q(80, 16));
    // A step that lands on a free cell is applied.
    assert_eq!(g.apply_move(q(80, 31), q(0, 2)), q(80, 33));
}

#[test]
fn blocking_applies_to_clamped_outside_positions() {
    let g = TerrainGrid::from_rows(8000, 0, 0, &LEGEND, &["~"], &table()).unwrap();
    assert!(g.blocks(p(100, 100)));
    assert_eq!(g.apply_move(p(100, 100), p(1, 0)), p(100, 100));
    let open = TerrainGrid::from_rows(8000, 0, 0, &LEGEND, &["."], &table()).unwrap();
    assert!(!open.blocks(p(100, 100)));
    assert_eq!(open.apply_move(p(100, 100), p(1, 0)), p(101, 100));
}
