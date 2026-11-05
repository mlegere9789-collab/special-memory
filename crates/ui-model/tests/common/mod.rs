//! Shared fixtures: a small deterministic battle and canned information and morale sources.
#![allow(dead_code)]

use battle_rules::{
    embedded_rules, BattleState, Formation, Order, RegimentSpec, TerrainGrid, Training,
};
use core_math::{Angle, Fx, RngStreams, Vec2Fx};
use sim_state::{SpatialGrid, WorldState};
use ui_model::{InfoSource, MoraleReport, MoraleSource, OrderRecord, Sighting};

pub fn metres(x: i32, y: i32) -> Vec2Fx {
    Vec2Fx::new(Fx::from_int(x), Fx::from_int(y))
}

/// An empty battle on 512 m by 512 m of open ground.
pub fn empty_battle(seed: u64) -> BattleState {
    let (tables, _) = embedded_rules().expect("embedded rules load");
    let terrain = TerrainGrid::from_rows(8000, 0, 0, &[('.', 0)], &["."], &tables.terrain)
        .expect("terrain is valid");
    let grid =
        SpatialGrid::new(Fx::from_int(2), 256, 256, metres(-64, -64)).expect("grid is valid");
    BattleState::new(
        WorldState::new(RngStreams::new(seed)),
        terrain,
        grid,
        tables,
    )
}

/// `(team, count, formation, x, y)` per regiment, each starting with a march order east.
pub fn battle(seed: u64, regs: &[(u8, usize, Formation, i32, i32)]) -> BattleState {
    let mut b = empty_battle(seed);
    for &(team, count, formation, x, y) in regs {
        let id = b.spawn_regiment(RegimentSpec {
            team,
            count,
            training: Training::Trained,
            formation,
            anchor: metres(x, y),
            heading: Angle::ZERO,
        });
        let to = metres(x + 100, y);
        b.issue_order(
            id,
            Order::Move {
                to,
                gait: battle_rules::Gait::Quick,
            },
        );
    }
    b
}

/// A morale source that reports the same canned values for every regiment.
pub struct CannedMorale(pub MoraleReport);

impl MoraleSource for CannedMorale {
    fn report(&self, _r: battle_rules::RegimentId) -> MoraleReport {
        self.0.clone()
    }
}

pub fn steady() -> MoraleReport {
    MoraleReport {
        level: 0,
        margin: 40,
        trend: 0,
        factors: vec![(1, 12), (2, -3)],
    }
}

/// An information source with fixed sightings and orders.
pub struct Canned {
    pub sightings: Vec<Sighting>,
    pub orders: Vec<OrderRecord>,
}

impl InfoSource for Canned {
    fn sightings(&self, _side: u8, _now: u64, out: &mut Vec<Sighting>) {
        out.extend(self.sightings.iter().copied());
    }
    fn orders(&self, _side: u8, out: &mut Vec<OrderRecord>) {
        out.extend(self.orders.iter().cloned());
    }
    fn eta_window(&self, _r: battle_rules::RegimentId, _o: &Order, now: u64) -> (u64, u64) {
        (now + 10, now + 20)
    }
}
