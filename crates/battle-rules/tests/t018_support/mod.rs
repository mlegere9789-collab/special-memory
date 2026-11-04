//! Shared set-up for the T-018 tests.
#![allow(dead_code)]

use battle_rules::{
    BattleState, Formation, RegimentId, RegimentSpec, Tables, TerrainGrid, Training,
};
use core_math::{Angle, Fx, RngStreams, Vec2Fx};
use sim_state::{SpatialGrid, WorldState};

pub const RULES_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/rules");

pub fn tables() -> Tables {
    Tables::load_dir(RULES_DIR).unwrap()
}

/// An empty battle on a 64 m by 64 m open field.
pub fn empty_battle(seed: u64) -> BattleState {
    let tables = tables();
    let terrain =
        TerrainGrid::from_rows(8000, 0, 0, &[('.', 0)], &["........"; 8], &tables.terrain).unwrap();
    let grid = SpatialGrid::new(Fx::from_int(8), 64, 64, Vec2Fx::ZERO).unwrap();
    BattleState::new(
        WorldState::new(RngStreams::new(seed)),
        terrain,
        grid,
        tables,
    )
}

pub fn spec(count: usize, training: Training, formation: Formation) -> RegimentSpec {
    RegimentSpec {
        team: 1,
        count,
        training,
        formation,
        anchor: Vec2Fx::new(Fx::from_int(100), Fx::from_int(200)),
        heading: Angle::ZERO,
    }
}

pub fn battle_with(
    count: usize,
    training: Training,
    formation: Formation,
) -> (BattleState, RegimentId) {
    let mut b = empty_battle(1);
    let id = b.spawn_regiment(spec(count, training, formation));
    (b, id)
}

/// One full step: the drill machine, then the tick counter.
pub fn step(b: &mut BattleState) {
    b.step_drill();
    b.world.advance_tick();
}
