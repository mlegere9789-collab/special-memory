//! T5-18: the F0 rule in the counters (SPEC-005 section 7).

mod t023_support;

use battle_rules::{FireMode, Training};
use core_math::Angle;
use t023_support::{field_of, gun_duel, line_spec, m};

#[test]
fn every_shot_is_a_projectile_and_no_unit_level_roll_happens() {
    let mut b = field_of(1, '.');
    let a = b.spawn_regiment(line_spec(0, 100, m(100, 200), Angle::ZERO));
    let c = b.spawn_regiment(line_spec(1, 100, m(190, 200), Angle::HALF));
    for (r, o) in [(a, c), (c, a)] {
        b.equip_regiment(r, "rifle_58", 0).unwrap();
        assert!(b.set_fire_mode(r, FireMode::AtWill { target: o }));
    }
    for _ in 0..1500 {
        b.step();
    }
    let n = b.counters();
    assert!(n.shots_fired > 100, "{n:?}");
    assert!(n.misfires > 0);
    assert_eq!(n.projectiles_spawned, n.shots_fired - n.misfires);
    assert_eq!(n.unit_level_rolls, 0);
}

#[test]
fn bursts_add_their_children_to_the_spawn_count() {
    let (mut b, _, _) = gun_duel(3, Training::Trained);
    for _ in 0..1600 {
        b.step();
    }
    let n = b.counters();
    assert_eq!(n.shots_fired, 1);
    assert_eq!(n.misfires, 0);
    assert_eq!(n.projectiles_spawned, 1 + 27);
    assert_eq!(n.unit_level_rolls, 0);
}
