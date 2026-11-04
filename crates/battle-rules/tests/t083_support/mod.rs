//! Shared set-up for the T-083 hand-over tests: a hand-built battle of six regiments.
#![allow(dead_code)]

use battle_rules::hit::QueuedHit;
use battle_rules::{
    BattleState, FireMode, Formation, HitOutcome, RegimentId, RegimentSpec, Tables, TerrainGrid,
    Training,
};
use core_math::{Angle, Fx, RngStreams, Vec2Fx};
use sim_state::{AgentId, SpatialGrid, WorldState};

pub const RULES_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/rules");

/// An empty battle on an open field of 2048 m by 512 m.
pub fn field(seed: u64) -> BattleState {
    let tables = Tables::load_dir(RULES_DIR).unwrap();
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

/// Six regiments of 60 men with rifles: 0, 1, 2 on team 0 facing east; 3, 4, 5 on team 1
/// facing west, 90 m away. Regiment `r` fires at will at regiment `(r + 3) % 6`.
pub fn six(seed: u64) -> BattleState {
    let mut b = field(seed);
    for r in 0..6u8 {
        let (team, x, heading) = if r < 3 {
            (0, 100, Angle::ZERO)
        } else {
            (1, 190, Angle::HALF)
        };
        b.spawn_regiment(RegimentSpec {
            team,
            count: 60,
            training: Training::Trained,
            formation: Formation::Line,
            anchor: metres(x, 100 + 80 * i32::from(r % 3)),
            heading,
        });
    }
    for r in 0..6u16 {
        b.equip_regiment(RegimentId(r), "rifle_58", 0).unwrap();
        let target = RegimentId((r + 3) % 6);
        assert!(b.set_fire_mode(RegimentId(r), FireMode::AtWill { target }));
    }
    b
}

/// Queues `outcome` against `agent` for the next casualty step.
pub fn hit(b: &mut BattleState, agent: AgentId, outcome: HitOutcome) {
    b.push_hit(QueuedHit {
        target: agent,
        outcome,
        disabled_ticks: 400,
        seq: 0,
        shooter_unit: 0,
        ammo: 0,
    });
}

/// The `k`-th member of a regiment's roster.
pub fn member(b: &BattleState, r: u16, k: usize) -> AgentId {
    b.regiments[usize::from(r)].roster[k]
}
