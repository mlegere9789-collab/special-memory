//! Aiming and dispersion (SPEC-005 section 3): the fixed set of draws every shot reads, the
//! range estimate, and the launch angles.
//!
//! The training numbers below are placeholders (calibration target CT-005-7).

use crate::{angle_from_urad, fx_ratio, gauss_unit, AmmoKind, AmmoRow, RangeTable, Training};
use crate::{WeaponRow, SMOKE_REF_UNITS};
use core_math::{Angle, Fx, Rng};

/// Words a shot reads from the soldier's stream, whatever the outcome: one misfire roll, four
/// gauss draws of twelve, one projectile seed.
pub const SHOT_WORDS: u32 = 50;

/// Range-estimate error in permille per unit of `gauss`: raw 250, trained 150, veteran 100.
pub const fn range_sigma_pm(t: Training) -> u32 {
    match t {
        Training::Raw => 250,
        Training::Trained => 150,
        Training::Veteran => 100,
    }
}

/// Bias of the elevation toward aiming high, micro-radians: raw 4000, trained 2000, veteran 1000.
pub const fn aim_high_urad(t: Training) -> u32 {
    match t {
        Training::Raw => 4000,
        Training::Trained => 2000,
        Training::Veteran => 1000,
    }
}

/// The shooter's own aim error, micro-radians: raw 12000, trained 6000, veteran 4000.
pub const fn aim_sigma_urad(t: Training) -> u32 {
    match t {
        Training::Raw => 12_000,
        Training::Trained => 6000,
        Training::Veteran => 4000,
    }
}

/// Everything one shot draws, in stream order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShotDraws {
    /// Misfire roll (`% 10^6` against the misfire chance).
    pub misfire_roll: u32,
    /// Range-estimate error, standard normal.
    pub range: Fx,
    /// Azimuth error, standard normal.
    pub azimuth: Fx,
    /// Elevation error, standard normal.
    pub elevation: Fx,
    /// Fuse error, standard normal.
    pub fuse: Fx,
    /// Seed of the projectile's own draws.
    pub seed: u64,
}

/// Reads one shot's draws: exactly [`SHOT_WORDS`] words in the fixed order.
pub fn draw_shot(rng: &mut Rng) -> ShotDraws {
    let misfire_roll = rng.next_u32();
    let range = gauss_unit(rng);
    let azimuth = gauss_unit(rng);
    let elevation = gauss_unit(rng);
    let fuse = gauss_unit(rng);
    ShotDraws {
        misfire_roll,
        range,
        azimuth,
        elevation,
        fuse,
        seed: rng.next_u64(),
    }
}

/// Smoke at the shooter in permille: `min(1000, density * 1000 / SMOKE_REF_UNITS)`.
pub fn smoke_pm(density: u32) -> u32 {
    (u64::from(density) * 1000 / u64::from(SMOKE_REF_UNITS)).min(1000) as u32
}

/// Total dispersion in micro-radians: `sqrt(sigma_b^2 + sigma_s^2)` with `sigma_b = sigma0 +
/// slope * r / 100000` and `sigma_s` the shooter's aim error scaled by the weapon, fatigue
/// (ppm) and smoke (permille), each factor floored in turn.
pub fn sigma_urad(
    weapon: &WeaponRow,
    training: Training,
    fatigue_ppm: u32,
    smoke_pm: u32,
    range_mm: u64,
) -> Fx {
    let sb = u64::from(weapon.sigma0_urad)
        + u64::from(weapon.sigma_slope_urad_per_100m) * range_mm / 100_000;
    let mut ss = u64::from(aim_sigma_urad(training));
    ss = ss * u64::from(weapon.aim_scale_pm) / 1000;
    ss = ss * (1000 + 600 * u64::from(fatigue_ppm) / 1_000_000) / 1000;
    ss = ss * (1000 + 4 * u64::from(smoke_pm)) / 1000;
    let sum = (sb * sb + ss * ss).min(i32::MAX as u64);
    Fx::from_int(sum as i32).sqrt()
}

/// What a shooter's draws and situation give: launch angles and fuse.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Aim {
    /// Horizontal launch direction.
    pub azimuth: Angle,
    /// Launch elevation above the horizontal.
    pub elevation: Angle,
    /// Sub-steps to the burst: -1 none, 0 at the muzzle, else the fuse.
    pub fuse: i32,
}

/// The shooter's situation for [`aim`].
pub struct AimInput<'a> {
    /// The weapon fired.
    pub weapon: &'a WeaponRow,
    /// The round fired.
    pub ammo: &'a AmmoRow,
    /// Range table of this weapon and round.
    pub table: &'a RangeTable,
    /// Shooter's training.
    pub training: Training,
    /// Shooter's fatigue, ppm.
    pub fatigue_ppm: u32,
    /// Smoke at the shooter, permille.
    pub smoke_pm: u32,
    /// True horizontal range to the target in metres.
    pub range: Fx,
    /// Bearing to the target.
    pub bearing: Angle,
}

/// Launch angles and fuse for one shot: the range estimate is the true range scaled by
/// `1000 + range_sigma_pm * g` permille, the elevation is the table's for that estimate plus the
/// training bias, and azimuth and elevation errors are `sigma * gauss` micro-radians.
pub fn aim(i: &AimInput, d: &ShotDraws) -> Aim {
    let thousand = Fx::from_int(1000);
    let scale = thousand + Fx::from_int(range_sigma_pm(i.training) as i32) * d.range;
    let r_est = (i.range * scale / thousand).max(Fx::ZERO);
    let range_mm = (i64::from((i.range * thousand).to_int_floor())).max(0) as u64;
    let sigma = sigma_urad(i.weapon, i.training, i.fatigue_ppm, i.smoke_pm, range_mm);
    let urad = |g: Fx| angle_from_urad(i64::from((sigma * g).to_int_floor()));
    let elevation = i.table.elevation(r_est)
        + angle_from_urad(i64::from(aim_high_urad(i.training)))
        + urad(d.elevation);
    let fuse = match (i.ammo.kind, &i.ammo.burst) {
        (AmmoKind::Canister, _) => 0,
        (AmmoKind::Shell | AmmoKind::Case, Some(b)) => {
            let err = (Fx::from_int(b.fuse_sigma_substeps as i32) * d.fuse).to_int_round_half_up();
            (i.table.time_of_flight(r_est) as i32 + err).max(1)
        }
        _ => -1,
    };
    Aim {
        azimuth: i.bearing + urad(d.azimuth),
        elevation,
        fuse,
    }
}

/// Launch velocity split into horizontal speed and vertical speed for a muzzle speed in mm/s.
pub fn launch_speeds(muzzle_mm_s: u32, elevation: Angle) -> (Fx, Fx) {
    let v = fx_ratio(i64::from(muzzle_mm_s), 1000);
    (v * elevation.cos(), v * elevation.sin())
}
