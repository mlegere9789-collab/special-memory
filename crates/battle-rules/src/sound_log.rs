//! The sound log (SPEC-005 amendment A10): one record per round that leaves the barrel and per
//! round that bursts, kept for one tick and read by the acoustics model (SPEC-012). The log is
//! not hashed and no rule reads it.

use crate::{sight::to_mm, Projectile};
use core_math::Vec2Fx;

/// What made the sound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FireRecordKind {
    /// A round left the barrel (a misfire does not).
    Shot,
    /// A shell, case or canister burst.
    Burst,
}

/// One sound-making event of the current tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FireRecord {
    /// Shot or burst.
    pub kind: FireRecordKind,
    /// `seq` of the projectile that fired or burst.
    pub seq: u64,
    /// Regiment id of the shooter.
    pub unit: u32,
    /// Side of the shooter.
    pub team: u8,
    /// Ammunition row of the round.
    pub ammo: u8,
    /// Muzzle, or place of the burst.
    pub pos: Vec2Fx,
    /// Height above the ground in millimetres.
    pub z_mm: i32,
}

impl FireRecord {
    /// The record of `p` leaving the barrel at `p.pos` (the projectile has just been created).
    pub(crate) fn of(kind: FireRecordKind, p: &Projectile) -> FireRecord {
        let z_mm = to_mm(p.z).clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
        FireRecord {
            kind,
            seq: p.seq,
            unit: p.unit,
            team: p.team,
            ammo: p.ammo,
            pos: p.pos,
            z_mm,
        }
    }
}
