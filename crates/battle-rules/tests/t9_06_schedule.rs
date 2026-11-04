//! T9-06: the grid changes only on ticks 3 mod 4 apart from sources, and a reversed cell order
//! gives the same grid. Also the SPEC-005 amendment A8 source: with the environment on, the fire
//! phase adds `powder_mg * yield_pm / 1000` per shot and the per-tick decay is replaced.

mod t022_support;
mod t93_support;

use battle_rules::SmokeGrid;
use core_math::Vec2Fx;
use t022_support::{duel, metres, no_misfire};
use t93_support::{env_table, grid, params, put, weather, Lcg};

#[test]
fn the_grid_changes_only_on_ticks_that_are_three_mod_four() {
    let wx = weather(&[(0, 700, 500, 2000, 1000, 0)]);
    let (mut b, _, _) = duel(
        4,
        "smoothbore_69",
        0,
        battle_rules::Training::Trained,
        4,
        40,
    );
    b.smoke = SmokeGrid::new(8000, 40, 40, metres(0, 0)).unwrap();
    b.set_environment(env_table(), wx);
    for (x, y, d) in [(12, 20, 90_000), (13, 20, 40_000), (30, 5, 7)] {
        put(&mut b.smoke, x, y, d);
    }
    let mut changes = Vec::new();
    for _ in 0..40 {
        let before = b.smoke.clone();
        let tick = b.world.tick;
        b.step_smoke();
        if b.smoke != before {
            changes.push(tick);
        }
        b.world.advance_tick();
    }
    assert_eq!(changes, (0..10).map(|k| 4 * k + 3).collect::<Vec<u64>>());
}

#[test]
fn reversed_cell_order_gives_an_identical_grid() {
    let p = params(&[]);
    let mut rng = Lcg(99);
    let mut a = grid(17, 13);
    for _ in 0..40 {
        let (x, y) = (rng.next(17) as u32, rng.next(13) as u32);
        put(&mut a, x, y, 1 + rng.next(400_000) as u32);
    }
    let mut b = a.clone();
    let retain = |pos: Vec2Fx| {
        if pos.x.to_int_floor() % 3 == 0 {
            300
        } else {
            0
        }
    };
    for k in 0..100 {
        let (wx, wy) = (rng.next(7001) as i64 - 3500, rng.next(7001) as i64 - 3500);
        let rain = (k % 5) * 200;
        a.update_with(&p, wx, wy, rain, &retain, false, false);
        b.update_with(&p, wx, wy, rain, &retain, true, false);
        assert_eq!(a, b, "update {k}");
    }
}

#[test]
fn with_the_environment_on_a_shot_adds_the_yield_and_no_per_tick_decay_runs() {
    let (mut b, _, _) = duel(
        4,
        "smoothbore_69",
        0,
        battle_rules::Training::Trained,
        100,
        40,
    );
    b.smoke = SmokeGrid::new(1_000_000, 2, 1, Vec2Fx::ZERO).unwrap();
    b.set_environment(env_table(), weather(&[(0, 700, 500, 0, 0, 0)]));
    no_misfire(&mut b);
    for f in b.fire.iter_mut().take(100) {
        f.rounds = 1;
    }
    let powder = u64::from(b.tables.weapons.get("smoothbore_69").unwrap().powder_mg);
    while b.counters().shots_fired < 100 {
        b.step_fire();
        b.world.advance_tick();
        assert!(b.world.tick < 2000);
    }
    let want = 100 * (powder * 550 / 1000);
    assert_eq!(u64::from(b.smoke.cell_density(metres(100, 200))), want);
    assert_eq!(b.smoke.injected, want);
    // Tick 0 mod 4 is not an update tick: the density stays.
    b.world.tick = 8;
    b.step_smoke();
    assert_eq!(u64::from(b.smoke.cell_density(metres(100, 200))), want);
    b.world.tick = 11;
    b.step_smoke();
    assert!(u64::from(b.smoke.cell_density(metres(100, 200))) < want);
    assert_eq!(
        b.smoke.injected,
        b.smoke.total() + b.smoke.lost_edge + b.smoke.lost_decay + b.smoke.lost_cap
    );
}

#[test]
fn with_the_environment_off_the_per_tick_decay_is_unchanged() {
    let (mut b, _, _) = duel(
        4,
        "smoothbore_69",
        0,
        battle_rules::Training::Trained,
        4,
        40,
    );
    b.smoke = SmokeGrid::new(1_000_000, 2, 1, Vec2Fx::ZERO).unwrap();
    b.smoke.add(metres(100, 200), 1000);
    b.step_smoke();
    assert_eq!(b.smoke.cell_density(metres(100, 200)), 995);
    assert_eq!(b.smoke.injected, 0);
}

#[test]
fn woods_under_a_cell_hold_its_smoke_back_in_the_battle_update() {
    use battle_rules::TerrainGrid;
    let run = |class: char| {
        let (mut b, _, _) = duel(
            4,
            "smoothbore_69",
            0,
            battle_rules::Training::Trained,
            4,
            40,
        );
        b.terrain = TerrainGrid::from_rows(
            8000,
            0,
            0,
            &[('.', 0), ('w', 3)],
            &vec![class.to_string().repeat(8); 8],
            &b.tables.terrain,
        )
        .unwrap();
        b.smoke = SmokeGrid::new(8000, 8, 8, metres(0, 0)).unwrap();
        b.set_environment(env_table(), weather(&[(0, 700, 500, 0, 0, 0)]));
        put(&mut b.smoke, 4, 4, 100_000);
        b.world.tick = 3;
        b.step_smoke();
        b.smoke.density[4 * 8 + 4]
    };
    // Open ground sends 4 * 1% to the neighbours; woods keep 40% of that share.
    assert_eq!(
        run('.'),
        100_000 - 4_000 - (96_000 * 4000_u32).div_ceil(1_000_000)
    );
    assert!(run('w') > run('.'));
}
