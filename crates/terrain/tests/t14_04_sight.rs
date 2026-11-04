//! T14-04: analytic line-of-sight cases on one-tile worlds.
mod common;

use common::lospack::{FIRE, SIGHT};
use common::pos;
use terrain::{Blocker, LosMode, ObstacleSeg, TerrainIndex, TerrainWorld, Tile, TileId};

const Y: i64 = 200_000;

fn world_from(t: Tile) -> TerrainWorld {
    let toml = common::index_toml(std::slice::from_ref(&t), 1, 1);
    let mut w = TerrainWorld::new(TerrainIndex::from_toml_str(&toml).unwrap());
    w.insert_tile(&t.encode()).unwrap();
    w
}

fn flat(cm: u16) -> Tile {
    let mut t = Tile::new(TileId { x: 0, y: 0 });
    t.heights_mut().iter_mut().for_each(|h| *h = cm);
    t
}

fn wall(id: u32, flags: u8, height_mm: u16, a: [i32; 2], b: [i32; 2]) -> ObstacleSeg {
    ObstacleSeg {
        id,
        kind: 1,
        flags,
        height_mm,
        a,
        b,
    }
}

fn clear(w: &TerrainWorld, x0: i64, z0: i64, x1: i64, z1: i64, mode: LosMode) -> bool {
    w.los(pos(x0, Y), z0, pos(x1, Y), z1, mode).unwrap().clear
}

#[test]
fn flat_ground_is_clear_in_both_modes() {
    for cm in [0, 500, 4000] {
        let w = world_from(flat(cm));
        for mode in [LosMode::Fire, LosMode::Sight] {
            let r = w.los(pos(10_000, 20_000), 1700, pos(400_000, 300_000), 450, mode);
            let r = r.unwrap();
            assert!(r.clear);
            assert_eq!((r.blocked_at_mm, r.blocker), (0, Blocker::None));
            // Eyes at ground level on flat ground are not blocked either.
            assert!(clear(&w, 50_000, 0, 150_000, 0, mode));
        }
    }
}

#[test]
fn a_ridge_blocks_below_its_height_and_passes_above() {
    // Vertex column 50 (x = 200,000 mm) is 1000 mm high; everything else is flat.
    let mut t = flat(0);
    for vy in 0..129 {
        t.heights_mut()[vy * 129 + 50] = 100;
    }
    let w = world_from(t);
    for mode in [LosMode::Fire, LosMode::Sight] {
        let r = w
            .los(pos(100_000, Y), 999, pos(300_000, Y), 999, mode)
            .unwrap();
        assert!(!r.clear);
        assert_eq!(r.blocker, Blocker::Ground);
        assert_eq!(r.blocked_at_mm, 100_000, "sample k=50 of n=100");
        assert!(clear(&w, 100_000, 1001, 300_000, 1001, mode));
        assert!(
            clear(&w, 100_000, 1000, 300_000, 1000, mode),
            "equal height passes"
        );
    }
}

#[test]
fn a_wall_blocks_iff_the_ray_is_at_or_below_its_top() {
    // A 1000 mm wall across a line falling from 1700 mm to 450 mm over 100 m.
    for (x_wall, blocks) in [
        (106_000i64, true), // f = 0.56: ray 1000 exactly
        (107_000, true),    // f = 0.57: ray 987
        (105_000, false),   // f = 0.55: ray 1012
        (100_000, false),   // f = 0.50: ray 1075
        (150_000, true),    // the target end
    ] {
        let seg = wall(
            7,
            FIRE | SIGHT,
            1000,
            [x_wall as i32, 195_000],
            [x_wall as i32, 205_000],
        );
        let mut t = flat(0);
        t.set_obstacles(vec![seg]).unwrap();
        let w = world_from(t);
        for mode in [LosMode::Fire, LosMode::Sight] {
            let r = w
                .los(pos(50_000, Y), 1700, pos(150_000, Y), 450, mode)
                .unwrap();
            assert_eq!(!r.clear, blocks, "wall at {x_wall}");
            if blocks {
                assert_eq!(r.blocker, Blocker::Obstacle(7));
                assert_eq!(r.blocked_at_mm, x_wall - 50_000);
            }
        }
    }
}

#[test]
fn walls_stand_on_the_ground_and_respect_the_mode() {
    let fire_only = wall(1, FIRE, 1000, [100_000, 195_000], [100_000, 205_000]);
    let sight_only = wall(2, SIGHT, 1000, [120_000, 195_000], [120_000, 205_000]);
    for ground_cm in [0u16, 3000] {
        let mut t = flat(ground_cm);
        t.set_obstacles(vec![fire_only, sight_only]).unwrap();
        let w = world_from(t);
        let ray = |mode| {
            w.los(pos(50_000, Y), 900, pos(110_000, Y), 900, mode)
                .unwrap()
        };
        assert_eq!(ray(LosMode::Fire).blocker, Blocker::Obstacle(1));
        assert!(
            ray(LosMode::Sight).clear,
            "the sight-only wall is past the target"
        );
        let ray = |mode| {
            w.los(pos(110_000, Y), 900, pos(150_000, Y), 900, mode)
                .unwrap()
        };
        assert!(ray(LosMode::Fire).clear);
        assert_eq!(ray(LosMode::Sight).blocker, Blocker::Obstacle(2));
        // Above the top (ground-relative) both pass.
        let r = w.los(pos(50_000, Y), 1001, pos(150_000, Y), 1001, LosMode::Fire);
        assert!(r.unwrap().clear);
    }
}

#[test]
fn the_nearest_blocker_wins_and_an_obstacle_wins_a_tie() {
    let mut t = flat(0);
    // Ground sample k = 25 of n = 50 lies at x = 100,000 (a 600 mm bump on vertex column 25).
    for vy in 0..129 {
        t.heights_mut()[vy * 129 + 25] = 60;
    }
    t.set_obstacles(vec![
        wall(3, FIRE, 2000, [100_000, 190_000], [100_000, 210_000]),
        wall(4, FIRE, 2000, [100_000, 195_000], [100_000, 205_000]),
        wall(9, FIRE, 2000, [140_000, 195_000], [140_000, 205_000]),
    ])
    .unwrap();
    let w = world_from(t);
    let r = w
        .los(pos(50_000, Y), 500, pos(150_000, Y), 500, LosMode::Fire)
        .unwrap();
    assert_eq!(
        r.blocker,
        Blocker::Obstacle(3),
        "tie on t goes to the lower id"
    );
    assert_eq!(r.blocked_at_mm, 50_000);
}

#[test]
fn a_wall_along_the_ray_blocks_from_its_first_point() {
    let mut t = flat(0);
    t.set_obstacles(vec![wall(
        1,
        FIRE,
        1000,
        [120_000, 200_000],
        [140_000, 200_000],
    )])
    .unwrap();
    let w = world_from(t);
    // Meets at x = 120,000 (t = 0.2): the ray there is 1700 - 1250 * 0.2 = 1450, above the wall.
    let r = w.los(pos(100_000, Y), 1700, pos(200_000, Y), 450, LosMode::Fire);
    assert!(r.unwrap().clear);
    // A flat ray at 900 mm is below the top the whole way.
    let r = w.los(pos(100_000, Y), 900, pos(200_000, Y), 900, LosMode::Fire);
    let r = r.unwrap();
    assert_eq!((r.blocker, r.blocked_at_mm), (Blocker::Obstacle(1), 20_000));
    // The reverse direction meets the far end first.
    let r = w.los(pos(200_000, Y), 900, pos(100_000, Y), 900, LosMode::Fire);
    assert_eq!(r.unwrap().blocked_at_mm, 60_000);
    // A parallel wall off the line, and a wall that only touches the extension, do not count.
    let mut t = flat(0);
    t.set_obstacles(vec![
        wall(1, FIRE, 1000, [120_000, 201_000], [140_000, 201_000]),
        wall(2, FIRE, 1000, [210_000, 200_000], [230_000, 200_000]),
    ])
    .unwrap();
    let w = world_from(t);
    assert!(clear(&w, 100_000, 900, 200_000, 900, LosMode::Fire));
}

#[test]
fn cover_blocks_sight_but_not_fire_and_end_cells_are_ignored() {
    // Cells x = 20..30 (80,000 to 120,000 mm) at y = 50 carry 18 m of cover.
    let mut t = flat(0);
    for cx in 20..30 {
        t.occ_mut()[50 * 128 + cx] = 180;
    }
    let w = world_from(t);
    let (y, z) = (200_000, 1700);
    let across = |mode| w.los(pos(40_000, y), z, pos(160_000, y), z, mode).unwrap();
    assert!(across(LosMode::Fire).clear);
    let r = across(LosMode::Sight);
    assert_eq!(r.blocker, Blocker::Occluder);
    assert_eq!(
        r.blocked_at_mm, 40_000,
        "first sample inside the cover, x = 80,000"
    );
    // Both ends in cover and the line staying in those two cells: clear.
    let r = w.los(pos(81_000, y), z, pos(87_000, y), z, LosMode::Sight);
    assert!(r.unwrap().clear, "own and target cells are ignored");
    // A third cell of cover in between does block.
    let r = w.los(pos(81_000, y), z, pos(101_000, y), z, LosMode::Sight);
    assert_eq!(r.unwrap().blocker, Blocker::Occluder);
}

#[test]
fn degenerate_and_short_rays_are_clear_on_open_ground() {
    let w = world_from(flat(0));
    let p = pos(100_000, Y);
    assert!(w.los(p, 1700, p, 450, LosMode::Sight).unwrap().clear);
    assert!(
        w.los(p, 0, pos(100_001, Y), 0, LosMode::Fire)
            .unwrap()
            .clear
    );
}

#[test]
fn a_missing_tile_is_reported_with_the_lowest_missing_id() {
    use common::lospack::{los_tiles, los_world_with};
    let ids: Vec<TileId> = los_tiles().iter().map(Tile::id).collect();
    let w = los_world_with(&ids[1..2]); // only (1, 0)
                                        // From tile (0,0) to tile (2,1): the box needs all six tiles; (0,0) is the lowest missing.
    let r = w.los(
        pos(1000, 1000),
        0,
        pos(1_500_000, 1_000_000),
        0,
        LosMode::Fire,
    );
    assert_eq!(
        r,
        Err(terrain::TerrainError::TileMissing(TileId { x: 0, y: 0 }))
    );
}
