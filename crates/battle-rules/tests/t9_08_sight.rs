//! T9-08 and T9-09 (SPEC-009 section 5): relief grid, bilinear ground, the sight query with
//! relief, wall and smoke terms, eye and aim heights, signal quality, and the relief sidecar.

use battle_rules::{
    aim_mm, eye_mm, optical_depth_micro, sight, signal_quality_pm, to_mm, DamageTable,
    ElevationGrid, Obstacle, ObstacleLayer, Posture, SightWorld, SmokeGrid, WeatherNow,
};
use core_math::{Fx, Vec2Fx};

const DAMAGE: &str = include_str!("../../../data/rules/damage.toml");

fn m(x: i32, y: i32) -> Vec2Fx {
    Vec2Fx::new(Fx::from_int(x), Fx::from_int(y))
}

/// Margin and minimum transmission of the shipped `[sight]` table.
fn world<'a>(
    relief: &'a ElevationGrid,
    obstacles: &'a ObstacleLayer,
    smoke: &'a SmokeGrid,
) -> SightWorld<'a> {
    SightWorld {
        relief,
        obstacles,
        smoke,
        ext_k: 20,
        clear_margin_mm: 100,
        min_transmit_pm: 50,
    }
}

/// 20 columns by 3 rows of 8 m cells; columns 9 and 10 (the plateau centred on x = 80 m) are
/// `top_dm` decimetres high, the rest flat.
fn ridge(top_dm: i32) -> ElevationGrid {
    let row: Vec<i32> = (0..20)
        .map(|c| if c == 9 || c == 10 { top_dm } else { 0 })
        .collect();
    ElevationGrid::from_rows(8000, 0, 0, &[row.clone(), row.clone(), row]).unwrap()
}

fn flat_world_parts() -> (ElevationGrid, ObstacleLayer, SmokeGrid) {
    (
        ElevationGrid::flat(),
        ObstacleLayer::default(),
        SmokeGrid::empty(),
    )
}

// ---- ground ----

#[test]
fn to_mm_floors() {
    assert_eq!(to_mm(Fx::from_int(3)), 3000);
    assert_eq!(to_mm(Fx::from_int(-3)), -3000);
    assert_eq!(to_mm(Fx::from_raw(-1)), -1);
    assert_eq!(to_mm(Fx::from_raw(1)), 0);
    assert_eq!(to_mm(Fx::from_raw((1i64 << 32) / 2)), 500);
}

#[test]
fn flat_grid_and_scenario_without_relief_read_zero() {
    let g = ElevationGrid::flat();
    assert_eq!(g.ground_mm(m(5, -7)), 0);
    let z = ElevationGrid::from_rows(8000, 0, 0, &[vec![0, 0], vec![0, 0]]).unwrap();
    assert_eq!(z.ground_mm(m(8, 8)), 0);
}

#[test]
fn bilinear_between_cell_centres_and_clamped_outside() {
    // Centres at x = 4 m and 12 m hold 0 and 1000 mm; rows equal.
    let g = ElevationGrid::from_rows(8000, 0, 0, &[vec![0, 10], vec![0, 10]]).unwrap();
    assert_eq!(g.h_mm, vec![0, 1000, 0, 1000]);
    assert_eq!(g.ground_mm(m(4, 4)), 0);
    assert_eq!(g.ground_mm(m(12, 4)), 1000);
    assert_eq!(g.ground_mm(m(6, 4)), 250);
    assert_eq!(g.ground_mm(m(8, 8)), 500);
    assert_eq!(g.ground_mm(m(-100, 4)), 0);
    assert_eq!(g.ground_mm(m(100, 400)), 1000);
    // Exactness: a third of the way is floored, not rounded.
    let third = Vec2Fx::new(
        Fx::from_raw((4i64 << 32) + (8i64 << 32) / 3),
        Fx::from_int(4),
    );
    assert_eq!(g.ground_mm(third), 333);
}

#[test]
fn bilinear_in_both_axes_and_negative_heights_floor() {
    // Corners 0, 100, 200, 300 mm; the middle is their mean.
    let g = ElevationGrid::from_rows(8000, 0, 0, &[vec![0, 1], vec![2, 3]]).unwrap();
    assert_eq!(g.ground_mm(m(8, 8)), 150);
    let n = ElevationGrid::from_rows(8000, 0, 0, &[vec![-1, 0]]).unwrap();
    // -100 mm to 0 over 8 m; a point 1 mm past the first centre reads floor(-99.9875) = -100.
    let p = Vec2Fx::new(
        Fx::from_raw((4i64 << 32) + (1i64 << 32) / 1000),
        Fx::from_int(4),
    );
    assert_eq!(n.ground_mm(p), -100);
}

#[test]
fn origin_offsets_the_grid() {
    let g = ElevationGrid::from_rows(8000, 100_000, -8000, &[vec![0, 10]]).unwrap();
    assert_eq!(g.ground_mm(m(104, -4)), 0);
    assert_eq!(g.ground_mm(m(112, -4)), 1000);
}

#[test]
fn from_rows_rejects_bad_shapes() {
    assert!(ElevationGrid::from_rows::<Vec<i32>>(8000, 0, 0, &[]).is_err());
    assert!(ElevationGrid::from_rows(8000, 0, 0, &[Vec::<i32>::new()]).is_err());
    assert!(ElevationGrid::from_rows(8000, 0, 0, &[vec![0, 0], vec![0]]).is_err());
    assert!(ElevationGrid::from_rows(0, 0, 0, &[vec![0]]).is_err());
    assert!(ElevationGrid::from_rows(8000, 0, 0, &[vec![20_000]]).is_err());
}

// ---- sidecar ----

const PROV: &str =
    "confidence = \"unverified\"\nplaceholder = true\ncalibration_target = \"CT-009-3\"";

fn sidecar(extra: &str, rows: &[&str]) -> String {
    let mut t = format!(
        "schema = \"relief/1\"\ncell_mm = 8000\norigin_x_mm = 0\norigin_y_mm = 0\n{PROV}\n{extra}\n"
    );
    for r in rows {
        t.push_str(&format!("[[row]]\ndm = \"{r}\"\n"));
    }
    t
}

#[test]
fn sidecar_loads_rows_south_first() {
    let g = ElevationGrid::from_toml_str(&sidecar("", &["0 10 20", "30 -5 0"]), 8000).unwrap();
    assert_eq!((g.width, g.height, g.cell_mm), (3, 2, 8000));
    assert_eq!(g.h_mm, vec![0, 1000, 2000, 3000, -500, 0]);
    assert_eq!(
        g,
        ElevationGrid::from_rows(8000, 0, 0, &[vec![0, 10, 20], vec![30, -5, 0]]).unwrap()
    );
}

#[test]
fn sidecar_refusals() {
    let ok = ["0 1", "2 3"];
    // Cell size must equal the terrain grid's.
    assert!(ElevationGrid::from_toml_str(&sidecar("", &ok), 4000).is_err());
    // Unknown key, wrong schema, float, ragged or non-integer rows, no rows, no provenance.
    assert!(ElevationGrid::from_toml_str(&sidecar("extra = 1", &ok), 8000).is_err());
    let wrong = sidecar("", &ok).replace("relief/1", "relief/2");
    assert!(ElevationGrid::from_toml_str(&wrong, 8000).is_err());
    assert!(ElevationGrid::from_toml_str(&sidecar("scale = 1.5", &ok), 8000).is_err());
    assert!(ElevationGrid::from_toml_str(&sidecar("", &["0 1", "2"]), 8000).is_err());
    assert!(ElevationGrid::from_toml_str(&sidecar("", &["0 1.5", "2 3"]), 8000).is_err());
    assert!(ElevationGrid::from_toml_str(&sidecar("", &["0 x", "2 3"]), 8000).is_err());
    assert!(ElevationGrid::from_toml_str(&sidecar("", &[]), 8000).is_err());
    let bare = sidecar("", &ok).replace(PROV, "");
    assert!(ElevationGrid::from_toml_str(&bare, 8000).is_err());
}

// ---- heights from posture ----

#[test]
fn eye_and_aim_heights_follow_posture() {
    let d = DamageTable::from_toml_str(DAMAGE).unwrap();
    assert_eq!(eye_mm(&d, Posture::Standing), 1450);
    assert_eq!(aim_mm(&d, Posture::Standing), 1020);
    assert_eq!(aim_mm(&d, Posture::Prone), 270);
    assert!(eye_mm(&d, Posture::Prone) < eye_mm(&d, Posture::Kneeling));
}

// ---- T9-08 ----

#[test]
fn flat_ground_is_clear() {
    let (r, o, s) = flat_world_parts();
    let w = world(&r, &o, &s);
    let v = sight(&w, m(0, 0), 1450, m(100, 0), 1020, 2000);
    assert!(v.visible);
    assert_eq!(v.transmit_pm, 1000);
    assert!(v.clearance_mm >= 1020);
}

#[test]
fn ridge_blocks_two_standing_men_and_not_a_man_on_it() {
    let r = ridge(30);
    let (o, s) = (ObstacleLayer::default(), SmokeGrid::empty());
    let w = world(&r, &o, &s);
    assert_eq!(r.ground_mm(m(80, 12)), 3000);
    let across = sight(&w, m(30, 12), 1450, m(130, 12), 1450, 2000);
    assert!(!across.visible);
    assert!(across.clearance_mm < 0);
    let on_it = sight(&w, m(30, 12), 1450, m(80, 12), 1450, 2000);
    assert!(on_it.visible, "clearance {}", on_it.clearance_mm);
}

#[test]
fn equal_eyes_give_a_symmetric_query() {
    let (o, s) = (ObstacleLayer::default(), SmokeGrid::empty());
    for top in [0, 8, 30] {
        let r = ridge(top);
        let w = world(&r, &o, &s);
        let ab = sight(&w, m(30, 12), 1450, m(130, 12), 1450, 2000);
        let ba = sight(&w, m(130, 12), 1450, m(30, 12), 1450, 2000);
        assert_eq!(ab, ba, "ridge {top}");
    }
}

fn wall(height_mm: u16) -> ObstacleLayer {
    ObstacleLayer {
        segments: vec![Obstacle {
            a: m(10, -5),
            b: m(10, 5),
            height_mm,
        }],
    }
}

#[test]
fn a_wall_hides_the_prone_and_not_the_standing() {
    let d = DamageTable::from_toml_str(DAMAGE).unwrap();
    let (r, s) = (ElevationGrid::flat(), SmokeGrid::empty());
    let o = wall(1000);
    let w = world(&r, &o, &s);
    let shooter = eye_mm(&d, Posture::Standing);
    let standing = sight(
        &w,
        m(0, 0),
        shooter,
        m(15, 0),
        aim_mm(&d, Posture::Standing),
        2000,
    );
    assert!(standing.visible, "clearance {}", standing.clearance_mm);
    assert_eq!(standing.clearance_mm, 163);
    let prone = sight(
        &w,
        m(0, 0),
        shooter,
        m(15, 0),
        aim_mm(&d, Posture::Prone),
        2000,
    );
    assert!(!prone.visible);
    assert_eq!(prone.clearance_mm, -337);
}

#[test]
fn wall_on_a_ray_end_counts_and_a_wall_off_the_ray_does_not() {
    let (r, s) = (ElevationGrid::flat(), SmokeGrid::empty());
    let o = ObstacleLayer {
        segments: vec![Obstacle {
            a: m(10, 0),
            b: m(10, 5),
            height_mm: 3000,
        }],
    };
    let w = world(&r, &o, &s);
    // The ray passes through the segment's end point (endpoints are inclusive).
    assert!(!sight(&w, m(0, 0), 1450, m(20, 0), 1450, 2000).visible);
    // Parallel and beside the ray: no effect.
    assert!(sight(&w, m(0, -3), 1450, m(20, -3), 1450, 2000).visible);
}

#[test]
fn a_wall_stands_on_the_ground_where_it_is_crossed() {
    // A 1000 mm wall on an 800 mm plateau: the ray, 1450 above the flat ground at both ends,
    // clears the plateau by 650 and the wall's top (ground + 1000) not at all.
    let r = ridge(8);
    let s = SmokeGrid::empty();
    let o = ObstacleLayer {
        segments: vec![Obstacle {
            a: m(80, 0),
            b: m(80, 24),
            height_mm: 1000,
        }],
    };
    let w = world(&r, &o, &s);
    let v = sight(&w, m(50, 12), 1450, m(110, 12), 1450, 2000);
    assert_eq!(v.clearance_mm, 1450 - 800 - 1000);
    assert!(!v.visible);
    let none = ObstacleLayer::default();
    assert!(
        sight(
            &world(&r, &none, &s),
            m(50, 12),
            1450,
            m(110, 12),
            1450,
            2000
        )
        .visible
    );
}

#[test]
fn coincident_or_very_near_points_are_visible() {
    let (r, o, s) = flat_world_parts();
    let w = world(&r, &o, &s);
    assert!(sight(&w, m(5, 5), 1450, m(5, 5), 1450, 2000).visible);
    assert!(sight(&w, m(5, 5), 1450, m(6, 5), 1450, 2000).visible);
}

// ---- T9-09 ----

fn smoke_cells(d: u32) -> SmokeGrid {
    let mut g = SmokeGrid::new(8000, 20, 3, Vec2Fx::ZERO).unwrap();
    g.density.iter_mut().for_each(|c| *c = d);
    g
}

#[test]
fn uniform_smoke_over_100_m_gives_one_unit_of_optical_depth() {
    let (r, o) = (ElevationGrid::flat(), ObstacleLayer::default());
    let s = smoke_cells(500);
    let w = world(&r, &o, &s);
    let tau = optical_depth_micro(&w, m(2, 12), m(102, 12), 2000);
    assert_eq!(tau, 1_000_000);
    let v = sight(&w, m(2, 12), 1450, m(102, 12), 1450, 2000);
    let ppm = battle_rules::exp_table(tau);
    assert!(i64::from(ppm).abs_diff(1_000_000 * 1000 / 2718) <= 200);
    assert_eq!(v.transmit_pm, (ppm / 1000) as u16);
    assert_eq!(v.transmit_pm, 367);
    assert!(v.visible);
}

#[test]
fn a_full_cell_blocks() {
    let (r, o) = (ElevationGrid::flat(), ObstacleLayer::default());
    let mut s = SmokeGrid::new(8000, 20, 3, Vec2Fx::ZERO).unwrap();
    s.density[20 + 6] = 1_000_000; // row 1, column 6 (x 48 to 56 m)
    let w = world(&r, &o, &s);
    let v = sight(&w, m(2, 12), 1450, m(102, 12), 1450, 2000);
    assert_eq!(v.transmit_pm, 0);
    assert!(!v.visible);
    // Off the cell's row the same path is clear.
    assert!(sight(&w, m(2, 4), 1450, m(102, 4), 1450, 2000).visible);
}

#[test]
fn thin_smoke_is_visible_and_the_threshold_is_the_minimum_transmit() {
    let (r, o) = (ElevationGrid::flat(), ObstacleLayer::default());
    let s = smoke_cells(1500); // tau 3 over 100 m: transmit about 50 permille
    let w = world(&r, &o, &s);
    let v = sight(&w, m(2, 12), 1450, m(102, 12), 1450, 2000);
    assert_eq!(v.visible, v.transmit_pm >= 50);
    let denser = smoke_cells(2000);
    assert!(
        !sight(
            &world(&r, &o, &denser),
            m(2, 12),
            1450,
            m(102, 12),
            1450,
            2000
        )
        .visible
    );
}

#[test]
fn smoke_outside_the_grid_reads_clear() {
    let (r, o) = (ElevationGrid::flat(), ObstacleLayer::default());
    let s = SmokeGrid::empty();
    assert_eq!(
        optical_depth_micro(&world(&r, &o, &s), m(0, 0), m(50, 0), 2000),
        0
    );
}

// ---- signal quality ----

fn weather(vis_mm: i32, light_pm: i32) -> WeatherNow {
    WeatherNow {
        temp_f10: 700,
        humidity_pm: 500,
        wind_x_mm_s: 0,
        wind_y_mm_s: 0,
        rain_pm: 0,
        clear_vis_mm: vis_mm,
        light_pm,
    }
}

#[test]
fn signal_quality_is_transmit_within_vision_and_zero_beyond() {
    let (r, o) = (ElevationGrid::flat(), ObstacleLayer::default());
    let s = smoke_cells(500);
    let w = world(&r, &o, &s);
    let (a, b) = (m(2, 12), m(102, 12));
    let day = weather(16_000_000, 1000);
    assert_eq!(signal_quality_pm(&w, a, 1450, b, 1450, 4000, &day), 367);
    // Night: 16 km * 5 percent = 800 m still reaches; 100 m of visibility does not.
    let dusk = weather(16_000_000, 50);
    assert_eq!(signal_quality_pm(&w, a, 1450, b, 1450, 4000, &dusk), 367);
    let fog = weather(200_000, 400); // 80 m
    assert_eq!(signal_quality_pm(&w, a, 1450, b, 1450, 4000, &fog), 0);
    let edge = weather(250_000, 400); // exactly 100 m
    assert_eq!(signal_quality_pm(&w, a, 1450, b, 1450, 4000, &edge), 367);
}

#[test]
fn signal_quality_is_zero_without_clearance_even_in_range() {
    let r = ridge(30);
    let (o, s) = (ObstacleLayer::default(), SmokeGrid::empty());
    let w = world(&r, &o, &s);
    let day = weather(16_000_000, 1000);
    assert_eq!(
        signal_quality_pm(&w, m(30, 12), 1450, m(130, 12), 1450, 4000, &day),
        0
    );
    assert_eq!(
        signal_quality_pm(&w, m(30, 12), 1450, m(60, 12), 1450, 4000, &day),
        1000
    );
}
