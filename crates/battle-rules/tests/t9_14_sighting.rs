//! T9-14 (SPEC-009 section 8): observers run once per period on their schedule, a hill observer
//! sees a valley regiment and a low one behind a ridge does not, smoke and the vision range
//! (clear visibility times light) hide a regiment, the sighting holds the sampled men, quality
//! and a centroid close to the truth, and the visibility oracle fits the command layer's trait.

mod t022_support;
mod t9_support;

use battle_rules::command::Visibility;
use battle_rules::{
    BattleState, ElevationGrid, EnvTable, Observer, Obstacle, ObstacleLayer, RegimentId,
    SightVisibility, SightWorld, SmokeGrid, Training, WeatherNow, WeatherTable,
};
use core_math::{Angle, Vec2Fx};
use t022_support::{field, line, metres};
use t9_support::env_table;

const HQ: u16 = 0x8000 + 12;

/// A one-key timeline with the given clear visibility (mm) and light (permille).
fn sky(clear_vis_mm: i64, light_pm: i64) -> WeatherTable {
    let text = format!(
        "schema = \"weather/1\"\nstart_minute = 0\nkey = [\n  {{ t_min = 0, temp_f10 = 700, \
         humidity_pm = 800, wind_x_mm_s = 0, wind_y_mm_s = 0, rain_pm = 0, \
         clear_vis_mm = {clear_vis_mm}, light_pm = {light_pm}, confidence = \"unverified\", \
         placeholder = true, calibration_target = \"CT-009-6\" }},\n]\n"
    );
    WeatherTable::from_toml_str(&text).unwrap()
}

fn hq(id: u16, x: i32, vision_mm: u32) -> Observer {
    Observer {
        id,
        team: 0,
        pos: metres(x, 200),
        eye_mm: 1700,
        vision_mm,
    }
}

/// 20 by 3 style relief on the 256 by 64 cell field: a 40 m plateau under columns 10 to 14 (the
/// hill, x 80 to 120 m) and a 12 m ridge at column 31 (x about 252 m).
fn relief() -> ElevationGrid {
    let row: Vec<i32> = (0..256)
        .map(|c| match c {
            10..=14 => 400,
            31 => 120,
            _ => 0,
        })
        .collect();
    let rows = vec![row; 64];
    ElevationGrid::from_rows(8000, 0, 0, &rows).unwrap()
}

/// A valley regiment of ten men of team 1 at x = 400 m and no team 0 regiment, so the only
/// observers of team 0 are the ones a test registers.
fn valley(vis: i64, light: i64, with_relief: bool) -> (BattleState, RegimentId) {
    let mut b = field(3);
    let r = b.spawn_regiment(line(
        1,
        10,
        Training::Trained,
        metres(400, 200),
        Angle::HALF,
    ));
    b.set_environment(env_table(), sky(vis, light));
    if with_relief {
        b.env.as_mut().unwrap().set_relief(relief());
    }
    (b, r)
}

fn run(b: &mut BattleState, ticks: u32) {
    for _ in 0..ticks {
        b.step();
    }
}

fn dense_bar(x0: u32, x1: u32) -> SmokeGrid {
    let mut g = SmokeGrid::new(8000, 256, 64, Vec2Fx::ZERO).unwrap();
    for y in 0..64usize {
        for x in (x0 / 8)..(x1 / 8) {
            g.density[y * 256 + x as usize] = 1_000_000;
        }
    }
    g
}

#[test]
fn an_observer_runs_exactly_once_per_period_on_its_schedule() {
    let (mut b, enemy) = valley(16_000_000, 1000, false);
    assert!(b.add_observer(hq(HQ, 100, 3_000_000)));
    assert!(b.sightings(0).is_empty());
    let mut seen = Vec::new();
    for _ in 0..100 {
        b.step();
        for s in b.sightings(0) {
            assert_eq!(s.enemy, enemy);
            if !seen.contains(&(s.observer, s.tick)) {
                seen.push((s.observer, s.tick));
            }
        }
    }
    // Five periods, one run each, on the ticks congruent to the id modulo 20.
    let phase = u64::from(HQ % 20);
    let ticks: Vec<u64> = seen.iter().map(|&(_, t)| t).collect();
    assert_eq!(
        ticks,
        [phase, phase + 20, phase + 40, phase + 60, phase + 80]
    );
    assert!(seen.iter().all(|&(o, _)| o == HQ));
}

#[test]
fn a_regiment_is_an_observer_at_its_anchor() {
    let mut b = field(4);
    let a = b.spawn_regiment(line(0, 6, Training::Trained, metres(100, 200), Angle::ZERO));
    let e = b.spawn_regiment(line(1, 6, Training::Trained, metres(300, 200), Angle::HALF));
    b.set_environment(env_table(), sky(16_000_000, 1000));
    run(&mut b, 40);
    let mine = b.sightings(0);
    assert_eq!(mine.len(), 1);
    assert_eq!((mine[0].observer, mine[0].enemy), (a.0, e));
    assert_eq!(mine[0].tick % 20, u64::from(a.0 % 20));
    let theirs = b.sightings(1);
    assert_eq!((theirs[0].observer, theirs[0].enemy), (e.0, a));
    // Nothing is learned about a friend.
    assert!(b.sightings(2).is_empty());
}

#[test]
fn a_hill_observer_sees_the_valley_and_a_low_one_behind_the_ridge_does_not() {
    let (mut b, enemy) = valley(16_000_000, 1000, true);
    assert!(b.add_observer(hq(HQ, 114, 3_000_000)));
    assert!(b.add_observer(hq(HQ + 1, 200, 3_000_000)));
    run(&mut b, 20);
    let log = b.sightings(0);
    assert_eq!(log.len(), 1);
    assert_eq!((log[0].observer, log[0].enemy), (HQ, enemy));
    assert_eq!(log[0].men_seen, 10);
}

#[test]
fn dense_smoke_hides_the_regiment() {
    let (mut b, _) = valley(16_000_000, 1000, true);
    assert!(b.add_observer(hq(HQ, 114, 3_000_000)));
    let mut clear = b.clone();
    run(&mut clear, 20);
    assert_eq!(clear.sightings(0).len(), 1, "the control sees it");
    b.smoke = dense_bar(200, 300);
    run(&mut b, 20);
    assert!(b.sightings(0).is_empty());
}

#[test]
fn the_range_is_the_vision_and_the_light_limited_visibility() {
    // 300 m away. Vision alone.
    let (mut b, _) = valley(16_000_000, 1000, false);
    assert!(b.add_observer(hq(HQ, 100, 250_000)));
    run(&mut b, 20);
    assert!(b.sightings(0).is_empty());
    let (mut b, _) = valley(16_000_000, 1000, false);
    assert!(b.add_observer(hq(HQ, 100, 350_000)));
    run(&mut b, 20);
    assert_eq!(b.sightings(0).len(), 1);
    // Light: clear_vis 1000 m, so 200 m at light 200 and 500 m at light 500.
    for (light, seen) in [(200, false), (500, true), (1000, true)] {
        let (mut b, _) = valley(1_000_000, light, false);
        assert!(b.add_observer(hq(HQ, 100, 3_000_000)));
        run(&mut b, 20);
        assert_eq!(!b.sightings(0).is_empty(), seen, "light {light}");
    }
}

#[test]
fn the_sighting_holds_men_quality_and_centroid() {
    let (mut b, _) = valley(16_000_000, 1000, false);
    assert!(b.add_observer(hq(HQ, 100, 1_000_000)));
    run(&mut b, 20);
    let s = b.sightings(0)[0];
    assert_eq!(s.men_seen, 10);
    // Clear air at about 300 m of a 1000 m range: about 700.
    assert!((690..=710).contains(&s.quality_pm), "{}", s.quality_pm);
    let (mut sx, mut sy, n) = (0i64, 0i64, 10i64);
    for id in &b.regiments[0].roster {
        let p = b.world.agents.pos(*id).unwrap();
        sx += p.x.raw();
        sy += p.y.raw();
    }
    let truth = Vec2Fx::new(
        core_math::Fx::from_raw(sx / n),
        core_math::Fx::from_raw(sy / n),
    );
    assert!((s.centroid - truth).length() < core_math::Fx::from_int(5));
}

#[test]
fn walls_hide_part_of_a_line_and_men_seen_follows() {
    let (mut b, _) = valley(16_000_000, 1000, false);
    // Spread the ten men 3 m apart from y = 190 to y = 217; a wall at x = 250 covers y < 203.
    for (k, id) in b.regiments[0].roster.clone().into_iter().enumerate() {
        b.world.agents.set_pos(id, metres(400, 190 + 3 * k as i32));
    }
    b.grid.rebuild(&b.world.agents);
    b.obstacles = ObstacleLayer {
        segments: vec![Obstacle {
            a: metres(250, 150),
            b: metres(250, 215),
            height_mm: 3000,
        }],
    };
    let mut hidden = b.clone();
    assert!(b.add_observer(hq(HQ, 100, 1_000_000)));
    // The wall spans the whole width of the line here: nobody is seen.
    run(&mut b, 20);
    assert!(b.sightings(0).is_empty());
    // Shorten the wall to y = 203 and the men north of it are seen.
    hidden.obstacles.segments[0].b = metres(250, 203);
    assert!(hidden.add_observer(hq(HQ, 100, 1_000_000)));
    run(&mut hidden, 20);
    let s = hidden.sightings(0)[0];
    assert!(s.men_seen > 0 && s.men_seen < 10, "{}", s.men_seen);
}

#[test]
fn a_new_sighting_replaces_the_old_one_for_the_same_enemy() {
    let (mut b, enemy) = valley(16_000_000, 1000, false);
    assert!(b.add_observer(hq(HQ, 100, 3_000_000)));
    run(&mut b, 12);
    let first = b.sightings(0)[0].tick;
    run(&mut b, 20);
    let log = b.sightings(0);
    assert_eq!(log.len(), 1);
    assert_eq!(log[0].enemy, enemy);
    assert_eq!(log[0].tick, first + 20);
    assert_eq!(log[0].staleness(b.world.tick), b.world.tick - first - 20);
}

#[test]
fn registration_rules_and_the_off_switch() {
    let (mut b, _) = valley(16_000_000, 1000, false);
    assert!(
        !b.add_observer(hq(5, 100, 1000)),
        "ids below 0x8000 belong to regiments"
    );
    assert!(b.add_observer(hq(HQ, 100, 3_000_000)));
    assert!(!b.add_observer(hq(HQ, 150, 3_000_000)), "duplicate id");
    // With the environment off nothing is observed.
    let mut off = b.clone();
    off.env = None;
    run(&mut off, 40);
    assert!(off.sightings(0).is_empty());
}

#[test]
fn the_oracle_adapter_reads_sight_for_the_command_layer() {
    let relief = ElevationGrid::flat();
    let obstacles = ObstacleLayer::default();
    let table: EnvTable = env_table();
    let clear = SmokeGrid::empty();
    let smoky = dense_bar(200, 300);
    let weather = WeatherNow {
        temp_f10: 700,
        humidity_pm: 800,
        wind_x_mm_s: 0,
        wind_y_mm_s: 0,
        rain_pm: 0,
        clear_vis_mm: 1_000_000,
        light_pm: 500,
    };
    fn make<'a>(
        parts: (
            &'a ElevationGrid,
            &'a ObstacleLayer,
            &'a EnvTable,
            &'a WeatherNow,
        ),
        smoke: &'a SmokeGrid,
    ) -> SightVisibility<'a> {
        let (relief, obstacles, table, weather) = parts;
        let world = SightWorld::new(relief, obstacles, smoke, &table.smoke, &table.sight);
        SightVisibility::new(world, table.sight.step_mm, 1700, 1020, weather)
    }
    let parts = (&relief, &obstacles, &table, &weather);
    let (a, t) = (metres(100, 200), metres(400, 200));
    let v = make(parts, &clear);
    assert!(v.visible(a, t));
    assert_eq!(v.transmit_pm(a, t), 1000);
    assert_eq!(v.range_mm(a, 3_000_000), 500_000);
    assert_eq!(v.range_mm(a, 300_000), 300_000);
    let v = make(parts, &smoky);
    assert!(!v.visible(a, t));
    assert_eq!(v.transmit_pm(a, t), 0);
}
