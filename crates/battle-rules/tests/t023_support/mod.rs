//! Shared set-up for the T-023 hit, cover, damage and burst tests.
#![allow(dead_code)]

use battle_rules::hit::{resolve_chord, Chord, ChordResult, HitWorld, QueuedHit};
use battle_rules::{
    BattleState, Formation, Projectile, RegimentSpec, Tables, TerrainGrid, Training,
};
use core_math::{Angle, Fx, RngStreams, Vec2Fx};
use sim_state::{AgentId, SpatialGrid, WorldState};

pub const RULES_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/rules");

pub fn tables() -> Tables {
    Tables::load_dir(RULES_DIR).unwrap()
}

/// An empty battle on a field of 2048 m by 512 m whose terrain is `ch` everywhere
/// (`.` open, `W` woods).
pub fn field_of(seed: u64, ch: char) -> BattleState {
    field_with(seed, ch, tables())
}

/// [`field_of`] with the given tables.
pub fn field_with(seed: u64, ch: char, tables: Tables) -> BattleState {
    let row = ch.to_string().repeat(8);
    let rows = vec![row; 8];
    let terrain =
        TerrainGrid::from_rows(8000, 0, 0, &[('.', 0), ('W', 3)], &rows, &tables.terrain).unwrap();
    let grid = SpatialGrid::new(Fx::from_int(8), 256, 64, Vec2Fx::ZERO).unwrap();
    BattleState::new(
        WorldState::new(RngStreams::new(seed)),
        terrain,
        grid,
        tables,
    )
}

pub fn m(x: i32, y: i32) -> Vec2Fx {
    Vec2Fx::new(Fx::from_int(x), Fx::from_int(y))
}

/// Millimetres as `Fx` metres.
pub fn mm(v: i64) -> Fx {
    battle_rules::fx_ratio(v, 1000)
}

/// One soldier of regiment `unit` (its own one-man regiment) at `at`, with `flags`.
pub fn man(b: &mut BattleState, team: u8, at: Vec2Fx, flags: u16) -> AgentId {
    let id = b.spawn_regiment(RegimentSpec {
        team,
        count: 1,
        training: Training::Trained,
        formation: Formation::Line,
        anchor: at,
        heading: Angle::ZERO,
    });
    let a = b.regiments[usize::from(id.0)].roster[0];
    b.world.agents.set_flags(a, flags);
    b.grid.rebuild(&b.world.agents);
    a
}

/// A projectile of ammunition `ammo` flying east at 290 m/s level, owned by soldier `owner`.
pub fn bullet(b: &BattleState, ammo: &str, owner: u32, unit: u32, seed: u64) -> Projectile {
    Projectile {
        seq: 0,
        owner,
        unit,
        team: 0,
        ammo: b.tables.ammo.index_of(ammo).unwrap() as u8,
        pos: Vec2Fx::ZERO,
        z: Fx::ZERO,
        vel: m(290, 0),
        vz: Fx::ZERO,
        travelled_mm: 0,
        bounces: 0,
        hits: 0,
        fuse: -1,
        seed,
    }
}

/// Resolves a level chord from 20 m west of `at` to 20 m east, at height `z_mm`, along
/// the line `at.y`; the projectile flew `travelled_mm` before the chord and flies on for the
/// chord's length.
pub fn fire_through(
    b: &BattleState,
    p: &mut Projectile,
    at: Vec2Fx,
    z_mm: i64,
    travelled_mm: u32,
    speed: i32,
    out: &mut Vec<QueuedHit>,
) -> ChordResult {
    let w = HitWorld {
        agents: &b.world.agents,
        grid: &b.grid,
        obstacles: &b.obstacles,
        terrain: &b.terrain,
        tables: &b.tables,
    };
    let chord = Chord {
        p0: Vec2Fx::new(at.x - Fx::from_int(20), at.y),
        p1: Vec2Fx::new(at.x + Fx::from_int(20), at.y),
        z0: mm(z_mm),
        z1: mm(z_mm),
        travelled0: travelled_mm,
        travelled1: travelled_mm,
    };
    resolve_chord(&w, p, &chord, Fx::from_int(speed), out)
}

/// A regiment spec for a line of `count` men.
pub fn line_spec(team: u8, count: usize, at: Vec2Fx, heading: Angle) -> RegimentSpec {
    RegimentSpec {
        team,
        count,
        training: Training::Trained,
        formation: Formation::Line,
        anchor: at,
        heading,
    }
}

/// One gun firing canister at a line of 10 men 150 m east. Returns the battle, the gun's
/// regiment and the target regiment.
pub fn gun_duel(
    seed: u64,
    training: Training,
) -> (
    BattleState,
    battle_rules::RegimentId,
    battle_rules::RegimentId,
) {
    let mut b = field_of(seed, '.');
    let gun = b.spawn_regiment(RegimentSpec {
        training,
        ..line_spec(0, 1, m(100, 200), Angle::ZERO)
    });
    let targets = b.spawn_regiment(line_spec(1, 10, m(250, 200), Angle::HALF));
    b.equip_regiment(gun, "napoleon_12", 3).unwrap();
    assert!(b.set_fire_mode(gun, battle_rules::FireMode::AtWill { target: targets }));
    b.weather.wet_pm = 0;
    (b, gun, targets)
}
