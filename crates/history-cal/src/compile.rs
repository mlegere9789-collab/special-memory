//! Script compiler (SPEC-011 section 4.1 and amendment A1): orders and triggers become replay
//! inputs of actor `0xFD`, sorted by `(tick, seq)`.

use crate::script::{OrderKind, OrderRow};
use crate::units::{UnitRecord, COORD_LIMIT_M};
use crate::{ReplayScript, ScriptError};
use battle_rules::registry::{TAG_SET_FIRE_MODE, TAG_TRIGGER};
use battle_rules::{encode_order, DataError, Order, RegimentId};
use core_math::{Angle, Fx, Vec2Fx};

/// Most inputs one tick may hold: the `u16` sequence number counts them from 0.
const MAX_PER_TICK: usize = 65_535;

/// One compiled input; the actor is always `0xFD` (`battle_rules::registry::ACTOR_HISTORY_SCRIPT`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptInput {
    /// Tick at which the input is applied.
    pub tick: u64,
    /// Sequence number within the tick, from 0.
    pub seq: u16,
    /// Payload bytes, first byte the tag.
    pub payload: Vec<u8>,
}

/// The units sorted by id, with the regiment numbering of the scenario rule.
struct Index<'a> {
    units: Vec<&'a UnitRecord>,
    /// Indices into `units` of the regiments, ascending id; the position is the `RegimentId`.
    regiments: Vec<usize>,
}

impl<'a> Index<'a> {
    fn new(records: &'a [UnitRecord]) -> Index<'a> {
        let mut units: Vec<&UnitRecord> = records.iter().collect();
        units.sort_by(|a, b| a.id.cmp(&b.id));
        let regiments = (0..units.len())
            .filter(|&i| units[i].level.is_regiment())
            .collect();
        Index { units, regiments }
    }

    fn find(&self, id: &str) -> Result<usize, ScriptError> {
        self.units
            .binary_search_by(|u| u.id.as_str().cmp(id))
            .map_err(|_| ScriptError::Reference(format!("unit {id}")))
    }

    fn regiment_id(&self, unit: usize) -> Option<u16> {
        self.regiments
            .binary_search(&unit)
            .ok()
            .and_then(|r| u16::try_from(r).ok())
    }

    /// Regiments at or below `unit`, ascending id.
    fn regiments_under(&self, unit: usize) -> Vec<usize> {
        let target = &self.units[unit].id;
        let under = |mut at: usize| loop {
            if &self.units[at].id == target {
                return true;
            }
            match self.find(&self.units[at].parent) {
                Ok(p) => at = p,
                Err(_) => return false,
            }
        };
        self.regiments
            .iter()
            .copied()
            .filter(|&r| under(r))
            .collect()
    }
}

fn fx(v: i64) -> Result<Fx, ScriptError> {
    if v.abs() > COORD_LIMIT_M {
        return Err(DataError::Range(format!("position {v} m beyond {COORD_LIMIT_M}")).into());
    }
    Ok(Fx::from_int(v as i32))
}

fn order_inputs(ix: &Index, row: &OrderRow) -> Result<Vec<Vec<u8>>, ScriptError> {
    let unit = ix.find(&row.unit)?;
    let targets = ix.regiments_under(unit);
    if targets.is_empty() {
        return Err(ScriptError::Reference(format!(
            "unit {} has no regiments",
            row.unit
        )));
    }
    let count = targets.len() as i64;
    let sum = |f: fn(&UnitRecord) -> i64| targets.iter().map(|&r| f(ix.units[r])).sum::<i64>();
    let anchor = (
        sum(|u| u.x_m).div_euclid(count),
        sum(|u| u.y_m).div_euclid(count),
    );
    let mut out = Vec::new();
    for &r in &targets {
        let id = RegimentId(ix.regiment_id(r).unwrap_or(0));
        let u = ix.units[r];
        let order = match &row.kind {
            OrderKind::Hold => Order::Hold,
            OrderKind::Move { x_m, y_m, gait } => {
                // A regiment keeps its offset from the parent's anchor; a lone regiment has none.
                let (dx, dy) = if unit == r {
                    (0, 0)
                } else {
                    (u.x_m - anchor.0, u.y_m - anchor.1)
                };
                Order::Move {
                    to: Vec2Fx::new(fx(x_m + dx)?, fx(y_m + dy)?),
                    gait: *gait,
                }
            }
            OrderKind::Form(f) => Order::Form(*f),
            OrderKind::Face { heading_deg } => {
                Order::Face(Angle::from_degrees_int(i32::from(*heading_deg)))
            }
            OrderKind::FireAt { target_unit } => {
                let t = ix.find(target_unit)?;
                let target = ix.regiment_id(t).ok_or_else(|| {
                    ScriptError::Reference(format!(
                        "fire_at target {target_unit} is not a regiment"
                    ))
                })?;
                let mut p = vec![TAG_SET_FIRE_MODE];
                p.extend_from_slice(&id.0.to_le_bytes());
                p.extend_from_slice(&target.to_le_bytes());
                out.push(p);
                continue;
            }
        };
        out.push(encode_order(id, &order));
    }
    Ok(out)
}

/// Compiles a script against its units into inputs sorted by `(tick, seq)`.
///
/// Within a tick the inputs of orders come first in file order (a parent order expanding to its
/// regiments in ascending regiment id), then triggers in ascending id; `seq` counts from 0.
///
/// Errors: [`ScriptError::Reference`] for a unit that is absent, a parent with no regiments or a
/// `fire_at` target that is not a regiment; [`ScriptError::TooManyInputs`] above 65,535 inputs in
/// one tick; [`ScriptError::Data`] for a position beyond 1,000,000 m.
pub fn compile_script(
    script: &ReplayScript,
    units: &[UnitRecord],
) -> Result<Vec<ScriptInput>, ScriptError> {
    let ix = Index::new(units);
    let mut staged: Vec<(u64, Vec<u8>)> = Vec::new();
    for row in &script.orders {
        let tick = script.meta.tick_of(row.at_s);
        staged.extend(order_inputs(&ix, row)?.into_iter().map(|p| (tick, p)));
    }
    for (i, trig) in script.triggers.iter().enumerate() {
        let index = u16::try_from(i).map_err(|_| ScriptError::TooManyInputs { tick: 0 })?;
        let mut p = vec![TAG_TRIGGER];
        p.extend_from_slice(&index.to_le_bytes());
        staged.push((script.meta.tick_of(trig.at_s), p));
    }
    // A stable sort keeps the staging order within a tick.
    staged.sort_by_key(|(tick, _)| *tick);
    let mut out: Vec<ScriptInput> = Vec::with_capacity(staged.len());
    let mut n = 0usize;
    for (tick, payload) in staged {
        n = if out.last().is_some_and(|p| p.tick == tick) {
            n + 1
        } else {
            0
        };
        if n >= MAX_PER_TICK {
            return Err(ScriptError::TooManyInputs { tick });
        }
        out.push(ScriptInput {
            tick,
            seq: n as u16,
            payload,
        });
    }
    Ok(out)
}
