//! T12-03: sound speed and delay (SPEC-012 section 6 item 1). Floating point appears only here,
//! as the independent reference.

use acoustics_model::propagation::{delay_ms, path_result, sound_speed_mm_s};
use acoustics_model::{AcousticTables, Weather};
use proptest::prelude::*;

fn tables() -> AcousticTables {
    AcousticTables::from_toml_str(include_str!("../../../data/rules/acoustics.toml")).unwrap()
}

#[test]
fn sound_speed_law() {
    let t = tables();
    assert_eq!(sound_speed_mm_s(&t, 200), 343_420);
    assert_eq!(sound_speed_mm_s(&t, 0), 331_300);
    assert_eq!(sound_speed_mm_s(&t, -100), 325_240);
}

#[test]
fn delay_over_one_kilometre() {
    let c = 343_420;
    assert_eq!(delay_ms(1_000_000, c, 0), 2912);
    assert_eq!(delay_ms(1_000_000, c, 5000), 2871);
    assert_eq!(delay_ms(1_000_000, c, -5000), 2955);
}

#[test]
fn effective_speed_never_drops_below_half() {
    // A wind of -100 m/s would stop the sound; the floor is c / 2.
    assert_eq!(delay_ms(343_420, 343_420, -343_420), 2000);
    assert_eq!(delay_ms(0, 343_420, 0), 0);
}

#[test]
fn path_delay_follows_the_along_wind_component() {
    let t = tables();
    let calm = Weather {
        temp_dc: 200,
        wind_mm_s: [0, 0],
        lapse_mk_per_m: 0,
    };
    let tail = Weather {
        wind_mm_s: [5000, 0],
        ..calm
    };
    let head = Weather {
        wind_mm_s: [-5000, 0],
        ..calm
    };
    let side = Weather {
        wind_mm_s: [0, 5000],
        ..calm
    };
    let at = |w: &Weather| {
        path_result(&t, w, [0, 0], 1500, [1_000_000, 0], 1500, None)
            .unwrap()
            .delay_ms
    };
    assert_eq!(at(&calm), 2912);
    assert_eq!(at(&tail), 2871);
    assert_eq!(at(&head), 2955);
    assert_eq!(at(&side), 2912);
}

proptest! {
    #[test]
    fn delay_matches_f64_within_one_ms(
        d in 1000i64..=100_000_000,
        temp_dc in -200i16..=400,
        wind in -15_000i32..=15_000,
    ) {
        let t = tables();
        let w = Weather { temp_dc, wind_mm_s: [wind, 0], lapse_mk_per_m: 0 };
        let got = path_result(&t, &w, [0, 0], 1500, [d, 0], 1500, None).unwrap().delay_ms;
        // The sound-speed law truncates to whole mm/s (SPEC-012 section 6); the rest is f64.
        let c = (331_300.0 + (606.0 * f64::from(temp_dc) / 10.0).trunc()) / 1000.0;
        let c_eff = (c + f64::from(wind) / 1000.0).max(c / 2.0);
        let want = d as f64 / 1000.0 / c_eff * 1000.0;
        prop_assert!((got as f64 - want).abs() <= 1.0, "got {got} want {want}");
    }
}
