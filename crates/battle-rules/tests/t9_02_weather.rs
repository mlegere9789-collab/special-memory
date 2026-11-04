//! T9-02: the weather timeline reproduces its keys exactly at their ticks, matches the
//! `qc/fixtures/ref_weather.py` reference (exact `Fraction`, truncated) between them, is clamped
//! outside the keys and is held between refresh periods (SPEC-009 section 3.2).

use battle_rules::{WeatherNow, WeatherTable};

const WEATHER: &str = include_str!("../../../data/scenarios/gettysburg/weather.toml");
const REF: &str = include_str!("../../../qc/fixtures/ref_weather.csv");
const TICKS_PER_MINUTE: u64 = 1200;

fn gettysburg() -> WeatherTable {
    WeatherTable::from_toml_str(WEATHER).unwrap()
}

fn now(v: [i64; 7]) -> WeatherNow {
    WeatherNow {
        temp_f10: v[0] as i32,
        humidity_pm: v[1] as i32,
        wind_x_mm_s: v[2],
        wind_y_mm_s: v[3],
        rain_pm: v[4] as i32,
        clear_vis_mm: v[5] as i32,
        light_pm: v[6] as i32,
    }
}

const ONE_KEY: &str = "schema = \"weather/1\"\nstart_minute = 10\nkey = [\n";
const KEY_A: &str = "  { t_min = 10, temp_f10 = 700, humidity_pm = 500, wind_x_mm_s = -3, wind_y_mm_s = 7, rain_pm = 0, clear_vis_mm = 9000, light_pm = 0, confidence = \"unverified\", placeholder = true, calibration_target = \"CT-009-6\" },\n";
const KEY_B: &str = "  { t_min = 11, temp_f10 = 600, humidity_pm = 502, wind_x_mm_s = 10, wind_y_mm_s = -7, rain_pm = 1000, clear_vis_mm = 9001, light_pm = 1000, confidence = \"unverified\", placeholder = true, calibration_target = \"CT-009-6\" },\n";

#[test]
fn keys_reproduce_exactly_at_their_ticks() {
    let w = gettysburg();
    for k in w.keys() {
        let tick = u64::try_from(k.t_min - w.start_minute()).unwrap() * TICKS_PER_MINUTE;
        let n = w.at(tick);
        assert_eq!(n.temp_f10, k.temp_f10);
        assert_eq!(n.humidity_pm, i32::from(k.humidity_pm));
        assert_eq!(
            (n.wind_x_mm_s, n.wind_y_mm_s),
            (k.wind_x_mm_s.into(), k.wind_y_mm_s.into())
        );
        assert_eq!(
            (n.rain_pm, n.light_pm),
            (i32::from(k.rain_pm), i32::from(k.light_pm))
        );
        assert_eq!(n.clear_vis_mm, k.clear_vis_mm);
    }
}

#[test]
fn midpoints_equal_the_reference() {
    let w = gettysburg();
    let mut rows = 0;
    for line in REF.lines() {
        let f: Vec<i64> = line
            .split(',')
            .skip(1)
            .map(|x| x.parse().unwrap())
            .collect();
        let tick = u64::try_from(f[0]).unwrap();
        let want = now([f[1], f[2], f[3], f[4], f[5], f[6], f[7]]);
        assert_eq!(w.at(tick), want, "tick {tick}");
        rows += 1;
    }
    assert_eq!(rows, 141);
}

#[test]
fn truncation_runs_toward_zero_and_start_minute_shifts_the_clock() {
    let text = format!("{ONE_KEY}{KEY_A}{KEY_B}]\n");
    let w = WeatherTable::from_toml_str(&text).unwrap();
    // 1/1200 of the way: wind_x -3 -> 10 is +13/1200 (truncates to -3), temp 700 -> 600 is
    // -100/1200 (truncates to 700), wind_y 7 -> -7 is -14/1200 (truncates to 7).
    let n = w.at(1);
    assert_eq!((n.temp_f10, n.wind_x_mm_s, n.wind_y_mm_s), (700, -3, 7));
    // Halfway: wind_x -3 + trunc(13 * 600 / 1200) = 3, wind_y 7 + trunc(-14 * 600 / 1200) = 0.
    let n = w.at(600);
    assert_eq!(
        (n.temp_f10, n.wind_x_mm_s, n.wind_y_mm_s, n.humidity_pm),
        (650, 3, 0, 501)
    );
    assert_eq!(w.at(1199).wind_x_mm_s, 9);
}

#[test]
fn clamped_before_the_first_key_and_after_the_last() {
    let text = format!(
        "{}{KEY_A}{KEY_B}]\n",
        ONE_KEY.replace("start_minute = 10", "start_minute = 5")
    );
    let w = WeatherTable::from_toml_str(&text).unwrap();
    let first = w.at(0);
    assert_eq!((first.temp_f10, first.rain_pm), (700, 0));
    assert_eq!(w.at(5 * TICKS_PER_MINUTE - 1), first);
    let last = w.at(6 * TICKS_PER_MINUTE);
    assert_eq!(
        (last.temp_f10, last.rain_pm, last.light_pm),
        (600, 1000, 1000)
    );
    assert_eq!(w.at(u64::from(u32::MAX)), last);
    let g = gettysburg();
    assert_eq!(g.at(100_000_000).rain_pm, 300);
}

#[test]
fn held_between_refresh_periods() {
    let w = WeatherTable::from_toml_str(&format!("{ONE_KEY}{KEY_A}{KEY_B}]\n")).unwrap();
    let period = 48;
    for tick in 0..300u64 {
        let held = w.at_period(tick, period);
        assert_eq!(held, w.at(tick - tick % period));
        if tick % period != 0 {
            assert_eq!(held, w.at_period(tick - 1, period));
        }
    }
    assert_eq!(w.at_period(47, period).temp_f10, 700);
    assert_eq!(w.at_period(48, period).temp_f10, 696);
    assert_eq!(w.at_period(5, 0), w.at(5));
}
