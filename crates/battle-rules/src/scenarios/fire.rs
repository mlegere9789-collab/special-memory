//! Headless fire scenarios (SPEC-005 sections 9 and 11): the calibration set-ups `cal_line`,
//! `cal_rof`, `cal_range`, `cal_canister` and the 20,000-soldier reference `f0_large`.
//!
//! Every scenario builds a [`BattleState`] at tick 0 with its regiments equipped and set to fire
//! at will; the only random draws are the soldier streams and, in `f0_large`, the loading phases
//! drawn once from `STREAM_BATTLE_SETUP`.

use crate::{
    fx_ratio, step_ticks, Ballistics, BattleState, FireMode, Flight, FlightEnv, Formation,
    Obstacle, Projectile, RegimentId, RegimentSpec, Tables, TerrainGrid, Training, WeaponClass,
    F_GUN, STREAM_BATTLE_SETUP,
};
use core_math::{Angle, Fx, RngStreams, Vec2Fx};
use sim_state::{SpatialGrid, WorldState};

/// Millimetres of a obstacle too tall for any projectile to clear (a stop for every round).
const BACKSTOP_MM: u16 = 30_000;
/// Height of the wall of `cal_line` in millimetres.
const WALL_MM: u16 = 1000;
/// Distance of that wall in front of the defending line, millimetres.
const WALL_AHEAD_MM: i64 = 1000;
/// Men in each regiment of `cal_line` (100 files, 2 ranks).
pub const LINE_MEN: usize = 200;
/// Guns of the battery of `cal_rof` when the weapon is a field gun.
pub const GUN_BATTERY: usize = 6;
/// Soldiers of `f0_large`.
pub const F0_LARGE_MEN: usize = 20_000;
/// Rounds the gun of `cal_canister` carries.
pub const CANISTER_ROUNDS: u16 = 200;

fn metres(x: i32, y: i32) -> Vec2Fx {
    Vec2Fx::new(Fx::from_int(x), Fx::from_int(y))
}

/// An empty battle on open ground covering `[0, w * cell) x [0, h * cell)` metres.
fn field(tables: &Tables, seed: u64, cell_m: i32, (w, h): (u32, u32)) -> BattleState {
    let terrain = TerrainGrid::from_rows(8000, 0, 0, &[('.', 0)], &["."], &tables.terrain)
        .expect("scenario terrain is valid");
    let grid = SpatialGrid::new(Fx::from_int(cell_m), w, h, Vec2Fx::ZERO).expect("grid size");
    BattleState::new(
        WorldState::new(RngStreams::new(seed)),
        terrain,
        grid,
        tables.clone(),
    )
}

fn line(team: u8, count: usize, training: Training, anchor: Vec2Fx) -> RegimentSpec {
    RegimentSpec {
        team,
        count,
        training,
        formation: Formation::Line,
        anchor,
        heading: if team == 0 { Angle::ZERO } else { Angle::HALF },
    }
}

/// Equips `id` with `weapon` (first ammunition) and sets it to fire at will on `target`.
fn arm(b: &mut BattleState, id: RegimentId, weapon: &str, ammo_sel: u8, target: RegimentId) {
    b.equip_regiment(id, weapon, ammo_sel)
        .expect("scenario weapon exists");
    assert!(b.set_fire_mode(id, FireMode::AtWill { target }));
}

fn wall(b: &mut BattleState, x_mm: i64, y: i32, half_width_m: i32, height_mm: u16) {
    let x = fx_ratio(x_mm, 1000);
    b.obstacles.segments.push(Obstacle {
        a: Vec2Fx::new(x, Fx::from_int(y - half_width_m)),
        b: Vec2Fx::new(x, Fx::from_int(y + half_width_m)),
        height_mm,
    });
}

/// X of the western line of the pair scenarios, metres.
const X0: i32 = 100;
/// Y of the centre of the pair scenarios, metres.
const Y0: i32 = 100;

/// `cal_line`: two regiments of 200 (100 files, 2 ranks) in `Steady(Line)`, facing each other
/// `range_mm` apart, both firing at will with `weapon`. Team 0 is the attacker; with `wall` a
/// 1000 mm wall stands one metre in front of team 1. Regiment 0 is team 0.
pub fn cal_line(
    tables: &Tables,
    seed: u64,
    weapon: &str,
    training: Training,
    range_mm: u32,
    wall_in_front: bool,
) -> BattleState {
    let mut b = field(tables, seed, 8, (256, 64));
    let east = X0 + i32::try_from(range_mm / 1000).expect("range fits");
    let a = b.spawn_regiment(line(0, LINE_MEN, training, metres(X0, Y0)));
    let d = b.spawn_regiment(line(1, LINE_MEN, training, metres(east, Y0)));
    arm(&mut b, a, weapon, 0, d);
    arm(&mut b, d, weapon, 0, a);
    if wall_in_front {
        let x_mm = i64::from(east) * 1000 - WALL_AHEAD_MM;
        wall(&mut b, x_mm, Y0, 60, WALL_MM);
    }
    b
}

/// `cal_rof`: one regiment of 200 (a battery of 6 for a field gun) firing at will at a regiment half its extended range away
/// that stands behind a wall no round can clear, so that only the loading cycle is measured.
/// Regiment 0 is the shooter.
pub fn cal_rof(tables: &Tables, seed: u64, weapon: &str, training: Training) -> BattleState {
    let row = &tables.weapons.rows()[tables
        .weapons
        .index_of(weapon)
        .expect("scenario weapon exists")];
    let range = i32::try_from(row.extended_mm / 2000).expect("range fits");
    let men = if row.class == WeaponClass::FieldGun {
        GUN_BATTERY
    } else {
        LINE_MEN
    };
    let mut b = field(tables, seed, 8, (512, 64));
    let s = b.spawn_regiment(line(0, men, training, metres(X0, Y0)));
    let t = b.spawn_regiment(line(1, 100, Training::Trained, metres(X0 + range, Y0)));
    arm(&mut b, s, weapon, 0, t);
    wall(
        &mut b,
        i64::from(X0 + range) * 1000 - WALL_AHEAD_MM,
        Y0,
        60,
        BACKSTOP_MM,
    );
    b
}

/// The single projectile of a `cal_range` check with the physics it flies by.
#[derive(Clone, Debug)]
pub struct RangeShot {
    /// The round at the muzzle.
    pub projectile: Projectile,
    /// Drag and restitution of the round.
    pub body: Ballistics,
    /// Gravity, wind (none) and dry ground.
    pub env: FlightEnv,
    /// Reach of the weapon, millimetres.
    pub reach_mm: u32,
}

/// `cal_range`: one round of `ammo` (an ammunition row id) leaving a gun muzzle at `elevation`
/// above the horizon, level in azimuth, no wind, dry ground, no fuse.
pub fn cal_range(tables: &Tables, ammo: &str, elevation: Angle) -> Option<RangeShot> {
    let ai = tables.ammo.index_of(ammo)?;
    let row = &tables.ammo.rows()[ai];
    let reach_mm = tables
        .weapons
        .rows()
        .iter()
        .filter(|w| w.ammo.iter().any(|l| l.id == ammo))
        .map(|w| w.reach_mm)
        .max()?;
    let (horizontal, vz) = crate::aim::launch_speeds(row.muzzle_mm_s, elevation);
    let muzzle_mm = tables.damage.posture(crate::Posture::Gun).muzzle_height_mm;
    Some(RangeShot {
        projectile: Projectile {
            seq: 0,
            owner: 0,
            unit: 0,
            team: 0,
            ammo: ai as u8,
            pos: Vec2Fx::ZERO,
            z: fx_ratio(i64::from(muzzle_mm), 1000),
            vel: Vec2Fx::new(horizontal, Fx::ZERO),
            vz,
            travelled_mm: 0,
            bounces: 0,
            hits: 0,
            fuse: crate::FUSE_NONE,
            seed: 0,
        },
        body: Ballistics::of_ammo(row),
        env: FlightEnv {
            gravity: tables.ammo.constants().gravity,
            wind: Vec2Fx::ZERO,
            ground_pm: 1000,
        },
        reach_mm,
    })
}

impl RangeShot {
    /// Flies the round until its first contact with the ground, a stop or the reach and returns
    /// the horizontal distance from the muzzle in millimetres.
    pub fn range_to_ground_mm(mut self) -> i64 {
        loop {
            let f =
                crate::step_projectile(&mut self.projectile, &self.body, self.reach_mm, &self.env);
            if !matches!(f, Flight::Flying) {
                let x = self.projectile.pos.x * Fx::from_int(1000);
                return i64::from(x.to_int_round_half_up());
            }
        }
    }
}

/// `cal_canister`: one gun (regiment 0, flagged `F_GUN`, trained) with 200 canister rounds, ready
/// to fire at a standing line of 100 men `range_mm` away (regiment 1), over open dry ground.
pub fn cal_canister(tables: &Tables, seed: u64, range_mm: u32) -> BattleState {
    let mut b = field(tables, seed, 8, (256, 64));
    let east = X0 + i32::try_from(range_mm / 1000).expect("range fits");
    let gun = b.spawn_regiment(line(0, 1, Training::Trained, metres(X0, Y0)));
    let target = b.spawn_regiment(line(1, 100, Training::Trained, metres(east, Y0)));
    let g = b.regiments[usize::from(gun.0)].roster[0];
    b.world.agents.set_flags(g, F_GUN);
    arm(&mut b, gun, "napoleon_12", 3, target);
    for f in &mut b.fire {
        if f.rounds > 0 {
            f.rounds = CANISTER_ROUNDS;
        }
    }
    b.weather.wet_pm = 0;
    b
}

/// Makes every equipped soldier ready to fire at once.
pub fn make_ready(b: &mut BattleState) {
    for ri in 0..b.regiments.len() {
        let Some(w) = b.regfire[ri].weapon.map(usize::from) else {
            continue;
        };
        let steps = b.tables.weapons.rows()[w].load_steps.len() as u8;
        for a in &b.regiments[ri].roster {
            let f = &mut b.fire[a.index as usize];
            f.step = steps;
            f.left = 0;
        }
    }
}

/// `f0_large`: 20,000 soldiers, 50 regiments of 400 in `Steady(Line)`, two lines of 25 facing each
/// other 200 m apart, all armed with `rifle_58` and firing at will at the regiment opposite.
/// Each soldier's place in the loading cycle is drawn once from `STREAM_BATTLE_SETUP`, so the
/// fight starts in its steady state (about 40 shots per tick) rather than in one volley.
pub fn f0_large(tables: &Tables, seed: u64) -> BattleState {
    let mut b = field(tables, seed, 4, (256, 800));
    let (y_step, east) = (120, 300);
    for i in 0..25 {
        let y = 80 + y_step * i;
        b.spawn_regiment(line(0, 400, Training::Trained, metres(100, y)));
        b.spawn_regiment(line(1, 400, Training::Trained, metres(east, y)));
    }
    for i in 0..25u16 {
        let (a, d) = (RegimentId(2 * i), RegimentId(2 * i + 1));
        arm(&mut b, a, "rifle_58", 0, d);
        arm(&mut b, d, "rifle_58", 0, a);
    }
    spread_phases(&mut b);
    b
}

/// `f0_large` with every soldier ready: the one-tick volley of all 20,000 (T5-21).
pub fn f0_volley(tables: &Tables, seed: u64) -> BattleState {
    let mut b = f0_large(tables, seed);
    make_ready(&mut b);
    b
}

/// Moves every equipped soldier to a uniformly drawn point of its loading cycle.
fn spread_phases(b: &mut BattleState) {
    let mut rng = b.world.rng.stream(STREAM_BATTLE_SETUP);
    for ri in 0..b.regiments.len() {
        let Some(w) = b.regfire[ri].weapon.map(usize::from) else {
            continue;
        };
        let training = b.regiments[ri].training;
        let row = &b.tables.weapons.rows()[w];
        let steps: Vec<u16> = (0..row.load_steps.len())
            .map(|s| step_ticks(row, s, training, 0))
            .collect();
        let total: u32 = steps.iter().map(|&t| u32::from(t)).sum();
        for a in &b.regiments[ri].roster {
            let mut p = rng.range_u32(0, total);
            let f = &mut b.fire[a.index as usize];
            for (s, &t) in steps.iter().enumerate() {
                if p < u32::from(t) {
                    f.step = s as u8;
                    f.left = t - p as u16;
                    break;
                }
                p -= u32::from(t);
            }
        }
    }
}
