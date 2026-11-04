//! Shared set-up for the T-022 fire-phase tests.
#![allow(dead_code)]

use battle_rules::{
    BattleState, FireMode, Formation, RegimentId, RegimentSpec, Tables, TerrainGrid, Training,
};
use core_math::{Angle, Fx, RngStreams, Vec2Fx};
use sim_state::{SpatialGrid, WorldState};

pub const RULES_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/rules");

pub fn tables() -> Tables {
    Tables::load_dir(RULES_DIR).unwrap()
}

/// An empty battle on an open field of 2048 m by 512 m.
pub fn field(seed: u64) -> BattleState {
    let tables = tables();
    let terrain =
        TerrainGrid::from_rows(8000, 0, 0, &[('.', 0)], &["........"; 8], &tables.terrain).unwrap();
    let grid = SpatialGrid::new(Fx::from_int(8), 256, 64, Vec2Fx::ZERO).unwrap();
    BattleState::new(
        WorldState::new(RngStreams::new(seed)),
        terrain,
        grid,
        tables,
    )
}

pub fn metres(x: i32, y: i32) -> Vec2Fx {
    Vec2Fx::new(Fx::from_int(x), Fx::from_int(y))
}

pub fn line(
    team: u8,
    count: usize,
    training: Training,
    at: Vec2Fx,
    heading: Angle,
) -> RegimentSpec {
    RegimentSpec {
        team,
        count,
        training,
        formation: Formation::Line,
        anchor: at,
        heading,
    }
}

/// `n` shooters of `weapon` (ammunition `ammo_sel`) facing a line of 10 targets `range_m` east.
/// The shooters fire at will. Returns the battle, the shooters and the targets.
pub fn duel(
    seed: u64,
    weapon: &str,
    ammo_sel: u8,
    training: Training,
    n: usize,
    range_m: i32,
) -> (BattleState, RegimentId, RegimentId) {
    let mut b = field(seed);
    let shooters = b.spawn_regiment(line(0, n, training, metres(100, 200), Angle::ZERO));
    let targets = b.spawn_regiment(line(
        1,
        10,
        Training::Trained,
        metres(100 + range_m, 200),
        Angle::HALF,
    ));
    b.equip_regiment(shooters, weapon, ammo_sel).unwrap();
    assert!(b.set_fire_mode(shooters, FireMode::AtWill { target: targets }));
    (b, shooters, targets)
}

/// The fire phase, the smoke decay and the tick counter (no movement, no flight).
pub fn tick(b: &mut BattleState) {
    b.step_fire();
    b.step_smoke();
    b.world.advance_tick();
}

/// Weather in which no round misfires.
pub fn no_misfire(b: &mut BattleState) {
    b.weather.wet_pm = 0;
}
