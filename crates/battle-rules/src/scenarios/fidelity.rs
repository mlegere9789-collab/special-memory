//! `fid_mixed` (SPEC-008 sections 5 and 11): six regiments of 60 riflemen, three a side 90 m
//! apart on open ground, each firing at will at the regiment opposite, stepped by
//! [`BattleState::step`] while class changes are forced at ticks 100, 300, 500 and 900.

use crate::{
    hand_over, BattleState, FidelityClass, FireMode, Formation, RegimentId, RegimentSpec, Tables,
    TerrainGrid, Training,
};
use core_math::{Angle, Fx, RngStreams, Vec2Fx};
use sim_state::{SpatialGrid, WorldState};

/// The forced class changes: `(tick, regiment, class)`, ascending by tick. A regiment's class
/// from a tick on is that of its last entry at or before it, class 0 before the first.
pub const FID_MIXED_PLAN: [(u64, u16, u8); 12] = [
    (100, 1, 2),
    (100, 4, 2),
    (300, 0, 2),
    (300, 1, 0),
    (300, 5, 2),
    (500, 0, 1),
    (500, 2, 2),
    (500, 4, 1),
    (900, 1, 2),
    (900, 2, 0),
    (900, 3, 2),
    (900, 5, 1),
];

fn metres(x: i32, y: i32) -> Vec2Fx {
    Vec2Fx::new(Fx::from_int(x), Fx::from_int(y))
}

/// `fid_mixed`: regiments 0, 1, 2 (team 0) face east at x = 100 m and 3, 4, 5 (team 1) face west
/// at x = 190 m, 80 m apart in y from y = 100 m; regiment `r` fires at will at `(r + 3) % 6`.
pub fn fid_mixed(tables: &Tables, seed: u64) -> BattleState {
    let terrain =
        TerrainGrid::from_rows(8000, 0, 0, &[('.', 0)], &["........"; 8], &tables.terrain)
            .expect("scenario terrain is valid");
    let grid = SpatialGrid::new(Fx::from_int(8), 256, 64, Vec2Fx::ZERO).expect("grid size");
    let mut b = BattleState::new(
        WorldState::new(RngStreams::new(seed)),
        terrain,
        grid,
        tables.clone(),
    );
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
        b.equip_regiment(RegimentId(r), "rifle_58", 0)
            .expect("rifle_58 is a weapon row");
        let target = RegimentId((r + 3) % 6);
        b.set_fire_mode(RegimentId(r), FireMode::AtWill { target });
    }
    b
}

/// The class the plan wants of `regiment` at `tick`.
fn planned(tick: u64, regiment: u16) -> u8 {
    FID_MIXED_PLAN
        .iter()
        .filter(|p| p.0 <= tick && p.1 == regiment)
        .map(|p| p.2)
        .next_back()
        .unwrap_or(0)
}

/// Brings every regiment to the class the plan wants now, at a tick divisible by 20. A hand-over
/// refused because the regiment still owns a projectile in flight is tried again at the next
/// boundary.
pub fn drive_fid_mixed(b: &mut BattleState) {
    let tick = b.world.tick;
    if !tick.is_multiple_of(20) {
        return;
    }
    for r in 0..6u16 {
        let want = FidelityClass(planned(tick, r));
        if b.regiments[usize::from(r)].class != want {
            hand_over(b, RegimentId(r), want);
        }
    }
}

/// [`drive_fid_mixed`], then one [`BattleState::step`].
pub fn step_fid_mixed(b: &mut BattleState) {
    drive_fid_mixed(b);
    b.step();
}
