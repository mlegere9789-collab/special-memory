//! T9-10: the targeting gate (SPEC-009 section 6, SPEC-005 amendment A9): a dense bar of smoke
//! withholds fire without touching the soldier streams, retries pick `(j + k) % n` in order,
//! clearing the smoke resumes fire, and in clear air the gate changes nothing.

mod t022_support;
mod t9_support;

use battle_rules::aim::{aim, aim_in_smoke, draw_shot, AimInput};
use battle_rules::{BattleState, Obstacle, RegimentId, SmokeGrid, Training};
use core_math::{Angle, Fx, Rng, Vec2Fx};
use t022_support::{duel, field, line, metres};
use t9_support::{env_table, env_table_with, weather};

fn neutral() -> battle_rules::WeatherTable {
    weather(&[(0, 700, 800, 0, 0, 0)])
}

/// One tick of the fire phase and the tick counter only, so that smoke put in by a test stays.
fn fire_tick(b: &mut BattleState) {
    b.step_fire();
    b.world.advance_tick();
}

fn rng_states(b: &BattleState) -> Vec<[u64; 4]> {
    b.fire.iter().map(|f| f.rng).collect()
}

/// A smoke grid over the whole field (2048 m by 512 m) with a bar of `density` between
/// `x0` and `x1` metres across the full width.
fn bar(x0: u32, x1: u32, density: u32) -> SmokeGrid {
    let mut g = SmokeGrid::new(8000, 256, 64, Vec2Fx::ZERO).unwrap();
    for y in 0..64usize {
        for x in (x0 / 8)..(x1 / 8) {
            g.density[y * 256 + x as usize] = density;
        }
    }
    g
}

#[test]
fn a_dense_bar_withholds_fire_and_leaves_the_streams_alone() {
    let (mut b, shooters, _) = duel(5, "rifle_58", 0, Training::Trained, 10, 120);
    b.set_environment(env_table(), neutral());
    b.smoke = bar(150, 190, 1_000_000);
    let before = rng_states(&b);
    for _ in 0..800 {
        fire_tick(&mut b);
    }
    let n = b.counters();
    assert!(n.shots_withheld > 0, "{n:?}");
    assert!(
        n.sight_queries > 0 && n.sight_blocked == n.sight_queries,
        "{n:?}"
    );
    assert_eq!((n.shots_fired, n.projectiles_spawned), (0, 0));
    assert!(b.pool.is_empty());
    assert_eq!(rng_states(&b), before, "no stream word may be drawn");
    let reg = &b.regiments[usize::from(shooters.0)];
    for id in &reg.roster {
        assert!(b.loco[id.index as usize].blind, "soldier {}", id.index);
    }
    // Held fire changes no loading state: every shooter is still Ready with its full rounds.
    let w = b
        .tables
        .weapons
        .rows()
        .iter()
        .find(|w| w.id == "rifle_58")
        .unwrap();
    for id in &reg.roster {
        let f = &b.fire[id.index as usize];
        assert_eq!(usize::from(f.step), w.load_steps.len());
        assert_eq!(f.fouling, 0);
    }
}

#[test]
fn clearing_the_smoke_resumes_fire() {
    let (mut b, _, _) = duel(5, "rifle_58", 0, Training::Trained, 10, 120);
    b.set_environment(env_table(), neutral());
    b.smoke = bar(150, 190, 1_000_000);
    for _ in 0..400 {
        fire_tick(&mut b);
    }
    assert_eq!(b.counters().shots_fired, 0);
    b.smoke = SmokeGrid::new(8000, 256, 64, Vec2Fx::ZERO).unwrap();
    for _ in 0..400 {
        fire_tick(&mut b);
    }
    assert!(b.counters().shots_fired >= 10, "{:?}", b.counters());
}

/// One shooter (`j = 0`) facing four targets in a column 100 m east, strung out north, behind
/// a tall wall that hides the first two only. Returns the battle with the first shot spawned.
fn retry_field(edits: &[(&str, &str)]) -> BattleState {
    let mut b = field(9);
    let s = b.spawn_regiment(line(0, 1, Training::Trained, metres(100, 200), Angle::ZERO));
    let t = b.spawn_regiment(line(1, 4, Training::Trained, metres(200, 200), Angle::HALF));
    for (k, y) in [200, 250, 300, 350].into_iter().enumerate() {
        let id = b.regiments[usize::from(t.0)].roster[k];
        b.world.agents.set_pos(id, metres(200, y));
    }
    b.grid.rebuild(&b.world.agents);
    b.equip_regiment(s, "rifle_58", 0).unwrap();
    assert!(b.set_fire_mode(s, battle_rules::FireMode::AtWill { target: t }));
    b.obstacles.segments.push(Obstacle {
        a: metres(150, 190),
        b: metres(150, 240),
        height_mm: 5000,
    });
    b.set_environment(env_table_with(edits), neutral());
    b.weather.wet_pm = 0;
    b
}

/// Direction `vy / vx` of the first projectile in permille, after at most 400 ticks.
fn first_slope(b: &mut BattleState) -> Option<i64> {
    for _ in 0..1500 {
        fire_tick(b);
        if let Some(p) = b.pool.first() {
            return Some(p.vel.y.raw() * 1000 / p.vel.x.raw());
        }
    }
    None
}

#[test]
fn retries_take_the_next_roster_entries_in_order() {
    // Entries 0 and 1 are behind the wall; entry 2 (100 m north-east) is the first visible.
    let mut b = retry_field(&[]);
    let slope = first_slope(&mut b).expect("a shot");
    assert!(
        (900..=1100).contains(&slope),
        "slope {slope} is not toward entry 2"
    );
    let n = b.counters();
    // The shot tick tested entries 0, 1 and 2, in that order, and stopped at the first visible.
    assert_eq!((n.sight_queries, n.sight_blocked), (3, 2), "{n:?}");
    assert_eq!(n.shots_withheld, 0);
}

#[test]
fn the_retry_count_limits_the_search() {
    // retries = 1 reaches entries 0 and 1 only: both hidden, so the soldier holds fire.
    let mut b = retry_field(&[("retries = 4", "retries = 1")]);
    assert_eq!(first_slope(&mut b), None);
    assert!(b.counters().shots_withheld > 0);
    // retries = 2 reaches entry 2.
    let mut b = retry_field(&[("retries = 4", "retries = 2")]);
    assert!(first_slope(&mut b).is_some());
}

#[test]
fn a_target_beyond_extended_range_is_skipped_without_a_query() {
    let (mut b, _, _) = duel(5, "rifle_58", 0, Training::Trained, 10, 1500);
    b.set_environment(env_table(), neutral());
    for _ in 0..300 {
        fire_tick(&mut b);
    }
    let n = b.counters();
    assert_eq!(
        (n.sight_queries, n.shots_fired, n.shots_withheld),
        (0, 0, 0)
    );
    assert!(b.loco.iter().all(|l| !l.blind));
}

/// The whole state that a clear-air gate must not change, as text.
fn trace(b: &BattleState) -> String {
    let c = b.counters();
    let pool: Vec<(u64, i32)> = b.pool.iter().map(|p| (p.seed, p.fuse)).collect();
    format!(
        "{:?} {} {} {} {} {pool:?}",
        rng_states(b),
        b.pool.len(),
        c.shots_fired,
        c.misfires,
        b.casualties.len(),
    )
}

#[test]
fn in_clear_air_the_gate_changes_nothing() {
    // T5-14 and T5-18 with the environment on and nothing in the air: the same shots as with
    // the environment off, soldier for soldier.
    let run = |on: bool| {
        let (mut b, _, _) = duel(21, "rifle_58", 0, Training::Trained, 60, 120);
        if on {
            b.set_environment(env_table(), neutral());
        }
        b.weather.wet_pm = 0;
        for _ in 0..1500 {
            b.step_fire();
            b.step_flight();
            b.step_casualties();
            b.world.advance_tick();
        }
        (trace(&b), b.counters())
    };
    let (off, c_off) = run(false);
    let (on, c_on) = run(true);
    assert_eq!(off, on);
    assert!(c_off.shots_fired > 100);
    assert_eq!(c_off.shots_fired, c_on.shots_fired);
    assert_eq!(c_on.shots_withheld, 0);
    assert_eq!(c_on.projectiles_spawned, c_on.shots_fired - c_on.misfires);
    assert_eq!(c_on.unit_level_rolls, 0);
    assert!(c_on.sight_queries >= c_on.shots_fired);
}

fn aim_input<'a>(b: &'a BattleState) -> AimInput<'a> {
    let w = b
        .tables
        .weapons
        .rows()
        .iter()
        .find(|w| w.id == "rifle_58")
        .unwrap();
    let ai = b.tables.ammo.index_of(&w.ammo[0].id).unwrap();
    let ammo = &b.tables.ammo.rows()[ai];
    let table = Box::leak(Box::new(battle_rules::RangeTable::build(
        ammo,
        b.tables.ammo.constants().gravity,
        w.reach_mm,
    )));
    AimInput {
        weapon: w,
        ammo,
        table,
        training: Training::Trained,
        fatigue_ppm: 0,
        smoke_pm: 0,
        range: Fx::from_int(150),
        bearing: Angle::ZERO,
    }
}

#[test]
fn the_range_error_multiplier_scales_the_error_term_only() {
    let b = field(1);
    let input = aim_input(&b);
    let mut rng = Rng::from_state([1, 2, 3, 4]);
    let mut d = draw_shot(&mut rng);
    d.range = Fx::from_int(1);
    assert_eq!(aim_in_smoke(&input, &d, 1000), aim(&input, &d));
    // A multiplier of 2000 at g = 1 estimates like a multiplier of 1000 at g = 2.
    let mut d2 = d;
    d2.range = Fx::from_int(2);
    assert_eq!(aim_in_smoke(&input, &d, 2000), aim(&input, &d2));
    assert_ne!(aim_in_smoke(&input, &d, 2000), aim(&input, &d));
}

#[test]
fn smoke_at_the_shooter_reads_the_table_reference() {
    // Equal concentration through different `ref_units` and densities gives the same shot when
    // the extinction and the range gain are switched off.
    let quiet = [
        ("ext_k = 20", "ext_k = 1"),
        ("range_smoke_gain_pm = 1500", "range_smoke_gain_pm = 0"),
    ];
    let shot = |ref_units: &str, density: u32| {
        let mut e = quiet.to_vec();
        e.push(("ref_units = 20000", ref_units));
        let mut b = field(3);
        let s = b.spawn_regiment(line(0, 1, Training::Trained, metres(100, 200), Angle::ZERO));
        let t = b.spawn_regiment(line(1, 4, Training::Trained, metres(220, 200), Angle::HALF));
        b.equip_regiment(s, "rifle_58", 0).unwrap();
        assert!(b.set_fire_mode(s, battle_rules::FireMode::AtWill { target: t }));
        b.set_environment(env_table_with(&e), neutral());
        b.weather.wet_pm = 0;
        let mut g = SmokeGrid::new(8000, 256, 64, Vec2Fx::ZERO).unwrap();
        let pos = b.world.agents.pos(b.regiments[0].roster[0]).unwrap();
        let i = g.cell_index(pos).unwrap();
        g.density[i] = density;
        b.smoke = g;
        for _ in 0..1500 {
            fire_tick(&mut b);
            if let Some(p) = b.pool.first() {
                return (p.vel, p.vz);
            }
        }
        panic!("no shot");
    };
    assert_eq!(
        shot("ref_units = 20000", 10_000),
        shot("ref_units = 10000", 5_000)
    );
    assert_ne!(
        shot("ref_units = 20000", 10_000),
        shot("ref_units = 20000", 0)
    );
}

#[test]
fn env_inputs_are_none_without_the_environment() {
    let b = field(1);
    assert!(b.env_inputs(RegimentId(0)).is_none());
}
