//! T14-10 (synthetic part): `slope_pm`. The slab part runs in a later ticket.
mod common;

use common::pos;
use terrain::{TerrainWorld, Tile, TileId};

/// One-tile world whose vertex heights are `f(vx, vy)` centimetres.
fn world(f: impl Fn(i64, i64) -> u16) -> TerrainWorld {
    let mut t = Tile::new(TileId { x: 0, y: 0 });
    for vy in 0..129i64 {
        for vx in 0..129i64 {
            t.heights_mut()[(vy * 129 + vx) as usize] = f(vx, vy);
        }
    }
    let toml = common::index_toml(&[t.clone()], 1, 1);
    let mut w = TerrainWorld::new(terrain::TerrainIndex::from_toml_str(&toml).unwrap());
    w.insert_tile(&t.encode()).unwrap();
    w
}

#[test]
fn a_one_metre_rise_over_four_metres_is_250_permille() {
    // 100 cm per cell along x, in every cell.
    let w = world(|vx, _| (vx * 100) as u16);
    for (x, y) in [
        (0, 0),
        (2000, 2000),
        (3999, 3999),
        (40_000, 8_000),
        (500_000, 500_000),
    ] {
        assert_eq!(w.slope_pm(pos(x, y)).unwrap(), 250, "{x},{y}");
    }
    // The same along y, and falling instead of rising.
    let w = world(|_, vy| (vy * 100) as u16);
    assert_eq!(w.slope_pm(pos(12_345, 67_890)).unwrap(), 250);
    let w = world(|vx, _| (12_800 - vx * 100) as u16);
    assert_eq!(w.slope_pm(pos(12_345, 67_890)).unwrap(), 250);
}

#[test]
fn flat_ground_and_combined_gradients() {
    let w = world(|_, _| 777);
    assert_eq!(w.slope_pm(pos(1, 1)).unwrap(), 0);
    // gx = gy = 100 cm: isqrt(20000) = 141, 141 * 10 * 1000 / 4000 = 352.
    let w = world(|vx, vy| ((vx + vy) * 100) as u16);
    assert_eq!(w.slope_pm(pos(50_000, 50_000)).unwrap(), 352);
    // Truncating halves: corner heights 0, 0, 0, 3 give gx = (3 - 0) / 2 = 1, gy = 1, so
    // isqrt(2) = 1 and 1 * 10 * 1000 / 4000 = 2.
    let w = world(|vx, vy| if vx == 1 && vy == 1 { 3 } else { 0 });
    assert_eq!(w.slope_pm(pos(1000, 1000)).unwrap(), 2);
}
