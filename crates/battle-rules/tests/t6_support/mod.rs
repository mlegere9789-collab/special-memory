//! Shared set-up for the SPEC-006 input and ceiling tests.
#![allow(dead_code)]

use battle_rules::{
    BattleState, Formation, RegimentId, RegimentSpec, Tables, TerrainGrid, Training,
};
use core_math::{Angle, Fx, RngStreams, Vec2Fx};
use sim_state::{SpatialGrid, WorldState};

pub const RULES_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/rules");

/// An empty battle whose terrain is `ch` everywhere (`.` open, `W` woods).
pub fn field(seed: u64, ch: char) -> BattleState {
    let tables = Tables::load_dir(RULES_DIR).unwrap();
    let rows = vec![ch.to_string().repeat(8); 8];
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

/// Spawns a regiment of `count` men facing `heading`.
pub fn spawn(
    b: &mut BattleState,
    team: u8,
    count: usize,
    training: Training,
    formation: Formation,
    at: Vec2Fx,
    heading: Angle,
) -> RegimentId {
    b.spawn_regiment(RegimentSpec {
        team,
        count,
        training,
        formation,
        anchor: at,
        heading,
    })
}

/// A line of `count` trained men at `at` facing east (+x).
pub fn line(b: &mut BattleState, team: u8, count: usize, at: Vec2Fx) -> RegimentId {
    spawn(
        b,
        team,
        count,
        Training::Trained,
        Formation::Line,
        at,
        Angle::ZERO,
    )
}
