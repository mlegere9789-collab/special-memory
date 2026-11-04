//! T-022 spawn test: muzzle position, launch vector magnitude, and a fixed draw count of 50 per
//! shot for hit, misfire and out-of-range cases (SPEC-005 sections 3 and 4, REGISTRY 6.4).

mod t022_support;

use battle_rules::aim::{draw_shot, SHOT_WORDS};
use battle_rules::{fx_ratio, soldier_stream, FireMode, RegimentId, Training};
use core_math::{Angle, Fx, Rng, Vec2Fx};
use t022_support::{duel, metres, no_misfire, tick};

fn advance(rng: &mut Rng, words: u32) {
    for _ in 0..words {
        rng.next_u64();
    }
}

#[test]
fn a_shot_reads_exactly_fifty_words_whatever_it_draws() {
    assert_eq!(SHOT_WORDS, 50);
    for seed in 0..20 {
        let mut a = Rng::from_seed(seed);
        let mut r = a.clone();
        let _ = draw_shot(&mut a);
        advance(&mut r, 50);
        assert_eq!(a.state(), r.state());
    }
}

/// Runs until soldier 0 has taken one shot; returns the stream state before and after.
fn one_shot(mut b: battle_rules::BattleState) -> (battle_rules::BattleState, [u64; 4], [u64; 4]) {
    for _ in 0..2000 {
        let before = b.fire[0].rng;
        let shots = b.counters().shots_fired;
        tick(&mut b);
        if b.counters().shots_fired > shots {
            let after = b.fire[0].rng;
            return (b, before, after);
        }
    }
    panic!("no shot");
}

fn assert_fifty(before: [u64; 4], after: [u64; 4]) {
    let mut r = Rng::from_state(before);
    advance(&mut r, 50);
    assert_eq!(r.state(), after);
}

#[test]
fn hit_misfire_and_beyond_effective_range_all_draw_fifty_words() {
    // Inside effective range.
    let (mut b, _, _) = duel(2, "rifle_58", 0, Training::Trained, 1, 100);
    no_misfire(&mut b);
    let (b, before, after) = one_shot(b);
    assert_eq!(b.pool.len(), 1);
    assert_fifty(before, after);

    // Misfire.
    let (mut b, _, _) = duel(2, "rifle_58", 0, Training::Trained, 1, 100);
    b.weather.wet_pm = 100_000;
    let (b, before, after) = one_shot(b);
    assert!(b.pool.is_empty());
    assert_fifty(before, after);

    // Beyond effective range (274 m) but inside extended range (805 m), past the table's pace.
    let (mut b, _, _) = duel(2, "rifle_58", 0, Training::Raw, 1, 700);
    no_misfire(&mut b);
    let (b, before, after) = one_shot(b);
    assert_eq!(b.pool.len(), 1);
    assert_fifty(before, after);
}

#[test]
fn a_target_beyond_extended_range_is_held_without_a_draw_or_a_round() {
    let (mut b, _, _) = duel(2, "smoothbore_69", 0, Training::Trained, 1, 400);
    let state = b.fire[0].rng;
    for _ in 0..1500 {
        tick(&mut b);
    }
    assert_eq!(b.counters().shots_fired, 0);
    assert_eq!(b.fire[0].rng, state);
    assert_eq!(b.fire[0].rounds, 60);
}

#[test]
fn no_target_mode_or_empty_target_regiment_holds_fire() {
    let (mut b, shooters, targets) = duel(2, "smoothbore_69", 0, Training::Trained, 1, 40);
    assert!(b.set_fire_mode(shooters, FireMode::HoldFire));
    for _ in 0..1500 {
        tick(&mut b);
    }
    assert_eq!(b.counters().shots_fired, 0);
    assert!(b.set_fire_mode(shooters, FireMode::AtWill { target: targets }));
    assert!(!b.set_fire_mode(RegimentId(99), FireMode::HoldFire));
}

#[test]
fn a_projectile_leaves_the_muzzle_with_the_muzzle_speed() {
    let (mut b, _, _) = duel(6, "rifle_58", 0, Training::Veteran, 1, 100);
    b.weather.wet_pm = 0;
    let shooter = b.regiments[0].roster[0];
    let pos = b.world.agents.pos(shooter).unwrap();
    let facing = b.world.agents.facing(shooter).unwrap();
    let (b, _, _) = one_shot(b);
    let p = &b.pool[0];
    assert_eq!(p.seq, 0);
    assert_eq!(p.owner, shooter.index);
    assert_eq!(p.unit, 0);
    assert_eq!(p.team, 0);
    assert_eq!(p.ammo, 1, "minie_58");
    assert_eq!(p.fuse, -1);
    assert_eq!((p.travelled_mm, p.bounces, p.hits), (0, 0, 0));
    let muzzle = pos + Vec2Fx::from_angle(facing).scale(fx_ratio(600, 1000));
    assert_eq!(p.pos, muzzle);
    assert_eq!(p.z, fx_ratio(1450, 1000));
    let speed = p.velocity().length();
    let want = fx_ratio(290_000, 1000);
    let err = (speed - want).abs();
    assert!(err < Fx::from_raw(want.raw() / 10_000), "speed {speed:?}");
    // Aimed along the line to the target within a degree.
    let toward = Angle::atan2(p.vel.y, p.vel.x);
    let diff = i64::from(toward.raw().wrapping_sub(Angle::ZERO.raw()) as i32).abs();
    assert!(diff < i64::from(Angle::from_degrees_int(2).raw()));
    assert!(p.vz > Fx::ZERO);
    let _ = (metres(0, 0), soldier_stream(0));
}

#[test]
fn projectiles_take_ascending_seq_in_ascending_shooter_order() {
    let (mut b, _, _) = duel(8, "smoothbore_69", 0, Training::Trained, 50, 40);
    for _ in 0..800 {
        tick(&mut b);
    }
    assert!(b.pool.len() > 40);
    for (i, p) in b.pool.iter().enumerate() {
        assert_eq!(p.seq, i as u64);
    }
    assert_eq!(b.counters().projectiles_spawned, b.pool.len() as u64);
    assert_eq!(b.counters().unit_level_rolls, 0);
}

#[test]
fn shell_and_canister_fuses() {
    let (b, _, _) = duel(3, "napoleon_12", 1, Training::Trained, 1, 400);
    let (b, _, _) = one_shot(b);
    assert!(b.pool[0].fuse > 0, "shell fuse in sub-steps");
    assert_eq!(b.pool[0].ammo, 5);
    let (b, _, _) = duel(3, "napoleon_12", 3, Training::Trained, 1, 150);
    let (b, _, _) = one_shot(b);
    assert_eq!(b.pool[0].fuse, 0, "canister bursts at the muzzle");
    assert_eq!(b.pool[0].ammo, 7);
}
