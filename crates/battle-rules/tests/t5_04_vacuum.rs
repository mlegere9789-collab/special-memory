//! T5-04: with no drag and no wind the range at 1, 5 and 10 degrees equals the closed form
//! `v^2 sin(2 theta) / g` within 0.2 percent (SPEC-005 sections 2 and 11).

use battle_rules::flight::{advance_projectile, SUBSTEPS_PER_TICK};
use battle_rules::{fx_ratio, Ballistics, Flight, FlightEnv, Projectile};
use core_math::{Angle, Fx, Vec2Fx};

const G_UM_S2: i64 = 9_806_650;

fn env() -> FlightEnv {
    FlightEnv {
        gravity: fx_ratio(G_UM_S2, 1_000_000),
        wind: Vec2Fx::ZERO,
        ground_pm: 1000,
    }
}

fn launch(speed_mm_s: i64, elev: Angle, z: Fx) -> Projectile {
    let speed = fx_ratio(speed_mm_s, 1000);
    Projectile {
        seq: 0,
        owner: 0,
        unit: 0,
        team: 0,
        ammo: 0,
        pos: Vec2Fx::ZERO,
        z,
        vel: Vec2Fx::new(elev.cos() * speed, Fx::ZERO),
        vz: elev.sin() * speed,
        travelled_mm: 0,
        bounces: 0,
        hits: 0,
        fuse: -1,
        seed: 0,
    }
}

/// Flies until the ground and returns the horizontal range in metres as f64.
fn range_m(mut p: Projectile, body: &Ballistics) -> f64 {
    for _ in 0..100_000 {
        let (f, _) = advance_projectile(&mut p, body, u32::MAX, &env(), SUBSTEPS_PER_TICK);
        match f {
            Flight::Flying => {}
            Flight::Ground => return p.pos.x.raw() as f64 / 4_294_967_296.0,
            other => panic!("unexpected end {other:?}"),
        }
    }
    panic!("never landed");
}

#[test]
fn range_matches_closed_form_at_three_elevations() {
    let body = Ballistics {
        drag_k: Fx::ZERO,
        restitution_pm: None,
    };
    let g = G_UM_S2 as f64 / 1.0e6;
    for speed_mm_s in [230_000i64, 300_000, 452_628] {
        let v = speed_mm_s as f64 / 1000.0;
        for deg in [1i32, 5, 10] {
            let want = v * v * (2.0 * f64::from(deg).to_radians()).sin() / g;
            let got = range_m(
                launch(speed_mm_s, Angle::from_degrees_int(deg), Fx::ZERO),
                &body,
            );
            let rel = ((got - want) / want).abs();
            assert!(
                rel < 0.002,
                "v {v} elev {deg}: got {got}, want {want}, rel {rel}"
            );
        }
    }
}

#[test]
fn ground_contact_is_interpolated_to_the_plane() {
    let body = Ballistics {
        drag_k: Fx::ZERO,
        restitution_pm: None,
    };
    let mut p = launch(300_000, Angle::from_degrees_int(5), Fx::from_int(1));
    loop {
        let (f, _) = advance_projectile(&mut p, &body, u32::MAX, &env(), SUBSTEPS_PER_TICK);
        if f == Flight::Ground {
            break;
        }
    }
    assert_eq!(p.z, Fx::ZERO);
}
