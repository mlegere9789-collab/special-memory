//! T4-06: per tick `|vel_new - vel_old| <= accel / 20` plus one ulp for every soldier over
//! `deploy_pair` (SPEC-004 section 13, snap rule of Amendment A2).

mod t019_support;

use battle_rules::scenarios;
use core_math::{Fx, Vec2Fx};
use t019_support::{mm, tables};

#[test]
fn velocity_changes_stay_within_the_acceleration_limit() {
    let t = tables();
    let limit = mm(i64::from(t.locomotion.globals().accel_mm_s2)) / Fx::from_int(20) + Fx::EPSILON;
    let mut b = scenarios::deploy_pair(&t, 2);
    let mut worst = Fx::ZERO;
    for _ in 0..2000 {
        let before: Vec<Vec2Fx> = b
            .world
            .agents
            .iter_alive()
            .map(|i| b.world.agents.vel(i).unwrap())
            .collect();
        b.step_movement();
        for (id, old) in b.world.agents.iter_alive().zip(before) {
            worst = worst.max((b.world.agents.vel(id).unwrap() - old).length());
        }
    }
    println!("largest velocity change {worst:?}, limit {limit:?}");
    assert!(worst > Fx::ZERO, "nobody moved");
    assert!(worst <= limit, "{worst:?} exceeds {limit:?}");
}
