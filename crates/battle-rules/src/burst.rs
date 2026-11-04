//! Bursts (SPEC-005 section 2, amendment A5): the children of a shell, case or canister.
//!
//! All draws come from `SplitMix64::new(parent.seed)`: the first is the dud roll, then each
//! child takes two, the seed of a private [`Rng`] for its direction and its own `seed`.

use crate::{angle_from_urad, fx_ratio, gauss_unit, AmmoRow, Burst, BurstShape, Projectile};
use core_math::{Angle, Fx, Rng, SplitMix64, Vec2Fx};

/// `Projectile::fuse` of a round with no fuse.
pub const FUSE_NONE: i32 = -1;
/// `Projectile::fuse` of a dud, which flies on as inert shot.
pub const FUSE_INERT: i32 = -2;
/// `Projectile::fuse` of a burst child (a pellet or fragment).
pub const FUSE_PELLET: i32 = -3;

/// A turn is 2^32 raw angle units; 45 degrees is an eighth.
const EIGHTH_TURN: u32 = 1 << 29;

/// The children of `parent` bursting now (their `seq` is zero; the caller numbers them), or
/// `None` when the dud roll says the round fails to burst.
pub fn burst_children(
    parent: &Projectile,
    ammo: &AmmoRow,
    burst: &Burst,
) -> Option<Vec<Projectile>> {
    let mut sm = SplitMix64::new(parent.seed);
    if sm.next_u64() % 1_000_000 < u64::from(burst.dud_ppm) {
        return None;
    }
    let v = parent.velocity();
    let horizontal = Vec2Fx::new(v.x, v.y);
    let (parent_az, parent_el) = (
        Angle::atan2(v.y, v.x),
        Angle::atan2(v.z, horizontal.length()),
    );
    let pm = fx_ratio(i64::from(burst.pellet_speed_pm), 1000);
    let sigma = Fx::from_int(i32::try_from(burst.spread_sigma_urad).unwrap_or(i32::MAX));
    let mut out = Vec::with_capacity(burst.count as usize);
    for _ in 0..burst.count {
        let mut rng = Rng::from_seed(sm.next_u64());
        let seed = sm.next_u64();
        let (az, el, speed) = match burst.shape {
            BurstShape::Cone => {
                let err = |g: Fx| angle_from_urad(i64::from((sigma * g).to_int_floor()));
                let az = parent_az + err(gauss_unit(&mut rng));
                let el = parent_el + err(gauss_unit(&mut rng));
                (az, el, v.length() * pm)
            }
            BurstShape::Sphere => {
                let az = Angle::from_raw(rng.next_u32());
                let up = i64::from(rng.range_u32(0, 2 * EIGHTH_TURN)) - i64::from(EIGHTH_TURN);
                let el = Angle::from_raw(up.rem_euclid(1 << 32) as u32);
                (az, el, fx_ratio(i64::from(ammo.muzzle_mm_s), 1000) * pm)
            }
        };
        out.push(Projectile {
            seq: 0,
            owner: parent.owner,
            unit: parent.unit,
            team: parent.team,
            ammo: parent.ammo,
            pos: parent.pos,
            z: parent.z,
            vel: Vec2Fx::from_angle(az).scale(speed * el.cos()),
            vz: speed * el.sin(),
            travelled_mm: 0,
            bounces: 0,
            hits: 0,
            fuse: FUSE_PELLET,
            seed,
        });
    }
    Some(out)
}
