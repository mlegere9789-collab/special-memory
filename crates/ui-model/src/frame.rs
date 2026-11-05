//! Per-soldier columns for the renderer (SPEC-015 section 3, item 2).

use battle_rules::BattleState;
use core_math::{Angle, Vec2Fx};

/// Soldier columns copied in agent slot order; reused between calls to avoid allocation.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SoldierFrame {
    /// Positions, metres.
    pub pos: Vec<Vec2Fx>,
    /// Facing angles.
    pub facing: Vec<Angle>,
    /// Flag bits.
    pub flags: Vec<u16>,
    /// Unit (regiment id) of each soldier.
    pub unit: Vec<u32>,
}

/// Copies `pos`, `facing`, `flags` and `unit` of every live soldier in slot order into `out`.
pub fn soldier_frame(state: &BattleState, out: &mut SoldierFrame) {
    out.pos.clear();
    out.facing.clear();
    out.flags.clear();
    out.unit.clear();
    let agents = &state.world.agents;
    for id in agents.iter_alive() {
        out.pos.push(agents.pos(id).unwrap_or_default());
        out.facing.push(agents.facing(id).unwrap_or_default());
        out.flags.push(agents.flags(id).unwrap_or_default());
        out.unit.push(agents.unit(id).unwrap_or_default());
    }
}
