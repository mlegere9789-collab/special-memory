//! T-022: the fire and smoke parts of `battle_hash`, and the `set_fire_mode` payload.

mod t022_support;

use battle_rules::{
    decode_fire_mode, encode_fire_mode, FireMode, OrderDecodeError, RegimentId, Training,
};
use core_math::Vec2Fx;
use t022_support::{duel, field, line, metres, tick};

#[test]
fn an_unequipped_battle_hashes_with_zero_fire_and_smoke_blocks() {
    // The same regiment before and after the fire code existed: the golden of T-018 still holds
    // (tests/battle_hash.rs); here, stepping the fire phase with nothing equipped changes nothing.
    let mut b = field(1);
    b.spawn_regiment(line(
        0,
        40,
        Training::Trained,
        metres(100, 100),
        core_math::Angle::ZERO,
    ));
    let h = b.hash();
    b.step_fire();
    b.step_smoke();
    assert_eq!(b.hash(), h);
    assert!(b.fire.is_empty() && b.pool.is_empty());
}

#[test]
fn equal_builds_agree_and_every_fire_field_is_hashed() {
    let build = || {
        let (mut b, _, _) = duel(5, "rifle_58", 0, Training::Trained, 6, 100);
        for _ in 0..700 {
            tick(&mut b);
        }
        b
    };
    let (a, b) = (build(), build());
    assert!(!a.pool.is_empty());
    assert_eq!(a.hash(), b.hash());
    let h = a.hash();
    type Tweak = fn(&mut battle_rules::BattleState);
    let tweaks: [(&str, Tweak); 14] = [
        ("rounds", |b| b.fire[0].rounds += 1),
        ("ammo_sel", |b| b.fire[0].ammo_sel += 1),
        ("step", |b| b.fire[0].step ^= 1),
        ("left", |b| b.fire[0].left += 1),
        ("fouling", |b| b.fire[0].fouling += 1),
        ("mag", |b| b.fire[0].mag += 1),
        ("disabled_until", |b| b.fire[0].disabled_until += 1),
        ("rng", |b| b.fire[0].rng[3] ^= 1),
        ("seq", |b| b.seq += 1),
        ("pool seed", |b| b.pool[0].seed ^= 1),
        ("pool fuse", |b| b.pool[0].fuse += 1),
        ("pool z", |b| b.pool[0].z += core_math::Fx::EPSILON),
        ("mode", |b| {
            b.set_fire_mode(RegimentId(0), FireMode::HoldFire);
        }),
        ("smoke", |b| {
            b.smoke = battle_rules::SmokeGrid::new(8000, 2, 2, Vec2Fx::ZERO).unwrap();
            b.smoke.density[1] = 1;
        }),
    ];
    for (name, tweak) in tweaks {
        let mut c = a.clone();
        tweak(&mut c);
        assert_ne!(c.hash(), h, "{name} is not hashed");
    }
    let mut b = b;
    b.pool.pop();
    assert_ne!(b.hash(), h, "pool length is not hashed");
}

#[test]
fn fire_mode_payload_round_trips() {
    for (r, m) in [
        (RegimentId(0), FireMode::HoldFire),
        (
            RegimentId(7),
            FireMode::AtWill {
                target: RegimentId(65_535),
            },
        ),
    ] {
        let bytes = encode_fire_mode(r, m);
        assert_eq!(bytes[0], 5);
        assert_eq!(decode_fire_mode(&bytes), Ok((r, m)));
    }
    let ok = encode_fire_mode(RegimentId(1), FireMode::HoldFire);
    assert_eq!(decode_fire_mode(&ok[..4]), Err(OrderDecodeError::Truncated));
    let mut long = ok.clone();
    long.push(0);
    assert_eq!(
        decode_fire_mode(&long),
        Err(OrderDecodeError::TrailingBytes)
    );
    let mut bad = ok.clone();
    bad[0] = 9;
    assert_eq!(decode_fire_mode(&bad), Err(OrderDecodeError::UnknownTag(9)));
    bad = ok;
    bad[3] = 4;
    assert_eq!(decode_fire_mode(&bad), Err(OrderDecodeError::UnknownTag(4)));
}
