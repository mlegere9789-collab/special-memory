//! T-123 supporting checks: weather conversion, range limit, distance clamp and bearing.

use acoustics_model::propagation::path_result;
use acoustics_model::{to_mm, AcousticTables, Weather};
use battle_rules::fx_ratio;
use core_math::{Angle, Fx, Vec2Fx};

fn tables() -> AcousticTables {
    AcousticTables::from_toml_str(include_str!("../../../data/rules/acoustics.toml")).unwrap()
}

const CALM: Weather = Weather {
    temp_dc: 200,
    wind_mm_s: [0, 0],
    lapse_mk_per_m: 0,
};

#[test]
fn to_mm_floors() {
    assert_eq!(to_mm(Fx::from_int(3)), 3000);
    assert_eq!(to_mm(Fx::from_int(-3)), -3000);
    assert_eq!(to_mm(fx_ratio(3, 2)), 1500);
    assert_eq!(to_mm(Fx::from_raw(1)), 0);
    assert_eq!(to_mm(Fx::from_raw(-1)), -1);
}

#[test]
fn from_sim_converts_the_wind() {
    let wind = Vec2Fx {
        x: fx_ratio(-6, 1),
        y: fx_ratio(3, 2),
    };
    let w = Weather::from_sim(200, wind, -20);
    assert_eq!(
        w,
        Weather {
            temp_dc: 200,
            wind_mm_s: [-6000, 1500],
            lapse_mk_per_m: -20
        }
    );
}

#[test]
fn presets_convert() {
    let t = tables();
    let w = Weather::from_preset(t.weather("toward_plus_x_inversion").unwrap());
    assert_eq!(
        w,
        Weather {
            temp_dc: 200,
            wind_mm_s: [4000, 0],
            lapse_mk_per_m: 30
        }
    );
}

#[test]
fn close_paths_use_one_metre() {
    let t = tables();
    let p = path_result(&t, &CALM, [0, 0], 1500, [300, 400], 1500, None).unwrap();
    assert_eq!(p.distance_mm, 1000);
    assert_eq!(p.loss_cdb, [0, 0, 0]);
    let q = path_result(&t, &CALM, [5, 5], 1500, [5, 5], 1500, None).unwrap();
    assert_eq!(q.distance_mm, 1000);
    assert_eq!(q.bearing, Angle::ZERO, "coincident points have no bearing");
}

#[test]
fn range_limit() {
    let t = tables();
    assert!(path_result(&t, &CALM, [0, 0], 1500, [100_000_000, 0], 1500, None).is_some());
    assert!(path_result(&t, &CALM, [0, 0], 1500, [100_000_001, 0], 1500, None).is_none());
    assert!(path_result(
        &t,
        &CALM,
        [0, 0],
        1500,
        [75_000_000, 75_000_000],
        1500,
        None
    )
    .is_none());
}

#[test]
fn bearing_points_from_the_listener_to_the_source() {
    let t = tables();
    let at = |lis: [i64; 2]| {
        path_result(&t, &CALM, [0, 0], 1500, lis, 1500, None)
            .unwrap()
            .bearing
    };
    // Source at the origin: it lies in -x of a listener at +x.
    assert_eq!(at([5000, 0]), Angle::HALF);
    assert_eq!(at([-5000, 0]), Angle::ZERO);
    assert_eq!(at([0, 5000]), Angle::from_raw(3 << 30));
    assert_eq!(at([0, -5000]), Angle::QUARTER);
}

#[test]
fn swapping_source_and_listener_gives_the_same_loss_without_wind() {
    let t = tables();
    let a = path_result(
        &t,
        &CALM,
        [1000, 2000],
        1500,
        [9_000_000, 4_000_000],
        1700,
        None,
    )
    .unwrap();
    let b = path_result(
        &t,
        &CALM,
        [9_000_000, 4_000_000],
        1700,
        [1000, 2000],
        1500,
        None,
    )
    .unwrap();
    assert_eq!(a.loss_cdb, b.loss_cdb);
    assert_eq!(a.delay_ms, b.delay_ms);
    assert_eq!(a.distance_mm, b.distance_mm);
}
