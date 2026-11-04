//! The Hot-set brute-force half of T8-15 (SPEC-008 section 7): a regiment crossing the map makes
//! the Hot set equal a brute-force overlap at every boundary, and a smoking sector stays Hot.

use battle_rules::{FidelityTable, MapSpec, SectorBody, SectorMap, SectorState, F0, F1, F2};
use core_math::{Fx, Vec2Fx};

const FIDELITY: &str = include_str!("../../../data/rules/fidelity.toml");
const SECTOR: i64 = 500_000;

fn table() -> FidelityTable {
    FidelityTable::from_toml_str(FIDELITY).unwrap()
}

fn map10() -> MapSpec {
    MapSpec {
        width_mm: 10_000_000,
        height_mm: 10_000_000,
        origin_x_mm: 0,
        origin_y_mm: 0,
    }
}

fn body(x_m: i32, y_m: i32, reach_mm: u32, class: battle_rules::FidelityClass) -> SectorBody {
    SectorBody {
        anchor: Vec2Fx::new(Fx::from_int(x_m), Fx::from_int(y_m)),
        reach_mm,
        class,
    }
}

/// States from first principles: every sector against every body, no index arithmetic shared
/// with the implementation. A box is closed, a sector is half open.
fn brute(map: &MapSpec, bodies: &[(i64, i64, i64, u8)], margin: i64) -> Vec<SectorState> {
    let sw = (map.width_mm + SECTOR - 1) / SECTOR;
    let sh = (map.height_mm + SECTOR - 1) / SECTOR;
    let overlaps = |sx: i64, sy: i64, b: &(i64, i64, i64, u8)| {
        let (x0, x1) = (sx * SECTOR, (sx + 1) * SECTOR);
        let (y0, y1) = (sy * SECTOR, (sy + 1) * SECTOR);
        b.0 - b.2 < x1 && b.0 + b.2 >= x0 && b.1 - b.2 < y1 && b.1 + b.2 >= y0
    };
    let mut out = Vec::new();
    for sy in 0..sh {
        for sx in 0..sw {
            let hot = (-margin..=margin).any(|dy| {
                (-margin..=margin).any(|dx| {
                    let (tx, ty) = (sx + dx, sy + dy);
                    (0..sw).contains(&tx)
                        && (0..sh).contains(&ty)
                        && bodies.iter().any(|b| b.3 <= 1 && overlaps(tx, ty, b))
                })
            });
            let warm = bodies.iter().any(|b| b.3 == 2 && overlaps(sx, sy, b));
            out.push(if hot {
                SectorState::Hot
            } else if warm {
                SectorState::Warm
            } else {
                SectorState::Cold
            });
        }
    }
    out
}

fn states(m: &SectorMap) -> Vec<SectorState> {
    m.states().to_vec()
}

#[test]
fn dimensions_follow_the_map() {
    let m = SectorMap::new(&map10(), &table());
    assert_eq!((m.width(), m.height()), (20, 20));
    assert_eq!(m.states().len(), 400);
    assert!(m.states().iter().all(|s| *s == SectorState::Cold));
    let odd = MapSpec {
        width_mm: 1_250_000,
        height_mm: 500_000,
        origin_x_mm: -100,
        origin_y_mm: 7,
    };
    let m = SectorMap::new(&odd, &table());
    assert_eq!((m.width(), m.height()), (3, 1));
    assert_eq!(m.state(99), SectorState::Cold);
    assert_eq!(m.hot_window(), None);
}

#[test]
fn a_regiment_crossing_the_map_makes_the_hot_set_a_brute_force_overlap() {
    let map = map10();
    let t = table();
    let margin = i64::from(t.globals.hot_margin_sectors);
    let mut m = SectorMap::new(&map, &t);
    // West to east along y = 3 km then diagonal back, reach 150 m, in 37 m steps, with a second
    // class 2 regiment drifting the other way.
    for step in 0..270i32 {
        let (x, y) = (step * 37, 3000 + (step * 37) % 4000);
        let (x2, y2) = (9990 - step * 37, 8000 - (step * 11) % 3000);
        let bodies = [body(x, y, 150_000, F0), body(x2, y2, 90_000, F2)];
        m.update(&bodies, |_| 0);
        let truth = [
            (i64::from(x) * 1000, i64::from(y) * 1000, 150_000, 0u8),
            (i64::from(x2) * 1000, i64::from(y2) * 1000, 90_000, 2u8),
        ];
        assert_eq!(states(&m), brute(&map, &truth, margin), "step {step}");
    }
}

#[test]
fn boxes_on_sector_edges_and_map_corners() {
    let map = map10();
    let t = table();
    let margin = i64::from(t.globals.hot_margin_sectors);
    let mut m = SectorMap::new(&map, &t);
    for (x, y, r) in [
        (0, 0, 0u32),
        (500, 500, 0),
        (500, 500, 1),
        (9999, 9999, 0),
        (499, 501, 1000),
    ] {
        m.update(&[body(x, y, r, F1)], |_| 0);
        let truth = [(i64::from(x) * 1000, i64::from(y) * 1000, i64::from(r), 1u8)];
        assert_eq!(states(&m), brute(&map, &truth, margin), "{x},{y},{r}");
    }
}

#[test]
fn a_body_past_the_edge_clamps_to_the_last_sector() {
    let mut m = SectorMap::new(&map10(), &table());
    m.update(&[body(10_000, 10_000, 0, F0), body(-50, 20, 0, F2)], |_| 0);
    assert_eq!(m.state(m.index(19, 19)), SectorState::Hot);
    assert_eq!(m.state(m.index(18, 18)), SectorState::Hot);
    assert_eq!(m.state(m.index(0, 0)), SectorState::Warm);
}

#[test]
fn map_origin_is_subtracted() {
    let map = MapSpec {
        width_mm: 5_000_000,
        height_mm: 5_000_000,
        origin_x_mm: 2_000_000,
        origin_y_mm: -1_000_000,
    };
    let mut m = SectorMap::new(&map, &table());
    // World (2.2 km, -0.8 km) is sector (0, 0) of this map.
    m.update(&[body(2200, -800, 0, F0)], |_| 0);
    assert_eq!(m.state(0), SectorState::Hot);
    assert_eq!(m.state(m.index(1, 1)), SectorState::Hot);
    assert_eq!(m.state(m.index(2, 2)), SectorState::Cold);
}

#[test]
fn a_smoking_sector_stays_hot_until_its_smoke_clears() {
    let map = map10();
    let mut m = SectorMap::new(&map, &table());
    m.update(&[body(1000, 1000, 0, F0)], |_| 0);
    let hot_idx = m.index(2, 2);
    assert_eq!(m.state(hot_idx), SectorState::Hot);
    // The regiment moves far away; the old centre sector has smoke and does not leave Hot.
    let far = [body(8000, 8000, 0, F0)];
    m.update(&far, |i| u64::from(i == hot_idx));
    assert_eq!(m.state(hot_idx), SectorState::Hot);
    // Its neighbours, which hold no smoke, do leave.
    assert_eq!(m.state(m.index(1, 1)), SectorState::Cold);
    // Still smoking next boundary: still Hot. Cleared: Cold.
    m.update(&far, |i| u64::from(i == hot_idx));
    assert_eq!(m.state(hot_idx), SectorState::Hot);
    m.update(&far, |_| 0);
    assert_eq!(m.state(hot_idx), SectorState::Cold);
    // A Warm sector with smoke is not Hot: the rule only keeps sectors that were Hot.
    m.update(&[body(1000, 1000, 0, F2)], |_| 1);
    assert_eq!(m.state(hot_idx), SectorState::Warm);
}

#[test]
fn smoke_is_asked_only_for_sectors_that_would_leave_hot() {
    let mut m = SectorMap::new(&map10(), &table());
    m.update(&[body(1000, 1000, 0, F0)], |_| 0);
    let asked = std::cell::RefCell::new(Vec::new());
    m.update(&[body(8000, 8000, 0, F0)], |i| {
        asked.borrow_mut().push(i);
        0
    });
    let mut a = asked.into_inner();
    a.sort_unstable();
    let want: Vec<usize> = (1..=3)
        .flat_map(|y| (1..=3).map(move |x| y * 20 + x))
        .collect();
    assert_eq!(a, want);
}

#[test]
fn hot_window_is_the_sector_aligned_box_of_hot_sectors() {
    let mut m = SectorMap::new(&map10(), &table());
    m.update(&[body(1200, 1200, 0, F0), body(4100, 2600, 0, F1)], |_| 0);
    let w = m.hot_window().unwrap();
    // Centres in sectors (2,2) and (8,5); margin 1.
    assert_eq!((w.sx0, w.sy0, w.sx1, w.sy1), (1, 1, 9, 6));
    assert_eq!(
        (w.x0_mm, w.y0_mm, w.x1_mm, w.y1_mm),
        (SECTOR, SECTOR, 10 * SECTOR, 7 * SECTOR)
    );
    // Clipped to the map at the east edge.
    m.update(&[body(9900, 9900, 0, F0)], |_| 0);
    let w = m.hot_window().unwrap();
    assert_eq!(
        (w.sx1, w.sy1, w.x1_mm, w.y1_mm),
        (19, 19, 10_000_000, 10_000_000)
    );
    // Only F2 regiments: no Hot sector, no window.
    m.update(&[body(500, 500, 0, F2)], |_| 0);
    assert_eq!(m.hot_window(), None);
}

#[test]
fn state_tags_are_stable() {
    assert_eq!(
        [
            SectorState::Cold.tag(),
            SectorState::Warm.tag(),
            SectorState::Hot.tag()
        ],
        [0, 1, 2]
    );
}
