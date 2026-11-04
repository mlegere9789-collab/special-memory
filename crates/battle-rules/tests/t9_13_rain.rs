//! T9-13: powder damp and ground wetness follow the SPEC-009 section 7 formulas, `Weather` is
//! filled from the timeline, and at full damp the misfire rate is the dry rate times `wet_pm`.

mod t022_support;
mod t9_support;

use battle_rules::aim::draw_shot;
use battle_rules::fire::misfire_ppm;
use battle_rules::{fx_ratio, BattleState};
use core_math::Rng;
use t9_support::{env_table_with, weather};

const SHIPPED: [i64; 4] = [600, 150, 400, 40];
/// Fast rates, so that both states reach their limits and fall back to zero within the run.
const FAST: [i64; 4] = [600_000, 300_000, 500_000, 200_000];

/// The section 7 formulas written out again: `(damp, ground)` after one period.
fn reference(rain_pm: i64, damp: i64, ground: i64, rates: [i64; 4]) -> (i64, i64) {
    let [pr, pf, gr, gf] = rates;
    let step = |v: i64, rise: i64, fall: i64| {
        let v = if rain_pm > 0 {
            v + rain_pm * rise / 1000
        } else {
            v - fall
        };
        v.clamp(0, 1_000_000)
    };
    (step(damp, pr, pf), step(ground, gr, gf))
}

/// Runs an empty battle for `ticks` ticks of the environment step and checks every refresh
/// against the reference. Returns the battle and the largest damp and ground seen.
fn run(rates: [i64; 4], ticks: u64) -> (BattleState, (i64, i64)) {
    let wx = weather(&[
        (0, 700, 800, 300, -200, 0),
        (1, 700, 800, 300, -200, 600),
        (3, 700, 800, -450, 120, 1000),
        (4, 700, 800, -450, 120, 1000),
        (5, 700, 800, 300, -200, 0),
    ]);
    let edits = [
        (
            "powder_rise_ppm = 600",
            format!("powder_rise_ppm = {}", rates[0]),
        ),
        (
            "powder_fall_ppm = 150",
            format!("powder_fall_ppm = {}", rates[1]),
        ),
        (
            "ground_rise_ppm = 400",
            format!("ground_rise_ppm = {}", rates[2]),
        ),
        (
            "ground_fall_ppm = 40",
            format!("ground_fall_ppm = {}", rates[3]),
        ),
    ];
    let edits: Vec<(&str, &str)> = edits.iter().map(|(a, b)| (*a, b.as_str())).collect();
    let mut b = t022_support::field(1);
    b.set_environment(env_table_with(&edits), wx.clone());
    let (mut d, mut g) = (0i64, 0i64);
    let mut peak = (0, 0);
    for tick in 0..ticks {
        b.step_environment();
        if tick % 4 == 0 {
            let now = wx.at(tick);
            (d, g) = reference(i64::from(now.rain_pm), d, g, rates);
            let env = b.env.as_ref().unwrap();
            assert_eq!(i64::from(env.powder_damp_ppm()), d, "damp at tick {tick}");
            assert_eq!(i64::from(env.ground_wet_ppm()), g, "ground at tick {tick}");
            assert_eq!(i64::from(b.weather.wet_pm), 1000 + d * 2000 / 1_000_000);
            assert_eq!(i64::from(b.weather.ground_pm), 1000 - g * 500 / 1_000_000);
            assert_eq!(b.weather.wind.x, fx_ratio(now.wind_x_mm_s, 1000));
            assert_eq!(b.weather.wind.y, fx_ratio(now.wind_y_mm_s, 1000));
            peak = (peak.0.max(d), peak.1.max(g));
        }
        b.world.advance_tick();
    }
    (b, peak)
}

#[test]
fn damp_and_ground_follow_the_formulas_at_the_shipped_rates() {
    let (b, peak) = run(SHIPPED, 6400);
    assert!(peak.0 > 0 && peak.1 > 0);
    // Rain raises the ground factor above its dry value and the misfire factor above 1000.
    assert!(b.weather.wet_pm >= 1000 && b.weather.ground_pm <= 1000);
}

#[test]
fn damp_and_ground_saturate_and_dry_out() {
    let (b, peak) = run(FAST, 8000);
    assert_eq!(peak, (1_000_000, 1_000_000));
    let env = b.env.as_ref().unwrap();
    assert_eq!((env.powder_damp_ppm(), env.ground_wet_ppm()), (0, 0));
    assert_eq!((b.weather.wet_pm, b.weather.ground_pm), (1000, 1000));
}

#[test]
fn at_full_damp_the_weather_is_wet_and_misfires_scale_by_wet_pm() {
    // Tick 4800 is four minutes in: full rain for 1.5 minutes at the fast rates.
    let (b, _) = run(FAST, 4801);
    assert_eq!(b.env.as_ref().unwrap().powder_damp_ppm(), 1_000_000);
    assert_eq!(b.weather.wet_pm, 3000);
    assert_eq!(b.weather.ground_pm, 500);
    let w = b.tables.weapons.get("smoothbore_69").unwrap();
    let want = misfire_ppm(w, 0, b.weather.wet_pm);
    assert_eq!(want, misfire_ppm(w, 0, 1000) * 3);
    let n = 200_000u32;
    let mut rng = Rng::from_seed(913);
    let hits = (0..n)
        .filter(|_| draw_shot(&mut rng).misfire_roll % 1_000_000 < want)
        .count();
    let p = f64::from(want) / 1e6;
    let mean = f64::from(n) * p;
    let sigma = (f64::from(n) * p * (1.0 - p)).sqrt();
    assert!(
        (hits as f64 - mean).abs() <= 3.0 * sigma,
        "{hits} misfires, expected {mean} +- {sigma}"
    );
}

#[test]
fn set_environment_fills_weather_at_the_current_tick_without_a_rain_step() {
    let wx = weather(&[(0, 700, 800, 1000, 0, 1000)]);
    let mut b = t022_support::field(1);
    b.world.tick = 4;
    b.set_environment(env_table_with(&[]), wx);
    let env = b.env.as_ref().unwrap();
    assert_eq!((env.powder_damp_ppm(), env.ground_wet_ppm()), (0, 0));
    assert_eq!(b.weather.wind.x, fx_ratio(1000, 1000));
    assert_eq!((b.weather.wet_pm, b.weather.ground_pm), (1000, 1000));
    // Ticks that are not a multiple of the period change nothing.
    b.world.tick = 5;
    b.step_environment();
    assert_eq!(b.env.as_ref().unwrap().powder_damp_ppm(), 0);
    b.world.tick = 8;
    b.step_environment();
    assert_eq!(b.env.as_ref().unwrap().powder_damp_ppm(), 600);
}
