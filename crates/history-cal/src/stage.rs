//! The battle a script runs on until the SPEC-008 scenario hands over its own order of battle
//! (SPEC-011 amendment A1): every regiment of `units.toml` on open ground at its anchor, in
//! ascending unit id so that the regiment id is the rank among regiments.

use crate::script::OrderKind;
use crate::units::UnitRecord;
use crate::{ReplayScript, ScriptError};
use battle_rules::{BattleState, DataError, RegimentSpec, Tables, TerrainGrid};
use core_math::{Angle, Fx, RngStreams, Vec2Fx};
use sim_state::{SpatialGrid, WorldState};

/// Free ground in metres around every anchor and order target.
const MARGIN_M: i64 = 256;
/// Spatial-index cell side in metres.
const CELL_M: i64 = 8;
/// Terrain cell side in millimetres (one open cell covers the field).
const TERRAIN_CELL_MM: i64 = 8000;

fn metres(x: i64, y: i64) -> Result<Vec2Fx, ScriptError> {
    let fx = |v: i64| i32::try_from(v).map(Fx::from_int);
    match (fx(x), fx(y)) {
        (Ok(x), Ok(y)) => Ok(Vec2Fx::new(x, y)),
        _ => Err(DataError::Range(format!("position ({x}, {y}) m")).into()),
    }
}

/// Builds the tick-0 battle for `script`: open ground covering every regiment anchor and every
/// `move` target with a margin, and one regiment per unit of level regiment or battery.
///
/// Errors: [`ScriptError::Data`] for a position beyond the exact `Fx` range or a field larger than
/// the spatial index allows.
pub fn stage_battle(
    units: &[UnitRecord],
    script: &ReplayScript,
    tables: &Tables,
    seed: u64,
) -> Result<BattleState, ScriptError> {
    let mut regs: Vec<&UnitRecord> = units.iter().filter(|u| u.level.is_regiment()).collect();
    regs.sort_by(|a, b| a.id.cmp(&b.id));
    let mut points: Vec<(i64, i64)> = regs.iter().map(|u| (u.x_m, u.y_m)).collect();
    let span = {
        let xs = points.iter().map(|p| p.0);
        let ys = points.iter().map(|p| p.1);
        let range = |it: &mut dyn Iterator<Item = i64>| {
            let v: Vec<i64> = it.collect();
            v.iter().max().copied().unwrap_or(0) - v.iter().min().copied().unwrap_or(0)
        };
        range(&mut xs.clone()).max(range(&mut ys.clone()))
    };
    for row in &script.orders {
        if let OrderKind::Move { x_m, y_m, .. } = row.kind {
            points.push((x_m, y_m));
        }
    }
    let pad = MARGIN_M + span;
    let lo = |f: fn(&(i64, i64)) -> i64| points.iter().map(f).min().unwrap_or(0) - pad;
    let hi = |f: fn(&(i64, i64)) -> i64| points.iter().map(f).max().unwrap_or(0) + pad;
    let (x0, y0) = (lo(|p| p.0), lo(|p| p.1));
    let cells = |a: i64, b: i64| u32::try_from((b - a + CELL_M - 1) / CELL_M);
    let (w, h) = match (cells(x0, hi(|p| p.0)), cells(y0, hi(|p| p.1))) {
        (Ok(w), Ok(h)) => (w, h),
        _ => return Err(DataError::Range("field size".into()).into()),
    };
    let terrain =
        TerrainGrid::from_rows(TERRAIN_CELL_MM, 0, 0, &[('.', 0)], &["."], &tables.terrain)?;
    let grid = SpatialGrid::new(Fx::from_int(CELL_M as i32), w, h, metres(x0, y0)?)
        .map_err(|e| DataError::Range(format!("spatial index: {e:?}")))?;
    let mut state = BattleState::new(
        WorldState::new(RngStreams::new(seed)),
        terrain,
        grid,
        tables.clone(),
    );
    for u in regs {
        state.spawn_regiment(RegimentSpec {
            team: u.side,
            count: usize::from(u.strength),
            training: u.training,
            formation: u.formation,
            anchor: metres(u.x_m, u.y_m)?,
            heading: Angle::ZERO,
        });
    }
    Ok(state)
}
