//! The environment calibration harness (SPEC-009 sections 10 and 11, T9-19): the scenarios
//! `smoke_line`, `crest`, `heat_march` and `hill_hq` reduced to the metrics of bands E1 to E8.
//!
//! Every metric is an integer; a band holds when the mean over seeds 1 to 8 is inside it. Rows
//! are reported as measured, never tuned: the band file is the only place limits live.
//!
//! Measurements (the spec fixes the quantity, this module the rays and clocks):
//! * E1, E2: after 1200 ticks of `smoke_line` (E2 with a 3 m/s wind across the line of fire),
//!   the mean `transmit_pm` of five standing-eye to standing-aim rays between the lines, from
//!   one anchor to the other shifted by -20, -10, 0, 10 and 20 m in y.
//! * E3: both regiments hold fire at tick 1200; the seconds (ticks over 20, floor) until that
//!   mean transmit reaches 500, capped at 600 s.
//! * E4: casualties of `smoke_line` at 183 m over casualties of the same `cal_line` with the
//!   environment off, 2400 ticks, ratio of the sums.
//! * E5: mean fatigue of a 100-man column after 18,000 ticks (900 s) of quick-time marching from
//!   14:00 on 2 July over the same from 04:30 on 2 July.
//! * E6: misfire ppm of `rifle_58` at the wetness the damp state reaches in steady full rain
//!   over the dry value.
//! * E7: share of the men of `crest` that have fired after 1200 ticks (60 s).
//! * E8: share of the enemy regiments of `hill_hq` (no fight) within 1500 m with a clear
//!   line from the hill (relief and walls, no smoke, any man, fine step) that the hill observer
//!   logged during the first 100 ticks.

use crate::calibration::bands::{parse_bands, Band};
use crate::calibration::fire::{per_seed, BandRow, SuiteReport, SEEDS};
use crate::scenarios::env::{
    calm_weather, crest, hill_hq, smoke_line_with, CROSSWIND_MM_S, DAY2_0430, DAY2_1400, HQ_ID,
    LINE_RANGE_MM,
};
use crate::scenarios::{env::heat_march, fire::cal_line};
use crate::{
    aim_mm, distance_mm, eye_mm, misfire_ppm, sight, BattleState, DataError, Environment, FireMode,
    Posture, RegimentId, SightWorld, Tables, Training, TICK_HZ,
};
use core_math::{Fx, Vec2Fx};

/// Ticks of an `smoke_line` run for E4 and of the line before the measurement of E1 and E2.
const LINE_TICKS: u64 = 2400;
const MEASURE_TICK: u64 = 1200;
/// Longest wait of E3 after the cease-fire, seconds.
const RECOVER_CAP_S: i64 = 600;
/// The window of E5: 900 s (SPEC-009 amendment A2).
const MARCH_TICKS: u64 = 900 * TICK_HZ as u64;
/// Men of the column of E5.
const MARCH_MEN: usize = 100;
/// Ticks of the observation of E8.
const HILL_TICKS: u64 = 100;
/// Range inside which E8 counts enemy regiments, millimetres.
const HILL_RANGE_MM: i64 = 1_500_000;
/// Lateral offsets of the rays of E1 to E3, metres.
const RAYS_M: [i32; 5] = [-20, -10, 0, 10, 20];

fn line_pos(b: &BattleState, r: usize, dy: i32) -> Vec2Fx {
    b.regiments[r].anchor + Vec2Fx::new(Fx::ZERO, Fx::from_int(dy))
}

/// Mean transmission of the rays between the two regiments of a line scenario, permille.
fn line_transmit(b: &BattleState) -> i64 {
    let env = b.env.as_ref().expect("environment is on");
    let table = env.table();
    let world = SightWorld::new(
        env.relief(),
        &b.obstacles,
        &b.smoke,
        &table.smoke,
        &table.sight,
    );
    let (eye, aim) = (
        eye_mm(&b.tables.damage, Posture::Standing),
        aim_mm(&b.tables.damage, Posture::Standing),
    );
    let sum: i64 = RAYS_M
        .iter()
        .map(|&dy| {
            let s = sight(
                &world,
                line_pos(b, 0, dy),
                eye,
                line_pos(b, 1, dy),
                aim,
                table.sight.step_mm,
            );
            i64::from(s.transmit_pm)
        })
        .sum();
    sum / RAYS_M.len() as i64
}

fn run(b: &mut BattleState, ticks: u64) {
    for _ in 0..ticks {
        b.step();
    }
}

fn casualties(b: &BattleState) -> i64 {
    b.casualties().len() as i64
}

/// E1 and E2: transmission between the lines at tick 1200.
fn transmit_at_1200(tables: &Tables, seed: u64, wind_y_mm_s: i32) -> i64 {
    let mut b = smoke_line_with(tables, seed, LINE_RANGE_MM, wind_y_mm_s);
    run(&mut b, MEASURE_TICK);
    line_transmit(&b)
}

/// E3: seconds from the cease-fire at tick 1200 until the transmission reaches 500.
fn recovery_seconds(tables: &Tables, seed: u64) -> i64 {
    let mut b = smoke_line_with(tables, seed, LINE_RANGE_MM, 0);
    run(&mut b, MEASURE_TICK);
    for r in 0..2u16 {
        assert!(b.set_fire_mode(RegimentId(r), FireMode::HoldFire));
    }
    let tps = i64::from(TICK_HZ);
    let mut waited = 0;
    while line_transmit(&b) < 500 && waited < RECOVER_CAP_S * tps {
        b.step();
        waited += 1;
    }
    waited / tps
}

/// E4: casualties with the smoke over casualties of the same lines without the layer.
fn smoke_casualties(tables: &Tables, seed: u64, range_mm: u32) -> (i64, i64) {
    let mut with = smoke_line_with(tables, seed, range_mm, 0);
    let mut without = cal_line(tables, seed, "rifle_58", Training::Trained, range_mm, false);
    run(&mut with, LINE_TICKS);
    run(&mut without, LINE_TICKS);
    (casualties(&with), casualties(&without))
}

/// Mean fatigue (ppm) of the column of E5 after 900 s from `start_minute`.
fn fatigue_after_window(tables: &Tables, seed: u64, start_minute: i64) -> i64 {
    let mut b = heat_march(tables, seed, start_minute, MARCH_MEN);
    run(&mut b, MARCH_TICKS);
    let men = &b.regiments[0].roster;
    let sum: i64 = men
        .iter()
        .map(|id| i64::from(b.loco[id.index as usize].fatigue))
        .sum();
    sum / men.len().max(1) as i64
}

/// E6: the misfire factor of full dampness, permille of the dry rate.
fn misfire_ratio(tables: &Tables) -> i64 {
    let table = crate::scenarios::env::embedded_env(tables);
    let rows = tables.weapons.rows();
    let w = &rows[tables
        .weapons
        .index_of("rifle_58")
        .expect("rifle_58 exists")];
    let mut env = Environment::new(table, calm_weather(0, 0, 1000), 0);
    let dry = i64::from(misfire_ppm(w, 0, env.weather_state().wet_pm));
    let mut tick = 0;
    while env.powder_damp_ppm() < 1_000_000 && tick < 100_000 {
        env.refresh(tick);
        tick += 1;
    }
    let wet = i64::from(misfire_ppm(w, 0, env.weather_state().wet_pm));
    wet * 1000 / dry.max(1)
}

/// E7: men of `crest` that have fired after 60 s, permille.
fn crest_share(tables: &Tables, seed: u64) -> i64 {
    let mut b = crest(tables, seed);
    let before: Vec<u16> = b.regiments[0]
        .roster
        .iter()
        .map(|a| b.fire[a.index as usize].rounds)
        .collect();
    run(&mut b, MEASURE_TICK);
    let roster = &b.regiments[0].roster;
    let fired = roster
        .iter()
        .zip(&before)
        .filter(|(a, r0)| b.fire[a.index as usize].rounds < **r0)
        .count();
    (fired * 1000 / roster.len().max(1)) as i64
}

/// E8: `(logged, with a clear line)` enemy regiments within 1500 m of the hill.
fn hill_counts(tables: &Tables, seed: u64) -> (i64, i64) {
    let mut b = hill_hq(tables, seed, false);
    run(&mut b, HILL_TICKS);
    let env = b.env.as_ref().expect("environment is on");
    let table = env.table();
    let world = SightWorld::new(
        env.relief(),
        &b.obstacles,
        &b.smoke,
        &table.smoke,
        &table.sight,
    );
    let hq = Vec2Fx::new(Fx::from_int(100), Fx::from_int(256));
    let aim = aim_mm(&b.tables.damage, Posture::Standing);
    let (mut seen, mut truth) = (0, 0);
    for (i, reg) in b.regiments.iter().enumerate().filter(|(_, r)| r.team == 1) {
        let near = distance_mm(hq, reg.anchor) <= HILL_RANGE_MM;
        let clear = reg.roster.iter().any(|id| {
            let at = b.world.agents.pos(*id).expect("soldier is placed");
            sight(&world, hq, 1700, at, aim, table.sight.step_mm).visible
        });
        if near && clear {
            truth += 1;
            let log = b.sightings(0).iter();
            seen += i64::from(
                log.into_iter()
                    .any(|s| s.enemy.0 as usize == i && s.observer == HQ_ID),
            );
        }
    }
    (seen, truth)
}

fn mean(v: &[i64]) -> i64 {
    v.iter().sum::<i64>() / v.len().max(1) as i64
}

fn ratio_milli(a: i64, b: i64) -> i64 {
    match (a, b) {
        (0, 0) => 1000,
        (_, 0) => 1_000_000,
        _ => a * 1000 / b,
    }
}

fn judged(band: &Band, label: &str, per_seed: Vec<i64>, mean: i64, p: usize) -> BandRow {
    let point = &band.points[p];
    BandRow {
        id: band.id.clone(),
        label: label.to_string(),
        min: per_seed.iter().copied().min().unwrap_or(mean),
        max: per_seed.iter().copied().max().unwrap_or(mean),
        per_seed,
        mean,
        low: point.low,
        high: point.high,
        scale: band.scale,
        pass: point.low.is_none_or(|l| mean >= l) && point.high.is_none_or(|h| mean <= h),
    }
}

fn rows_of(band: &Band, tables: &Tables) -> Result<Vec<BandRow>, DataError> {
    let mut out = Vec::new();
    for (p, point) in band.points.iter().enumerate() {
        let scalar = |v: Vec<i64>| {
            let m = mean(&v);
            (v, m)
        };
        let (label, v, m) = match (band.metric.as_str(), band.weapon.as_str()) {
            ("line_transmit", "calm") => {
                let (v, m) = scalar(per_seed(&SEEDS, |s| transmit_at_1200(tables, s, 0)));
                ("calm", v, m)
            }
            ("line_transmit", "crosswind") => {
                let (v, m) = scalar(per_seed(&SEEDS, |s| {
                    transmit_at_1200(tables, s, CROSSWIND_MM_S)
                }));
                ("crosswind", v, m)
            }
            ("line_recover_s", _) => {
                let (v, m) = scalar(per_seed(&SEEDS, |s| recovery_seconds(tables, s)));
                ("calm", v, m)
            }
            ("smoke_casualty_ratio", _) => {
                let runs = per_seed(&SEEDS, |s| smoke_casualties(tables, s, point.at_mm));
                let v = runs.iter().map(|&(a, b)| ratio_milli(a, b)).collect();
                let m = ratio_milli(
                    runs.iter().map(|r| r.0).sum(),
                    runs.iter().map(|r| r.1).sum(),
                );
                ("183 m", v, m)
            }
            ("heat_fatigue_ratio", _) => {
                let runs = per_seed(&SEEDS, |s| {
                    (
                        fatigue_after_window(tables, s, DAY2_1400),
                        fatigue_after_window(tables, s, DAY2_0430),
                    )
                });
                let v = runs.iter().map(|&(h, c)| ratio_milli(h, c)).collect();
                let m = ratio_milli(
                    runs.iter().map(|r| r.0).sum(),
                    runs.iter().map(|r| r.1).sum(),
                );
                ("900 s quick", v, m)
            }
            ("misfire_ratio", _) => {
                let m = misfire_ratio(tables);
                ("full damp", vec![m], m)
            }
            ("crest_share", _) => {
                let (v, m) = scalar(per_seed(&SEEDS, |s| crest_share(tables, s)));
                ("60 s", v, m)
            }
            ("hill_share", _) => {
                let runs = per_seed(&SEEDS, |s| hill_counts(tables, s));
                let v = runs.iter().map(|&(a, b)| ratio_milli(a, b)).collect();
                let m = ratio_milli(
                    runs.iter().map(|r| r.0).sum(),
                    runs.iter().map(|r| r.1).sum(),
                );
                ("hill", v, m)
            }
            _ => {
                return Err(DataError::Parse(format!(
                    "unknown metric {:?}",
                    band.metric
                )))
            }
        };
        out.push(judged(band, label, v, m, p));
    }
    Ok(out)
}

/// Runs the bands of family `env` (E1 to E8) in the text of the band file and returns every
/// measured row, in file order.
///
/// Errors: [`DataError`] when the band file does not parse or names an unknown metric.
pub fn run_env_suite(tables: &Tables, bands_toml: &str) -> Result<SuiteReport, DataError> {
    let mut rows = Vec::new();
    for band in parse_bands(bands_toml)?
        .iter()
        .filter(|b| b.family == "env")
    {
        rows.extend(rows_of(band, tables)?);
    }
    Ok(SuiteReport {
        family: "env".to_string(),
        rows,
        lines: Vec::new(),
        canister: Vec::new(),
    })
}
