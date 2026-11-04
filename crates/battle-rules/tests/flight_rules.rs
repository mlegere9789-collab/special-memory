//! Flight rules of SPEC-005 section 2 (bursts excluded): ground contact, ricochet, fuse, reach,
//! minimum speed, and a literal regression of one drag flight.

use battle_rules::flight::{advance_projectile, DT, SUBSTEPS_PER_TICK};
use battle_rules::{
    fx_ratio, step_projectile, AmmoTable, Ballistics, Flight, FlightEnv, Projectile, Vec3Fx,
};
use core_math::{Angle, Fx, Vec2Fx};

const AMMO: &str = include_str!("../../../data/rules/ammo.toml");

fn env(ground_pm: u32) -> FlightEnv {
    FlightEnv {
        gravity: fx_ratio(9_806_650, 1_000_000),
        wind: Vec2Fx::ZERO,
        ground_pm,
    }
}

fn projectile(pos_x: i32, z: Fx, vx: i32, vz: Fx) -> Projectile {
    Projectile {
        seq: 1,
        owner: 2,
        unit: 3,
        team: 1,
        ammo: 0,
        pos: Vec2Fx::new(Fx::from_int(pos_x), Fx::ZERO),
        z,
        vel: Vec2Fx::new(Fx::from_int(vx), Fx::ZERO),
        vz,
        travelled_mm: 0,
        bounces: 0,
        hits: 0,
        fuse: -1,
        seed: 9,
    }
}

fn shot(restitution_pm: u32) -> Ballistics {
    Ballistics {
        drag_k: Fx::ZERO,
        restitution_pm: Some(restitution_pm),
    }
}

fn ball() -> Ballistics {
    Ballistics {
        drag_k: Fx::ZERO,
        restitution_pm: None,
    }
}

#[test]
fn vec3_operations() {
    let a = Vec3Fx::new(Fx::from_int(3), Fx::from_int(4), Fx::from_int(12));
    assert_eq!(a.length_sq(), Fx::from_int(169));
    assert_eq!(a.length(), Fx::from_int(13));
    assert_eq!(
        a.dot(Vec3Fx::new(Fx::ONE, Fx::ONE, Fx::ONE)),
        Fx::from_int(19)
    );
    assert_eq!(a.scale(Fx::HALF).z, Fx::from_int(6));
    assert_eq!(a + a, a.scale(Fx::from_int(2)));
    assert_eq!(a - a, Vec3Fx::ZERO);
    assert_eq!((-a).x, Fx::from_int(-3));
}

#[test]
fn non_shot_ends_at_the_ground_contact_point() {
    let mut p = projectile(0, Fx::from_int(1), 100, Fx::ZERO);
    let mut ticks = 0;
    let end = loop {
        ticks += 1;
        let f = step_projectile(&mut p, &ball(), u32::MAX, &env(1000));
        if !f.is_alive() {
            break f;
        }
    };
    assert_eq!(end, Flight::Ground);
    assert_eq!(p.z, Fx::ZERO);
    // Fall of 1 m takes 0.452 s = 9.05 ticks.
    assert_eq!(ticks, 10);
    assert!(
        p.pos.x > Fx::from_int(40) && p.pos.x < Fx::from_int(50),
        "{:?}",
        p.pos.x
    );
    assert!(p.vz < Fx::ZERO);
}

#[test]
fn shot_ricochets_with_restitution_ground_factor_and_horizontal_loss() {
    for (ground, factor) in [(1000u32, 400i64), (500, 200)] {
        let mut p = projectile(0, Fx::HALF, 100, Fx::from_int(-50));
        let f = step_projectile(&mut p, &shot(400), u32::MAX, &env(ground));
        assert_eq!(f, Flight::Ricochet);
        assert!(f.is_alive());
        assert_eq!(p.bounces, 1);
        assert_eq!(p.z, Fx::ZERO);
        // Speed down is a little over 50 m/s at contact; up is `factor` permille of it.
        let want = fx_ratio(factor, 1000) * Fx::from_int(50);
        assert!(
            p.vz > want && p.vz < want + Fx::from_int(3),
            "{ground}: {:?}",
            p.vz
        );
        assert!(p.vel.x > Fx::from_int(79) && p.vel.x < Fx::from_int(81));
        // Flight for the tick stopped at the contact: well under the full 10 sub-steps.
        assert!(p.pos.x < Fx::from_int(5), "{:?}", p.pos.x);
    }
}

#[test]
fn shot_ends_after_three_bounces_or_below_30_ms() {
    let mut p = projectile(0, Fx::ZERO, 100, Fx::from_int(-50));
    let mut outcomes = Vec::new();
    for _ in 0..5000 {
        let f = step_projectile(&mut p, &shot(900), u32::MAX, &env(1000));
        outcomes.push(f);
        if !f.is_alive() {
            break;
        }
    }
    let bounces = outcomes.iter().filter(|f| **f == Flight::Ricochet).count();
    assert_eq!(bounces, 3);
    assert_eq!(*outcomes.last().unwrap(), Flight::Ground);
    assert_eq!(p.bounces, 3);

    // Below 30 m/s at contact the shot does not bounce.
    let mut slow = projectile(0, fx_ratio(1, 100), 20, Fx::from_int(-10));
    let f = step_projectile(&mut slow, &shot(900), u32::MAX, &env(1000));
    assert_eq!(f, Flight::Ground);
    assert_eq!(slow.bounces, 0);
}

#[test]
fn fuse_counts_sub_steps_and_expires() {
    let mut p = projectile(0, Fx::from_int(500), 200, Fx::ZERO);
    p.fuse = 7;
    let (f, n) = advance_projectile(&mut p, &ball(), u32::MAX, &env(1000), SUBSTEPS_PER_TICK);
    assert_eq!(f, Flight::FuseExpired);
    assert_eq!(n, Fx::from_int(7));
    assert_eq!(p.fuse, 0);
    // Seven sub-steps of 200 m/s.
    let want = Fx::from_int(200) * DT * Fx::from_int(7);
    assert!((p.pos.x - want).abs() < Fx::from_raw(1 << 12));
    // A fuse of -1 is never counted.
    let mut q = projectile(0, Fx::from_int(500), 200, Fx::ZERO);
    assert_eq!(
        step_projectile(&mut q, &ball(), u32::MAX, &env(1000)),
        Flight::Flying
    );
    assert_eq!(q.fuse, -1);
}

#[test]
fn reach_and_minimum_speed_end_the_flight() {
    let mut p = projectile(0, Fx::from_int(500), 200, Fx::ZERO);
    let f = step_projectile(&mut p, &ball(), 1000, &env(1000));
    assert_eq!(f, Flight::ReachLimit);
    assert!(
        p.travelled_mm >= 1000 && p.travelled_mm < 1000 + 1100,
        "{}",
        p.travelled_mm
    );

    let mut s = projectile(0, Fx::from_int(500), 39, Fx::ZERO);
    assert_eq!(
        step_projectile(&mut s, &ball(), u32::MAX, &env(1000)),
        Flight::TooSlow
    );
    let mut ok = projectile(0, Fx::from_int(500), 41, Fx::ZERO);
    assert_eq!(
        step_projectile(&mut ok, &ball(), u32::MAX, &env(1000)),
        Flight::Flying
    );
}

#[test]
fn drag_slows_and_wind_pushes() {
    let table = AmmoTable::from_toml_str(AMMO).unwrap();
    let body = Ballistics::of_ammo(table.get("ball_69").unwrap());
    let calm = env(1000);
    let mut windy = calm;
    windy.wind = Vec2Fx::new(Fx::ZERO, Fx::from_int(10));
    let (mut a, mut b) = (
        projectile(0, Fx::from_int(500), 300, Fx::ZERO),
        projectile(0, Fx::from_int(500), 300, Fx::ZERO),
    );
    step_projectile(&mut a, &body, u32::MAX, &calm);
    step_projectile(&mut b, &body, u32::MAX, &windy);
    assert!(a.vel.x < Fx::from_int(300));
    assert_eq!(a.vel.y, Fx::ZERO);
    assert!(b.vel.y > Fx::ZERO && b.pos.y > Fx::ZERO);
}

/// Literal regression of a complete drag flight with cross wind: a failure means the
/// integrator's arithmetic changed.
#[test]
fn drag_flight_literal() {
    let table = AmmoTable::from_toml_str(AMMO).unwrap();
    let row = table.get("minie_58").unwrap();
    let body = Ballistics {
        restitution_pm: None,
        ..Ballistics::of_ammo(row)
    };
    let mut e = env(1000);
    e.wind = Vec2Fx::new(Fx::ZERO, Fx::from_int(5));
    let speed = fx_ratio(i64::from(row.muzzle_mm_s), 1000);
    let elev = Angle::from_degrees_int(5);
    let mut p = projectile(0, fx_ratio(1200, 1000), 0, Fx::ZERO);
    p.vel = Vec2Fx::new(elev.cos() * speed, Fx::ZERO);
    p.vz = elev.sin() * speed;
    let mut ticks = 0;
    while step_projectile(&mut p, &body, u32::MAX, &e).is_alive() {
        ticks += 1;
    }
    assert_eq!(
        (
            ticks,
            p.pos.x.raw(),
            p.pos.y.raw(),
            p.vel.x.raw(),
            p.vz.raw(),
            p.travelled_mm
        ),
        LITERAL
    );
}

const LITERAL: (i32, i64, i64, i64, i64, u32) = (
    86,
    3_469_319_858_768,
    33_099_666_079,
    545_121_011_587,
    -83_798_364_019,
    809_683,
);

/// Micro-benchmark for hypothesis H5: 1,250 projectiles in flight, ticks per second of wall
/// time. Run with `cargo test --release -p battle-rules --test flight_rules -- --ignored --nocapture`.
#[test]
#[ignore]
fn bench_pool_of_1250() {
    let table = AmmoTable::from_toml_str(AMMO).unwrap();
    let body = Ballistics::of_ammo(table.get("minie_58").unwrap());
    let e = env(1000);
    let mut pool: Vec<Projectile> = (0..1250)
        .map(|i| projectile(0, Fx::from_int(1000 + i), 290, Fx::ZERO))
        .collect();
    let start = std::time::Instant::now();
    for _ in 0..100 {
        for p in pool.iter_mut() {
            step_projectile(p, &body, u32::MAX, &e);
        }
    }
    let per_tick = start.elapsed().as_secs_f64() * 10.0;
    println!("1250 projectiles: {per_tick:.3} ms per tick");
}
