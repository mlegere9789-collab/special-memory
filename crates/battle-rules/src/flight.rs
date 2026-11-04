//! Projectile flight (SPEC-005 section 2, bursts excluded): quadratic drag, gravity, wind,
//! ground contact and ricochet, in fixed point only.
//!
//! One tick is [`SUBSTEPS_PER_TICK`] sub-steps of [`DT`] seconds. Each sub-step evaluates the
//! acceleration once from the velocity at its start, `a = -k |v - w| (v - w) - (0, 0, g)`,
//! then `v' = v + a DT` and `p' = p + (v + v') DT / 2` (SPEC-005 amendment A4: the position
//! update uses the mean velocity of the sub-step, which is exact without drag).

use crate::{fx_ratio, AmmoRow, Burst, Vec3Fx};
use core_math::{Fx, Vec2Fx};

/// Sub-steps per tick.
pub const SUBSTEPS_PER_TICK: u32 = 10;

/// Sub-step length in seconds: `Fx::ONE / 200` (raw 21474836).
pub const DT: Fx = Fx::from_raw(21_474_836);

/// Half a sub-step, used for the mean-velocity position update.
const HALF_DT: Fx = Fx::from_raw(10_737_418);

/// A shot above this speed squared (30 m/s) ricochets.
const RICOCHET_MIN_SPEED_SQ: Fx = Fx::from_int(900);

/// A projectile below this speed squared (40 m/s) ends.
const MIN_SPEED_SQ: Fx = Fx::from_int(1600);

/// Horizontal speed kept by a ricochet: 0.8 (`4/5` rounded down).
const BOUNCE_HORIZONTAL: Fx = Fx::from_raw(3_435_973_836);

/// Ricochets a shot may make.
const MAX_BOUNCES: u8 = 3;

/// A projectile in flight. The pool holds these in ascending `seq`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Projectile {
    /// Unique, ascending spawn number.
    pub seq: u64,
    /// Index of the soldier that fired it.
    pub owner: u32,
    /// Index of the owner's unit.
    pub unit: u32,
    /// Owner's team.
    pub team: u8,
    /// Index of the ammunition row.
    pub ammo: u8,
    /// Horizontal position in metres.
    pub pos: Vec2Fx,
    /// Height above ground in metres.
    pub z: Fx,
    /// Horizontal velocity in m/s over the ground.
    pub vel: Vec2Fx,
    /// Vertical velocity in m/s.
    pub vz: Fx,
    /// Path length flown so far in millimetres.
    pub travelled_mm: u32,
    /// Ricochets so far.
    pub bounces: u8,
    /// Bodies struck so far.
    pub hits: u8,
    /// Sub-steps until the burst, or -1 for none.
    pub fuse: i32,
    /// Seed of the draws a burst makes.
    pub seed: u64,
}

impl Projectile {
    /// Position as a 3D vector.
    pub fn position(&self) -> Vec3Fx {
        Vec3Fx::new(self.pos.x, self.pos.y, self.z)
    }

    /// Velocity as a 3D vector.
    pub fn velocity(&self) -> Vec3Fx {
        Vec3Fx::new(self.vel.x, self.vel.y, self.vz)
    }
}

/// The physical constants of one body in flight.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ballistics {
    /// Drag constant `k` in 1/m (see [`crate::drag_k`]).
    pub drag_k: Fx,
    /// Ground restitution in permille; `Some` only for solid shot, which ricochets.
    pub restitution_pm: Option<u32>,
}

impl Ballistics {
    /// The body of an ammunition row itself (a shell, case or canister before it bursts).
    pub fn of_ammo(ammo: &AmmoRow) -> Ballistics {
        Ballistics {
            drag_k: ammo.drag_k,
            restitution_pm: ammo.restitution_pm,
        }
    }

    /// One pellet or fragment of a burst: its own drag, no ricochet.
    pub fn of_pellet(burst: &Burst) -> Ballistics {
        Ballistics {
            drag_k: burst.pellet_drag_k,
            restitution_pm: None,
        }
    }
}

/// The scenario's air and ground, shared by every projectile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FlightEnv {
    /// Gravity in m/s^2.
    pub gravity: Fx,
    /// Wind velocity in m/s (horizontal).
    pub wind: Vec2Fx,
    /// Ground restitution factor in permille: 1000 dry, 500 wet.
    pub ground_pm: u32,
}

/// How a call to [`step_projectile`] ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flight {
    /// All sub-steps flown; the projectile is still in the air.
    Flying,
    /// Shot struck the ground and bounced; flight for this tick stopped there, still alive.
    Ricochet,
    /// Ended at the ground, at the contact point (`z == 0`, velocity at impact).
    Ground,
    /// The fuse reached zero; the burst is the caller's to spawn.
    FuseExpired,
    /// Ended at the weapon's reach.
    ReachLimit,
    /// Ended below 40 m/s.
    TooSlow,
}

impl Flight {
    /// True while the projectile stays in the pool.
    pub fn is_alive(self) -> bool {
        matches!(self, Flight::Flying | Flight::Ricochet)
    }
}

/// Position and velocity during integration.
#[derive(Clone, Copy)]
pub(crate) struct Kinematics {
    pub pos: Vec3Fx,
    pub vel: Vec3Fx,
}

/// One sub-step of the integrator.
pub(crate) fn substep(s: Kinematics, k: Fx, wind: Vec2Fx, gravity: Fx) -> Kinematics {
    let rel = Vec3Fx::new(s.vel.x - wind.x, s.vel.y - wind.y, s.vel.z);
    let kv = k * rel.length();
    let acc = Vec3Fx::new(-(kv * rel.x), -(kv * rel.y), -(kv * rel.z) - gravity);
    let vel = s.vel + acc.scale(DT);
    let pos = s.pos + (s.vel + vel).scale(HALF_DT);
    Kinematics { pos, vel }
}

/// Millimetres in a length given in metres, rounded to nearest.
fn metres_to_mm(m: Fx) -> u32 {
    u32::try_from((m * Fx::from_int(1000)).to_int_round_half_up()).unwrap_or(0)
}

/// Advances one projectile by one tick (ten sub-steps) in the given environment.
///
/// See [`advance_projectile`] for the rules; this is that function with
/// [`SUBSTEPS_PER_TICK`] sub-steps and the elapsed time dropped.
pub fn step_projectile(
    p: &mut Projectile,
    body: &Ballistics,
    reach_mm: u32,
    env: &FlightEnv,
) -> Flight {
    advance_projectile(p, body, reach_mm, env, SUBSTEPS_PER_TICK).0
}

/// Advances one projectile by up to `substeps` sub-steps and reports how it ended and how many
/// sub-steps it flew (a whole number, except that a ground contact counts the fraction of the
/// last sub-step up to the instant `z = 0`).
///
/// Each sub-step is checked in this order: ground contact (`z <= 0`: a shot above 30 m/s with
/// fewer than 3 bounces ricochets with `vz = -vz * restitution_pm * ground_pm / 10^6` and the
/// horizontal speed times 0.8, ending this call's flight; anything else ends), the fuse, the
/// reach (`travelled_mm >= reach_mm`) and the minimum speed. At ground contact the position
/// and velocity are interpolated back to the instant `z = 0`. A fuse of zero or below is not
/// counted down.
pub fn advance_projectile(
    p: &mut Projectile,
    body: &Ballistics,
    reach_mm: u32,
    env: &FlightEnv,
    substeps: u32,
) -> (Flight, Fx) {
    let mut s = Kinematics {
        pos: p.position(),
        vel: p.velocity(),
    };
    let mut outcome = Flight::Flying;
    let mut elapsed = Fx::ZERO;
    for done in 0..substeps {
        let next = substep(s, body.drag_k, env.wind, env.gravity);
        if next.pos.z <= Fx::ZERO {
            // Interpolate to the ground plane; a start at or below it stays where it is.
            let f = if s.pos.z > Fx::ZERO {
                s.pos.z / (s.pos.z - next.pos.z)
            } else {
                Fx::ZERO
            };
            let at = Kinematics {
                pos: s.pos + (next.pos - s.pos).scale(f),
                vel: s.vel + (next.vel - s.vel).scale(f),
            };
            let step_mm = metres_to_mm((at.pos - s.pos).length());
            p.travelled_mm = p.travelled_mm.saturating_add(step_mm);
            elapsed = Fx::from_int(i32::try_from(done).unwrap_or(i32::MAX)) + f;
            s = at;
            s.pos.z = Fx::ZERO;
            outcome = match body.restitution_pm {
                Some(rest)
                    if s.vel.length_sq() > RICOCHET_MIN_SPEED_SQ && p.bounces < MAX_BOUNCES =>
                {
                    p.bounces += 1;
                    let factor = fx_ratio(i64::from(rest) * i64::from(env.ground_pm), 1_000_000);
                    s.vel.z = -(s.vel.z * factor);
                    s.vel.x *= BOUNCE_HORIZONTAL;
                    s.vel.y *= BOUNCE_HORIZONTAL;
                    Flight::Ricochet
                }
                _ => Flight::Ground,
            };
            break;
        }
        let step_mm = metres_to_mm((next.pos - s.pos).length());
        p.travelled_mm = p.travelled_mm.saturating_add(step_mm);
        s = next;
        elapsed = Fx::from_int(i32::try_from(done + 1).unwrap_or(i32::MAX));
        if p.fuse > 0 {
            p.fuse -= 1;
            if p.fuse == 0 {
                outcome = Flight::FuseExpired;
                break;
            }
        }
        if p.travelled_mm >= reach_mm {
            outcome = Flight::ReachLimit;
            break;
        }
        if s.vel.length_sq() < MIN_SPEED_SQ {
            outcome = Flight::TooSlow;
            break;
        }
    }
    p.pos = Vec2Fx::new(s.pos.x, s.pos.y);
    p.z = s.pos.z;
    p.vel = Vec2Fx::new(s.vel.x, s.vel.y);
    p.vz = s.vel.z;
    (outcome, elapsed)
}
