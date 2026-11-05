//! SPEC-009 scenarios (sections 10 and 11): `smoke_line`, `crest` (the band set-ups E1 to E7),
//! the golden battles `env_smoke_line`, `env_hill_hq` and `env_heat_march`, and `f0_large_env`
//! for the performance test.
//!
//! Every scenario builds a [`BattleState`] at tick 0 with the environment on: the rules of
//! `data/rules/environment.toml` (embedded, so the library and its tests read one file), a
//! smoke grid the size of the terrain, and a weather timeline. Calm weather is one key
//! (70 F, humidity 0.8, no wind, no rain, clear visibility 16 km, full light); the Gettysburg
//! timeline is `data/scenarios/gettysburg/weather.toml` with `start_minute` set to the clock
//! time the scenario starts at. The only random draws are the soldier streams.

use crate::scenarios::fire::{cal_line, f0_large};
use crate::{
    BattleState, ElevationGrid, EnvTable, FireMode, Formation, Gait, Observer, Order, RegimentSpec,
    SmokeGrid, Tables, TerrainGrid, Training, WeatherTable,
};
use core_math::{Angle, Fx, RngStreams, Vec2Fx};
use sim_state::{SpatialGrid, WorldState};

const ENV_TOML: &str = include_str!("../../../../data/rules/environment.toml");
const GETTYSBURG: &str = include_str!("../../../../data/scenarios/gettysburg/weather.toml");
/// Minutes after 00:00 on 1 July of 14:00 on 2 July: the hot afternoon of the heat bands.
pub const DAY2_1400: i64 = 1440 + 14 * 60;
/// Minutes after 00:00 on 1 July of 04:30 on 2 July: the cool morning they are compared with.
pub const DAY2_0430: i64 = 1440 + 270;
/// Minutes after 00:00 on 1 July of 14:00 on 3 July.
const DAY3_1400: i64 = 2 * 1440 + 14 * 60;
/// The crosswind of band E2, millimetres per second.
pub const CROSSWIND_MM_S: i32 = 3000;
/// Distance of the lines of `smoke_line`, millimetres (100 yd).
pub const LINE_RANGE_MM: u32 = 91_440;
/// Men of the regiment in `crest`.
pub const CREST_MEN: usize = 30;
/// Height of the crest in `crest`, decimetres.
pub const CREST_DM: i32 = 15;
/// Names of the scenarios of this module.
pub const NAMES: [&str; 5] = [
    "env_smoke_line",
    "env_hill_hq",
    "env_heat_march",
    "smoke_line",
    "crest",
];
/// Id of the headquarters observer of `env_hill_hq`.
pub const HQ_ID: u16 = crate::OBSERVER_ID_BASE;

fn metres(x: i32, y: i32) -> Vec2Fx {
    Vec2Fx::new(Fx::from_int(x), Fx::from_int(y))
}

/// The rules of `data/rules/environment.toml`.
///
/// # Panics
/// Panics when the shipped file does not load against the terrain table of `tables`.
pub fn embedded_env(tables: &Tables) -> EnvTable {
    EnvTable::from_toml_str(ENV_TOML, &tables.terrain).expect("environment.toml is valid")
}

/// A timeline of one key: no rain, clear visibility 16 km, full light, wind
/// `(wind_x_mm_s, wind_y_mm_s)`, `rain_pm` rain.
///
/// # Panics
/// Panics only if the key text built here stopped being valid.
pub fn calm_weather(wind_x_mm_s: i32, wind_y_mm_s: i32, rain_pm: u32) -> WeatherTable {
    let text = format!(
        "schema = \"weather/1\"\nstart_minute = 0\nkey = [\n  {{ t_min = 0, temp_f10 = 700, \
         humidity_pm = 800, wind_x_mm_s = {wind_x_mm_s}, wind_y_mm_s = {wind_y_mm_s}, \
         rain_pm = {rain_pm}, clear_vis_mm = 16000000, light_pm = 1000, \
         confidence = \"unverified\", placeholder = true, calibration_target = \"CT-009-6\" }},\n]\n"
    );
    WeatherTable::from_toml_str(&text).expect("calm weather is valid")
}

/// The Gettysburg timeline read from `start_minute` (minutes after 00:00 on 1 July).
///
/// # Panics
/// Panics when the shipped file does not load.
pub fn gettysburg_weather(start_minute: i64) -> WeatherTable {
    let text = GETTYSBURG.replacen(
        "start_minute = 0",
        &format!("start_minute = {start_minute}"),
        1,
    );
    WeatherTable::from_toml_str(&text).expect("gettysburg weather is valid")
}

/// An empty battle on `w * 8 m` by `h * 8 m` of open ground (terrain and spatial cells of 8 m)
/// with the environment on and a smoke grid of the same size.
fn field(tables: &Tables, seed: u64, (w, h): (u32, u32), weather: WeatherTable) -> BattleState {
    let terrain = TerrainGrid::from_rows(8000, 0, 0, &[('.', 0)], &["."], &tables.terrain)
        .expect("scenario terrain is valid");
    let grid = SpatialGrid::new(Fx::from_int(8), w, h, Vec2Fx::ZERO).expect("grid size is valid");
    let mut b = BattleState::new(
        WorldState::new(RngStreams::new(seed)),
        terrain,
        grid,
        tables.clone(),
    );
    switch_on(&mut b, (w, h), weather);
    b
}

/// Allocates a smoke grid of `w` by `h` cells of 8 m from the origin and switches the
/// environment on.
fn switch_on(b: &mut BattleState, (w, h): (u32, u32), weather: WeatherTable) {
    b.smoke = SmokeGrid::new(8000, w, h, Vec2Fx::ZERO).expect("smoke grid size is valid");
    b.set_environment(embedded_env(&b.tables), weather);
}

fn line(team: u8, count: usize, anchor: Vec2Fx) -> RegimentSpec {
    RegimentSpec {
        team,
        count,
        training: Training::Trained,
        formation: Formation::Line,
        anchor,
        heading: if team == 0 { Angle::ZERO } else { Angle::HALF },
    }
}

/// `smoke_line` with `range_mm` between the lines and a wind of `wind_y_mm_s` across the line
/// of fire: `cal_line(rifle_58, trained, range_mm)` (two regiments of 200, firing at will) with
/// the environment on in calm weather plus that wind.
pub fn smoke_line_with(tables: &Tables, seed: u64, range_mm: u32, wind_y_mm_s: i32) -> BattleState {
    let mut b = cal_line(tables, seed, "rifle_58", Training::Trained, range_mm, false);
    switch_on(&mut b, (256, 64), calm_weather(0, wind_y_mm_s, 0));
    b
}

/// `smoke_line`: two regiments of 200 riflemen 91.44 m apart firing at will, calm (bands E1,
/// E3; E2 adds [`CROSSWIND_MM_S`] and E4 uses 183 m).
pub fn smoke_line(tables: &Tables, seed: u64) -> BattleState {
    smoke_line_with(tables, seed, LINE_RANGE_MM, 0)
}

/// `env_smoke_line`: `smoke_line` under the Gettysburg weather of 14:00 on 3 July (warm, a
/// light and slowly turning wind), the golden of the smoke grid and the gate.
pub fn env_smoke_line(tables: &Tables, seed: u64) -> BattleState {
    let mut b = cal_line(
        tables,
        seed,
        "rifle_58",
        Training::Trained,
        LINE_RANGE_MM,
        false,
    );
    switch_on(&mut b, (256, 64), gettysburg_weather(DAY3_1400));
    b
}

/// The relief of `crest_with`: open ground that rises from column 8 (68 m) to `crest_dm`
/// decimetres at column 16 (132 m) and falls away by column 19, on the 256 by 64 cell field.
fn crest_relief(crest_dm: i32) -> ElevationGrid {
    let profile = |col: i32| -> i32 {
        match col {
            ..=8 => 0,
            9..=16 => crest_dm * (col - 8) / 8,
            17..=19 => crest_dm * (19 - col) / 3,
            _ => 0,
        }
    };
    let row: Vec<i32> = (0..256).map(profile).collect();
    ElevationGrid::from_rows(8000, 0, 0, &vec![row; 64]).expect("relief rows are valid")
}

/// `crest` with a crest of `crest_dm` decimetres and `men` men: the regiment stands at x = 72 m,
/// 60 m behind the crest line, facing east and firing at will on a line of 20 enemy men at
/// x = 260 m that holds fire; the men that cannot see walk forward to peek (band E7).
pub fn crest_with(tables: &Tables, seed: u64, crest_dm: i32, men: usize) -> BattleState {
    let mut b = field(tables, seed, (256, 64), calm_weather(0, 0, 0));
    let s = b.spawn_regiment(line(0, men, metres(72, 256)));
    let t = b.spawn_regiment(line(1, 20, metres(260, 256)));
    b.equip_regiment(s, "rifle_58", 0)
        .expect("scenario weapon exists");
    assert!(b.set_fire_mode(s, FireMode::AtWill { target: t }));
    if let Some(env) = b.env.as_mut() {
        env.set_relief(crest_relief(crest_dm));
    }
    b
}

/// `crest`: [`crest_with`] a 1.5 m crest and 30 men (SPEC-009 amendment A2).
pub fn crest(tables: &Tables, seed: u64) -> BattleState {
    crest_with(tables, seed, CREST_DM, CREST_MEN)
}

/// The enemy regiments of `hill_hq`: `(x, y)` in metres; the last stands behind the ridge.
pub const HILL_ENEMIES: [(i32, i32); 5] =
    [(400, 256), (700, 256), (1000, 200), (1400, 300), (640, 256)];

fn hill_relief() -> ElevationGrid {
    let row: Vec<i32> = (0..256)
        .map(|c| match c {
            8 | 16 => 100,
            9 | 15 => 250,
            10..=14 => 400,
            70..=71 => 150,
            _ => 0,
        })
        .collect();
    ElevationGrid::from_rows(8000, 0, 0, &vec![row; 64]).expect("relief rows are valid")
}

/// `hill_hq`: a 40 m hill with a headquarters observer of team 0 on top (x = 100 m, eye 1.7 m,
/// vision 3 km), and five regiments of team 1 (30 men, holding fire) at [`HILL_ENEMIES`], the
/// last behind a 15 m ridge. With `fight`, regiments 0 (team 0, at x = 300 m) and 1 (team 1,
/// x = 390 m) of 60 riflemen shoot each other and fill the valley with smoke; without it they
/// stand and hold. The enemy regiments are ids 2 to 6.
pub fn hill_hq(tables: &Tables, seed: u64, fight: bool) -> BattleState {
    let mut b = field(tables, seed, (256, 64), calm_weather(0, 0, 0));
    let a = b.spawn_regiment(line(0, 60, metres(300, 256)));
    let d = b.spawn_regiment(line(1, 60, metres(390, 256)));
    for &(x, y) in &HILL_ENEMIES {
        b.spawn_regiment(line(1, 30, metres(x, y)));
    }
    if fight {
        for (r, t) in [(a, d), (d, a)] {
            b.equip_regiment(r, "rifle_58", 0)
                .expect("scenario weapon exists");
            assert!(b.set_fire_mode(r, FireMode::AtWill { target: t }));
        }
    }
    let hq = Observer {
        id: HQ_ID,
        team: 0,
        pos: metres(100, 256),
        eye_mm: 1700,
        vision_mm: 3_000_000,
    };
    assert!(b.add_observer(hq));
    if let Some(env) = b.env.as_mut() {
        env.set_relief(hill_relief());
    }
    b
}

/// `env_hill_hq`: [`hill_hq`] with the fight, in calm weather.
pub fn env_hill_hq(tables: &Tables, seed: u64) -> BattleState {
    hill_hq(tables, seed, true)
}

/// `heat_march` of `men` men: a column marches 6 km east at quick time over open ground under
/// the Gettysburg weather from `start_minute`.
pub fn heat_march(tables: &Tables, seed: u64, start_minute: i64, men: usize) -> BattleState {
    let mut b = field(tables, seed, (1024, 64), gettysburg_weather(start_minute));
    let spec = RegimentSpec {
        team: 0,
        count: men,
        training: Training::Trained,
        formation: Formation::Column,
        anchor: metres(100, 256),
        heading: Angle::ZERO,
    };
    let id = b.spawn_regiment(spec);
    b.issue_order(
        id,
        Order::Move {
            to: metres(6100, 256),
            gait: Gait::Quick,
        },
    );
    b
}

/// `env_heat_march`: [`heat_march`] of 400 men from 14:00 on 2 July.
pub fn env_heat_march(tables: &Tables, seed: u64) -> BattleState {
    heat_march(tables, seed, DAY2_1400, 400)
}

/// `f0_large` (20,000 soldiers in firing lines) with the environment on in calm weather: the
/// load of the performance test T9-18.
pub fn f0_large_env(tables: &Tables, seed: u64) -> BattleState {
    let mut b = f0_large(tables, seed);
    switch_on(&mut b, (128, 400), calm_weather(0, 0, 0));
    b
}

/// The scenario called `name` (see [`NAMES`]).
pub fn by_name(name: &str, tables: &Tables, seed: u64) -> Option<BattleState> {
    match name {
        "env_smoke_line" => Some(env_smoke_line(tables, seed)),
        "env_hill_hq" => Some(env_hill_hq(tables, seed)),
        "env_heat_march" => Some(env_heat_march(tables, seed)),
        "smoke_line" => Some(smoke_line(tables, seed)),
        "crest" => Some(crest(tables, seed)),
        _ => None,
    }
}
