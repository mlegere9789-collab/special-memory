//! T5-17: the smoke source (SPEC-005 section 8).

mod t022_support;

use battle_rules::smoke::{shot_units, SmokeGrid};
use battle_rules::{FireMode, Training};
use core_math::{Angle, Fx, Vec2Fx};
use t022_support::{duel, line, metres, no_misfire};

fn decay_once(d: u64) -> u64 {
    d - (d * 5000).div_ceil(1_000_000)
}

#[test]
fn a_hundred_shots_give_three_hundred_units_and_decay_follows_the_formula_to_zero() {
    // One cell a kilometre wide holds every muzzle.
    let (mut b, _, _) = duel(4, "smoothbore_69", 0, Training::Trained, 100, 40);
    no_misfire(&mut b);
    b.smoke = SmokeGrid::new(1_000_000, 2, 1, Vec2Fx::ZERO).unwrap();
    for f in b.fire.iter_mut().take(100) {
        f.rounds = 1;
    }
    while b.counters().shots_fired < 100 {
        b.step_fire();
        b.world.advance_tick();
        assert!(b.world.tick < 2000);
    }
    let cell = metres(100, 200);
    assert_eq!(b.smoke.cell_density(cell), 300);

    let mut d = 300u64;
    let mut ticks = 0;
    while d > 0 {
        b.step_smoke();
        d = decay_once(d);
        ticks += 1;
        assert_eq!(u64::from(b.smoke.cell_density(cell)), d, "tick {ticks}");
    }
    assert!(ticks > 100 && ticks < 2000);
    b.step_smoke();
    assert_eq!(b.smoke.cell_density(cell), 0);
}

#[test]
fn shot_units_use_integer_division_and_a_misfire_a_quarter() {
    assert_eq!(shot_units(3900, false), 3);
    assert_eq!(shot_units(907_000, false), 907);
    assert_eq!(shot_units(907_000, true), 226);
    assert_eq!(shot_units(999, false), 0);
}

#[test]
fn smoke_lands_in_the_cell_of_the_muzzle_and_clamps_at_the_edge() {
    let mut g = SmokeGrid::new(8000, 4, 4, metres(100, 100)).unwrap();
    g.add(metres(103, 105), 7);
    g.add(metres(110, 108), 5);
    g.add(metres(-500, 5000), 2); // outside: nearest edge cell
    assert_eq!(g.cell_density(metres(100, 100)), 7);
    assert_eq!(g.cell_density(metres(108, 108)), 5);
    assert_eq!(g.cell_density(metres(100, 124)), 2);
    assert!(SmokeGrid::new(0, 1, 1, Vec2Fx::ZERO).is_none());
    assert_eq!(SmokeGrid::empty().cell_density(Vec2Fx::ZERO), 0);
    let _ = (Fx::ONE, Angle::ZERO, FireMode::HoldFire, line);
}
