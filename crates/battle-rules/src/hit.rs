//! Hit test and flight phase (SPEC-005 sections 2 and 5, amendment A5).
//!
//! A projectile flies one tick, giving a chord. [`resolve_chord`] tests the chord against the
//! obstacles and against the soldiers of the frozen start-of-tick snapshot and queues the hits.
//! Nothing here changes an agent, so the result does not depend on the order of projectiles.

use crate::burst::{burst_children, FUSE_INERT, FUSE_PELLET};
use crate::fire::posture_of;
use crate::{
    energy_joules, AmmoKind, Ballistics, BattleState, DamageTable, Flight, FlightEnv, HitOutcome,
    Projectile, Tables, TerrainGrid, F_DISABLED, F_WOUNDED,
};
use crate::{FireRecord, FireRecordKind};
use core_math::{Fx, SplitMix64, Vec2Fx};
use sim_state::{AgentId, AgentStore, SpatialGrid};

/// A projectile at less than this distance flown does not hit its own unit (placeholder CT-005-5).
const OWN_UNIT_CLEARANCE_MM: i64 = 2500;
/// Energy that must remain after a body hit for the projectile to fly on, in joules.
const PIERCE_MIN_J: i32 = 150;
/// Bodies one projectile may strike.
const MAX_HITS: u8 = 8;
/// Largest half-width of a body (a gun, 1400 mm wide) in millimetres.
const QUERY_HALF_WIDTH_MM: i64 = 700;

/// A wall or earthwork: a segment with a height above the ground.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Obstacle {
    /// One end.
    pub a: Vec2Fx,
    /// The other end.
    pub b: Vec2Fx,
    /// Height in millimetres.
    pub height_mm: u16,
}

/// The obstacles of a scenario (static; not part of the state hash).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ObstacleLayer {
    /// The segments.
    pub segments: Vec<Obstacle>,
}

/// A hit waiting to be applied at the end of the tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QueuedHit {
    /// Who was hit.
    pub target: AgentId,
    /// What the hit does.
    pub outcome: HitOutcome,
    /// Ticks down, for a disabling hit.
    pub disabled_ticks: u32,
    /// `seq` of the projectile.
    pub seq: u64,
    /// Unit of the shooter.
    pub shooter_unit: u32,
    /// Ammunition row.
    pub ammo: u8,
}

/// The frozen start-of-tick world a chord is tested against.
#[derive(Clone, Copy)]
pub struct HitWorld<'a> {
    /// Agents.
    pub agents: &'a AgentStore,
    /// Spatial index rebuilt at the start of the tick.
    pub grid: &'a SpatialGrid,
    /// Obstacles.
    pub obstacles: &'a ObstacleLayer,
    /// Terrain (for cover).
    pub terrain: &'a TerrainGrid,
    /// Rules tables.
    pub tables: &'a Tables,
}

/// The straight path of one tick: positions, heights (metres) and distance flown (mm).
#[derive(Clone, Copy, Debug)]
pub struct Chord {
    /// Start.
    pub p0: Vec2Fx,
    /// End.
    pub p1: Vec2Fx,
    /// Height at the start.
    pub z0: Fx,
    /// Height at the end.
    pub z1: Fx,
    /// Distance flown at the start.
    pub travelled0: u32,
    /// Distance flown at the end.
    pub travelled1: u32,
}

/// What a chord did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChordResult {
    /// The projectile ended on the chord (obstacle, cover, or too little energy left).
    pub stopped: bool,
    /// Soldiers reached, whether cover absorbed the hit or not.
    pub struck: u32,
}

/// A fraction of the chord in Q32 (`0..=2^32`).
type Q32 = i128;

fn ratio_q32(n: i128, d: i128) -> Q32 {
    if d <= 0 {
        return 0;
    }
    let shift = if n > 1 << 94 { 32 } else { 0 };
    ((n >> shift) << 32) / (d >> shift).max(1)
}

fn raw2(v: Vec2Fx) -> (i128, i128) {
    (i128::from(v.x.raw()), i128::from(v.y.raw()))
}

fn cross(a: (i128, i128), b: (i128, i128)) -> i128 {
    a.0 * b.1 - a.1 * b.0
}

/// Where along the chord it crosses the segment, if it does.
fn obstacle_t(p0: Vec2Fx, p1: Vec2Fx, o: &Obstacle) -> Option<Q32> {
    let (r, a0) = (raw2(p1 - p0), raw2(o.a - p0));
    let e = raw2(o.b - o.a);
    let (mut den, mut tn, mut un) = (cross(r, e), cross(a0, e), cross(a0, r));
    if den == 0 {
        return None;
    }
    if den < 0 {
        (den, tn, un) = (-den, -tn, -un);
    }
    if tn < 0 || tn > den || un < 0 || un > den {
        return None;
    }
    Some(ratio_q32(tn, den))
}

fn lerp(a: i128, b: i128, t: Q32) -> i128 {
    a + (((b - a) * t) >> 32)
}

/// Tests one chord. `speed` is the projectile's speed at the start of the tick (m/s). Hits are
/// pushed to `out`; a hit that does not end the projectile scales its velocity (and the speed
/// of later hits on this chord) by `sqrt(E' / E)`.
///
/// Events on the chord are the first obstacle whose height reaches the path and the soldiers
/// within the body half-width of it. The earliest wins and the obstacle wins ties. A soldier hit
/// rolls cover, then the outcome and its duration from `SplitMix64(seed)` words `3h`, `3h + 1`
/// and `3h + 2` with `h` the number of bodies already struck by this projectile.
pub fn resolve_chord(
    w: &HitWorld,
    p: &mut Projectile,
    chord: &Chord,
    speed: Fx,
    out: &mut Vec<QueuedHit>,
) -> ChordResult {
    let mut res = ChordResult {
        stopped: false,
        struck: 0,
    };
    let (p0r, rr) = (raw2(chord.p0), raw2(chord.p1 - chord.p0));
    let len_sq = rr.0 * rr.0 + rr.1 * rr.1;
    let z_at = |t: Q32| lerp(i128::from(chord.z0.raw()), i128::from(chord.z1.raw()), t);

    let mut t_stop: Option<Q32> = None;
    for o in &w.obstacles.segments {
        if let Some(t) = obstacle_t(chord.p0, chord.p1, o) {
            let height = i128::from(o.height_mm) << 32;
            if z_at(t) * 1000 <= height && t_stop.is_none_or(|s| t < s) {
                t_stop = Some(t);
            }
        }
    }

    let half = crate::fx_ratio(QUERY_HALF_WIDTH_MM, 1000);
    let mut near = Vec::new();
    w.grid.query_segment(chord.p0, chord.p1, half, &mut near);

    let (ammo, burst) = {
        let row = &w.tables.ammo.rows()[usize::from(p.ammo)];
        (row, row.burst.as_ref())
    };
    let pellet = p.fuse == FUSE_PELLET;
    let (mass_g, absorb_j) = match (pellet, burst) {
        (true, Some(b)) => (b.pellet_mass_g, w.tables.damage.absorb_pellet_j()),
        _ => (ammo.mass_g, w.tables.damage.absorb_j(ammo.kind)),
    };
    let damage: &DamageTable = &w.tables.damage;
    let mut speed = speed;

    for id in near {
        let Some(a) = w.agents.get(id) else { continue };
        if id.index == p.owner || a.flags & (F_DISABLED | F_WOUNDED) != 0 {
            continue;
        }
        let row = damage.posture(posture_of(a.flags));
        let q = raw2(a.pos);
        let t = if len_sq == 0 {
            0
        } else {
            let along = (q.0 - p0r.0) * rr.0 + (q.1 - p0r.1) * rr.1;
            ratio_q32(along.clamp(0, len_sq), len_sq)
        };
        let (dx, dy) = (
            q.0 - p0r.0 - ((rr.0 * t) >> 32),
            q.1 - p0r.1 - ((rr.1 * t) >> 32),
        );
        let half_w = crate::fx_ratio(i64::from(row.width_mm), 2000).raw();
        let half_w = i128::from(half_w);
        if dx * dx + dy * dy > half_w * half_w {
            continue;
        }
        let z = z_at(t);
        let height = i128::from(row.height_mm) << 32;
        if z < 0 || z * 1000 > height {
            continue;
        }
        if t_stop.is_some_and(|s| t >= s) {
            break;
        }
        let flown = lerp(
            i128::from(chord.travelled0),
            i128::from(chord.travelled1),
            t,
        );
        if a.unit == p.unit && flown < i128::from(OWN_UNIT_CLEARANCE_MM) {
            continue;
        }

        res.struck += 1;
        let mut sm = SplitMix64::new(p.seed);
        for _ in 0..3 * u32::from(p.hits) {
            sm.next_u64();
        }
        let (cover_roll, outcome_roll, ticks_roll) = (sm.next_u64(), sm.next_u64(), sm.next_u64());
        let terrain_cover = w
            .tables
            .terrain
            .class(w.terrain.class_at(a.pos))
            .map_or(0, |c| c.cover_pm);
        if cover_roll % 1000 < u64::from(terrain_cover * row.cover_pm / 1000) {
            res.stopped = true;
            return res;
        }
        let energy = energy_joules(mass_g, speed);
        let height_pm = (z * 1_000_000 / height).clamp(0, 1000) as u32;
        let band = damage.band_for_energy(energy);
        let region = damage.region_for_height(height_pm);
        let outcome = damage.outcome(band, region, outcome_roll);
        if outcome != HitOutcome::NoEffect {
            out.push(QueuedHit {
                target: id,
                outcome,
                disabled_ticks: damage.disabled_ticks(ticks_roll),
                seq: p.seq,
                shooter_unit: p.unit,
                ammo: p.ammo,
            });
        }
        p.hits += 1;
        let left = energy - Fx::from_int(absorb_j.min(i32::MAX as u32) as i32);
        if left < Fx::from_int(PIERCE_MIN_J) || p.hits >= MAX_HITS {
            res.stopped = true;
            return res;
        }
        let factor = (left / energy).sqrt();
        speed *= factor;
        p.vel = p.vel.scale(factor);
        p.vz *= factor;
    }
    res.stopped = t_stop.is_some();
    res
}

impl BattleState {
    /// The flight and hit phase (REGISTRY TK-07), projectiles in ascending `seq`.
    pub fn step_flight(&mut self) {
        self.step_flight_with(false);
    }

    /// [`BattleState::step_flight`] with the order of resolution chosen; `reverse` walks the pool
    /// backwards. The result is the same either way (test hook).
    ///
    /// Each projectile flies one tick, its chord is tested ([`resolve_chord`]) and its end
    /// handled: ground contact bursts a shell or case, an expired fuse bursts any burst round
    /// (or leaves a dud flying inert), a canister with fuse zero bursts before it flies. Children
    /// are numbered after the phase, ascending by parent `seq`, and join the tail of the pool.
    #[doc(hidden)]
    pub fn step_flight_with(&mut self, reverse: bool) {
        if self.pool.is_empty() {
            return;
        }
        let BattleState {
            world,
            grid,
            terrain,
            tables,
            pool,
            obstacles,
            weather,
            seq,
            counters,
            pending,
            sound_log,
            ..
        } = self;
        let env = FlightEnv {
            gravity: tables.ammo.constants().gravity,
            wind: weather.wind,
            ground_pm: weather.ground_pm,
        };
        let reach: Vec<u32> = tables
            .ammo
            .rows()
            .iter()
            .map(|a| {
                tables
                    .weapons
                    .rows()
                    .iter()
                    .filter(|w| w.ammo.iter().any(|l| l.id == a.id))
                    .map(|w| w.reach_mm)
                    .max()
                    .unwrap_or(u32::MAX)
            })
            .collect();
        let hw = HitWorld {
            agents: &world.agents,
            grid,
            obstacles,
            terrain,
            tables,
        };
        let n = pool.len();
        let mut alive = vec![true; n];
        let mut born: Vec<(u64, Vec<Projectile>)> = Vec::new();
        let mut bursts_heard: Vec<FireRecord> = Vec::new();
        for k in 0..n {
            let i = if reverse { n - 1 - k } else { k };
            let p = &mut pool[i];
            let ammo = &tables.ammo.rows()[usize::from(p.ammo)];
            if p.fuse == 0 && ammo.kind == AmmoKind::Canister {
                if let Some(c) = ammo.burst.as_ref().and_then(|b| burst_children(p, ammo, b)) {
                    alive[i] = false;
                    bursts_heard.push(FireRecord::of(FireRecordKind::Burst, p));
                    born.push((p.seq, c));
                    continue;
                }
                p.fuse = FUSE_INERT;
            }
            let body = match (p.fuse == FUSE_PELLET, ammo.burst.as_ref()) {
                (true, Some(b)) => Ballistics::of_pellet(b),
                _ => Ballistics::of_ammo(ammo),
            };
            let start_speed = p.velocity().length();
            let (p0, z0, t0) = (p.pos, p.z, p.travelled_mm);
            let flight = crate::step_projectile(p, &body, reach[usize::from(p.ammo)], &env);
            let chord = Chord {
                p0,
                p1: p.pos,
                z0,
                z1: p.z,
                travelled0: t0,
                travelled1: p.travelled_mm,
            };
            if resolve_chord(&hw, p, &chord, start_speed, pending).stopped {
                alive[i] = false;
                continue;
            }
            let bursts = match flight {
                Flight::Flying | Flight::Ricochet => continue,
                Flight::Ground => {
                    matches!(ammo.kind, AmmoKind::Shell | AmmoKind::Case)
                        && p.fuse != FUSE_INERT
                        && p.fuse != FUSE_PELLET
                }
                Flight::FuseExpired => true,
                Flight::ReachLimit | Flight::TooSlow => false,
            };
            alive[i] = false;
            if bursts {
                match ammo.burst.as_ref().and_then(|b| burst_children(p, ammo, b)) {
                    Some(c) => {
                        bursts_heard.push(FireRecord::of(FireRecordKind::Burst, p));
                        born.push((p.seq, c));
                    }
                    None if flight == Flight::FuseExpired => {
                        p.fuse = FUSE_INERT;
                        alive[i] = true;
                    }
                    None => {}
                }
            }
        }
        let mut k = 0;
        pool.retain(|_| {
            k += 1;
            alive[k - 1]
        });
        bursts_heard.sort_by_key(|r| r.seq);
        sound_log.extend(bursts_heard);
        born.sort_by_key(|b| b.0);
        for (_, children) in born {
            for mut c in children {
                c.seq = *seq;
                *seq += 1;
                counters.projectiles_spawned += 1;
                pool.push(c);
            }
        }
    }
}
