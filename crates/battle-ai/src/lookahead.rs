//! Lookahead (SPEC-010 section 7): the belief world, the forecast, the score and the budget.
//!
//! This is the one file of the crate that names the world state (T10-04). A belief is built
//! only from a [`BeliefView`]; a forecast clones it, applies orders and steps the real rules, so
//! the crate holds no closed-form combat estimate. Weights and bounds below are placeholders
//! with the calibration target CT-010-5.

use crate::belief::{dist_sq, noised, radius_raw, Area, BeliefView, Frame};
use crate::port::{ContactId, UnitRef};
use battle_rules::{gauss_unit, soldier_stream, Training};
use battle_rules::{BattleState, FireMode, Formation, Gait, Order, RegimentId, RegimentSpec};
use core_math::{Angle, Fx, Rng, RngStreams, SplitMix64, Vec2Fx};
use sim_state::{SpatialGrid, WorldState};

pub use crate::belief::EST_LATENCY_TICKS;

/// Shortest horizon of a forecast, ticks.
pub const HORIZON_MIN: u32 = 100;
/// Longest horizon of a forecast, ticks.
pub const HORIZON_MAX: u32 = 1200;
/// A tactical pass uses this fraction of the budget (one in `TL_BUDGET_DIV`).
pub const TL_BUDGET_DIV: u64 = 10;
/// A tactical pass weighs at most this many candidates.
pub const TL_CANDIDATES: u32 = 2;

/// What a budget buys: the candidates and samples that remain and the horizon they run for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BudgetPlan {
    /// Candidates forecast.
    pub candidates: u32,
    /// Samples per candidate.
    pub samples: u32,
    /// Ticks each forecast runs.
    pub horizon: u32,
}

/// `horizon = clamp(budget / (candidates * samples * soldiers), 100, 1200)`, floor division. If
/// the quotient is under 100, samples are dropped one at a time down to 1, then candidates
/// down to 1, from the end of the list; at `1 x 1` the horizon is clamped to 100 even though it
/// does not fit. Cost is in soldier-ticks, never time. `soldiers` of 0 counts as 1.
pub fn plan_budget(budget: u64, candidates: u32, samples: u32, soldiers: u32) -> BudgetPlan {
    let (mut c, mut s) = (candidates.max(1), samples.max(1));
    let n = u128::from(soldiers.max(1));
    let quotient = |c: u32, s: u32| u128::from(budget) / (u128::from(c) * u128::from(s) * n);
    while quotient(c, s) < u128::from(HORIZON_MIN) && (s > 1 || c > 1) {
        if s > 1 {
            s -= 1;
        } else {
            c -= 1;
        }
    }
    let h = quotient(c, s).clamp(u128::from(HORIZON_MIN), u128::from(HORIZON_MAX));
    BudgetPlan {
        candidates: c,
        samples: s,
        horizon: u32::try_from(h).unwrap_or(HORIZON_MAX),
    }
}

/// [`plan_budget`] for a tactical pass: a tenth of the budget and at most two candidates.
pub fn tactical_budget(budget: u64, candidates: u32, samples: u32, soldiers: u32) -> BudgetPlan {
    plan_budget(
        budget / TL_BUDGET_DIV,
        candidates.min(TL_CANDIDATES),
        samples,
        soldiers,
    )
}

/// The orders of one candidate plan, applied [`EST_LATENCY_TICKS`] into the forecast.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Script {
    /// In application order.
    pub items: Vec<ScriptItem>,
}

/// One order of a [`Script`]; units and contacts are named as the port names them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScriptItem {
    /// A regiment order to an own unit.
    Order {
        /// The unit.
        unit: UnitRef,
        /// The order.
        order: Order,
    },
    /// Fire at the regiment behind a contact, or hold fire with `None`.
    Fire {
        /// The unit.
        unit: UnitRef,
        /// The contact fired on.
        target: Option<ContactId>,
    },
}

/// What the enemy is assumed to do during a forecast. Each enemy fires at will on its nearest
/// own regiment (lowest id on a tie).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnemyModel {
    /// Keep its track: move to where the last two sightings lead over the horizon; with no
    /// track it holds.
    Continue,
    /// March on the nearest own regiment.
    Advance,
    /// Stand where it is.
    Hold,
}

impl EnemyModel {
    /// The model of sample `i`: `Continue`, `Advance`, `Hold`, then again in turn.
    pub const fn of_sample(i: u32) -> EnemyModel {
        match i % 3 {
            0 => EnemyModel::Continue,
            1 => EnemyModel::Advance,
            _ => EnemyModel::Hold,
        }
    }
}

/// The outcome of one forecast.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ForecastResult {
    /// Own soldiers no longer fit at the end.
    pub own_lost: u64,
    /// Enemy soldiers no longer fit at the end.
    pub enemy_lost: u64,
    /// Own share of the soldiers inside the objective area at the end, permille; 0 when nobody
    /// is inside.
    pub ctrl_pm: u16,
    /// Hash of the forecast world after the last tick.
    pub final_hash: u64,
    /// Own soldiers at the start.
    pub own_start: u64,
    /// Enemy soldiers at the start.
    pub enemy_start: u64,
}

/// Builds the belief world from the picture (SPEC-010 section 7). The master seed comes from
/// `rng` (the forecast stream) through SplitMix64, so live streams are never touched. Own
/// regiments come from reports (strength, rounds, fatigue by the public setters); contacts
/// become enemy regiments at a noised strength (`noise_pm` times a standard-normal draw from
/// `rng`), `formation_est` or `Line`, the estimated weapon or the side's majority weapon,
/// `Trained`, facing the objective. Regiments are in [`Frame`] order. A weapon name the tables
/// do not know leaves that regiment unequipped.
pub fn build_belief(view: &BeliefView<'_>, rng: &mut Rng, noise_pm: u16) -> BattleState {
    let seed = SplitMix64::new(rng.next_u64()).next_u64();
    let g = view.grid;
    let cells = (g.width.clamp(1, 4096), g.height.clamp(1, 4096));
    let grid = SpatialGrid::new(Fx::from_int(g.cell_m.max(1)), cells.0, cells.1, g.origin)
        .expect("a clamped grid spec is valid");
    let mut b = BattleState::new(
        WorldState::new(RngStreams::new(seed)),
        view.map.terrain.clone(),
        grid,
        view.tables.clone(),
    );
    b.obstacles = view.map.obstacles.clone();
    b.world.tick = view.now;
    let frame = Frame::of(view);
    for r in &frame.own {
        let id = b.spawn_regiment(RegimentSpec {
            team: frame.side,
            count: usize::from(r.strength),
            training: Training::Trained,
            formation: r.formation,
            anchor: r.pos,
            heading: r.heading,
        });
        let _ = b.equip_regiment(id, view.own_weapon, 0);
        b.set_fatigue(id, u32::from(r.fatigue_pm) * 1000);
        b.set_rounds_pm(id, r.rounds_pm);
    }
    for f in &frame.foes {
        let c = &f.contact;
        let strength = noised(c.strength_est, noise_pm, gauss_unit(rng));
        let weapon = c
            .weapon_est
            .and_then(|w| view.tables.weapons.rows().get(usize::from(w)))
            .map_or(view.enemy_weapon, |row| row.id.as_str());
        let id = b.spawn_regiment(RegimentSpec {
            team: frame.side ^ 1,
            count: usize::from(strength),
            training: Training::Trained,
            formation: c.formation_est.unwrap_or(Formation::Line),
            anchor: c.pos,
            heading: Angle::atan2(view.area.center.y - c.pos.y, view.area.center.x - c.pos.x),
        });
        let _ = b.equip_regiment(id, weapon, 0);
    }
    b
}

/// Index of the regiment of another team nearest to `of`, lowest index on a tie.
fn nearest(b: &BattleState, of: usize) -> Option<usize> {
    let me = b.regiments[of].anchor;
    let team = b.regiments[of].team;
    b.regiments
        .iter()
        .filter(|r| r.team != team)
        .min_by_key(|r| (dist_sq(r.anchor, me), r.id))
        .map(|r| usize::from(r.id.0))
}

/// Gives every enemy its model's order and its fire mode at tick 0 of a forecast.
fn apply_enemy_model(b: &mut BattleState, frame: &Frame, model: EnemyModel, horizon: u32) {
    for (j, foe) in frame.foes.iter().enumerate() {
        let me = frame.own.len() + j;
        let id = RegimentId(u16::try_from(me).unwrap_or(u16::MAX));
        let Some(near) = nearest(b, me) else {
            continue;
        };
        let order = match model {
            EnemyModel::Hold => Order::Hold,
            EnemyModel::Advance => Order::Move {
                to: b.regiments[near].anchor,
                gait: Gait::Quick,
            },
            EnemyModel::Continue if foe.vel == Vec2Fx::ZERO => Order::Hold,
            EnemyModel::Continue => {
                let h = i64::from(horizon);
                Order::Move {
                    to: Vec2Fx::new(
                        Fx::from_raw(foe.contact.pos.x.raw() + foe.vel.x.raw() * h),
                        Fx::from_raw(foe.contact.pos.y.raw() + foe.vel.y.raw() * h),
                    ),
                    gait: Gait::Quick,
                }
            }
        };
        b.issue_order(id, order);
        let target = RegimentId(u16::try_from(near).unwrap_or(u16::MAX));
        b.set_fire_mode(id, FireMode::AtWill { target });
    }
}

/// Applies the script of the own side through `issue_order` and `set_fire_mode`.
fn apply_script(b: &mut BattleState, frame: &Frame, script: &Script) {
    let reg = |i: usize| RegimentId(u16::try_from(i).unwrap_or(u16::MAX));
    for item in &script.items {
        match *item {
            ScriptItem::Order { unit, order } => {
                if let Some(i) = frame.own_index(unit) {
                    b.issue_order(reg(i), order);
                }
            }
            ScriptItem::Fire { unit, target } => {
                let Some(i) = frame.own_index(unit) else {
                    continue;
                };
                let mode = match target.and_then(|c| frame.foe_index(c)) {
                    Some(t) => FireMode::AtWill { target: reg(t) },
                    None => FireMode::HoldFire,
                };
                b.set_fire_mode(reg(i), mode);
            }
        }
    }
}

/// Fit soldiers of a team, and of that team inside `area`.
fn census(b: &BattleState, team: u8, area: &Area) -> (u64, u64) {
    let r2 = radius_raw(area.radius_mm).pow(2);
    let (mut all, mut inside) = (0u64, 0u64);
    for r in b.regiments.iter().filter(|r| r.team == team) {
        all += u64::from(b.strength(r.id).unwrap_or(0));
        inside += r
            .roster
            .iter()
            .filter_map(|a| b.world.agents.pos(*a))
            .filter(|p| dist_sq(*p, area.center) <= r2)
            .count() as u64;
    }
    (all, inside)
}

/// Runs the script on a clone of `belief` for `horizon` ticks of the real step (class 0, which
/// the size bound of the belief makes affordable) and reports the outcome. The enemy model's
/// orders are given at tick 0, the script's at [`EST_LATENCY_TICKS`]. With `seed` of
/// `Some(s)` the master seed and every soldier's fire stream are re-derived from `s` (loading
/// phases stay as they are), which gives a further sample; `None` keeps the belief's streams, so a belief equal to truth
/// forecasts the played run exactly (T10-07). The belief is not touched.
pub fn forecast(
    belief: &BattleState,
    frame: &Frame,
    script: &Script,
    horizon: u32,
    model: EnemyModel,
    seed: Option<u64>,
) -> ForecastResult {
    let mut b = belief.clone();
    if let Some(s) = seed {
        b.world.rng = RngStreams::new(s);
        for r in &b.regiments {
            for a in &r.roster {
                if let Some(f) = b.fire.get_mut(a.index as usize) {
                    f.rng = b.world.rng.stream(soldier_stream(a.index)).state();
                }
            }
        }
    }
    let (own_start, _) = census(&b, frame.side, &frame.area);
    let (enemy_start, _) = census(&b, frame.side ^ 1, &frame.area);
    apply_enemy_model(&mut b, frame, model, horizon);
    for tick in 0..u64::from(horizon) {
        if tick == EST_LATENCY_TICKS {
            apply_script(&mut b, frame, script);
        }
        b.step();
    }
    let (own_end, own_in) = census(&b, frame.side, &frame.area);
    let (enemy_end, enemy_in) = census(&b, frame.side ^ 1, &frame.area);
    ForecastResult {
        own_lost: own_start - own_end.min(own_start),
        enemy_lost: enemy_start - enemy_end.min(enemy_start),
        ctrl_pm: u16::try_from((own_in * 1000).checked_div(own_in + enemy_in).unwrap_or(0))
            .unwrap_or(1000),
        final_hash: b.hash(),
        own_start,
        enemy_start,
    }
}

/// `ctrl_pm + enemy_lost * 1000 / enemy_start - own_lost * 1000 / own_start * (2000 - risk) / 1000`,
/// each division truncating; a side that started with no soldiers loses nothing.
pub fn score_sample(r: &ForecastResult, risk_tolerance: u16) -> i64 {
    let share =
        |lost: u64, start: u64| i64::try_from(lost * 1000).map_or(0, |v| v / start.max(1) as i64);
    let risk = i64::from(risk_tolerance.min(1000));
    i64::from(r.ctrl_pm) + share(r.enemy_lost, r.enemy_start)
        - share(r.own_lost, r.own_start) * (2000 - risk) / 1000
}

/// `mean - (mean - min) * (1000 - risk) / 1000`, truncating; 0 for no samples.
pub fn aggregate_scores(scores: &[i64], risk_tolerance: u16) -> i64 {
    let Some(min) = scores.iter().copied().min() else {
        return 0;
    };
    let mean = scores.iter().sum::<i64>() / scores.len() as i64;
    mean - (mean - min) * (1000 - i64::from(risk_tolerance.min(1000))) / 1000
}
