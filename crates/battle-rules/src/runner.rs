//! The `Battle` wrapper (SPEC-008 sections 4, 5, 8, 9): a [`BattleState`] under the fidelity
//! scheduler, built from a [`Scenario`].
//!
//! [`Battle::step`] follows the tick order of REGISTRY section 6. On a **boundary** (`tick %
//! assign_interval_ticks == 0`) and before movement it applies departures, arrivals, triggers
//! and the weather row for this tick, classifies every present regiment, hands regiments over
//! between classes in ascending id, and updates the sector states; then it runs
//! [`BattleState::step`]. Nothing reads anything but state, so [`Battle::run`] and a played run
//! of the same inputs agree.
//!
//! Until the F1 bundle (T-085) and the F2 exchange (T-086) land, a class 1 regiment shoots by the
//! SPEC-005 rule like class 0, and a class 2 regiment is an aggregate that neither fires nor
//! moves: the steps TK-08 and TK-09 are absent from the list.

mod boundary;
mod embedded;
mod fidhash;

use crate::battle_hash::battle_hash_into;
use crate::fidelity::anchor_mm;
use crate::{
    fx_ratio, Arm, BattleState, FidelityTable, Ledger, Objective, RegimentStatus, Scenario,
    ScenarioError, SectorMap, Side, SmokeGrid, Tables, F2, F_DISABLED, F_WOUNDED,
};
use core_math::{RngStreams, Vec2Fx};
use sim_state::{SpatialGrid, StateHasher, WorldState};

pub use embedded::embedded_rules;

/// The scheduler's state next to the battle (SPEC-008 section 4). Every field except `table` and
/// `reg_unit` is hashed in the FID1 block.
#[derive(Clone, Debug)]
pub struct FidelityState {
    /// The loaded `fidelity.toml`.
    pub table: FidelityTable,
    /// Sector states, recomputed at each boundary.
    pub sectors: SectorMap,
    /// Arrivals applied so far (a cursor into `Scenario::arrivals`).
    pub arrivals_done: usize,
    /// Departures applied so far.
    pub departures_done: usize,
    /// Triggers applied so far.
    pub triggers_done: usize,
    /// Index of the weather row now in force.
    pub weather_row: Option<usize>,
    /// Soldiers that arrived, by team.
    pub arrived: [u64; 2],
    /// Soldiers that departed, by team.
    pub departed: [u64; 2],
    /// Index into `Scenario::units` of the unit behind each regiment.
    pub(crate) reg_unit: Vec<usize>,
}

/// The soldier and ammunition identities of one team (SPEC-008 section 5).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BattleLedger {
    /// Present, killed and the round account (`held + spent + lost == initial + resupplied`).
    pub ledger: Ledger,
    /// Soldiers that arrived (`present_start + arrived == present + killed + departed`).
    pub arrived: u64,
    /// Soldiers that left the field with their regiment.
    pub departed: u64,
}

/// A battle under the fidelity scheduler.
#[derive(Clone, Debug)]
pub struct Battle {
    /// Agents, regiments and every SPEC-004 and SPEC-005 rule.
    pub state: BattleState,
    /// The scheduler.
    pub fid: FidelityState,
    /// The scenario that built it.
    pub scenario: Scenario,
}

/// Cells of side `cell_mm` needed to cover `extent_mm`.
fn cells(extent_mm: i64, cell_mm: i64) -> u32 {
    u32::try_from((extent_mm + cell_mm - 1) / cell_mm).unwrap_or(u32::MAX)
}

/// The cell size at or above `start_mm` for which a map of `w` by `h` mm needs at most `max`
/// cells (the size doubles until it fits, SPEC-008 section 7).
fn fitted_cell(w: i64, h: i64, start_mm: i64, max: u64) -> i64 {
    let mut cell = start_mm.max(1);
    while u64::from(cells(w, cell)) * u64::from(cells(h, cell)) > max {
        cell *= 2;
    }
    cell
}

fn metres(mm: i64) -> core_math::Fx {
    fx_ratio(mm, 1000)
}

fn range_err(what: &str) -> ScenarioError {
    ScenarioError::Range(what.to_owned())
}

impl Battle {
    /// Builds the battle of `scenario` with the rules compiled into the crate (the files of
    /// `data/rules`), at tick 0, seeded with `seed`.
    ///
    /// Errors: [`ScenarioError::Reference`] for an unknown weapon, [`ScenarioError::Range`] for a
    /// map that no grid covers, [`ScenarioError::Data`] for a rules file that does not load.
    pub fn from_scenario(scenario: &Scenario, seed: u64) -> Result<Battle, ScenarioError> {
        let (tables, fidelity) = embedded_rules()?;
        Battle::from_scenario_with(scenario, seed, tables, fidelity)
    }

    /// [`Battle::from_scenario`] with rules the caller loaded (for example from a data
    /// directory).
    ///
    /// Every non-headquarters unit becomes a regiment in id order (SPEC-008 section 3), equipped
    /// and not yet arrived; the agent grid and the smoke grid cover the map at the cell size of
    /// the table, doubled until the cell count fits `max_grid_cells`.
    pub fn from_scenario_with(
        scenario: &Scenario,
        seed: u64,
        tables: Tables,
        fidelity: FidelityTable,
    ) -> Result<Battle, ScenarioError> {
        scenario.check_weapons(&tables.weapons)?;
        let map = scenario.map;
        let g = &fidelity.globals;
        let cell = fitted_cell(
            map.width_mm,
            map.height_mm,
            i64::from(g.grid_cell_mm),
            u64::from(g.max_grid_cells),
        );
        let origin = Vec2Fx::new(metres(map.origin_x_mm), metres(map.origin_y_mm));
        let grid = SpatialGrid::new(
            metres(cell),
            cells(map.width_mm, cell),
            cells(map.height_mm, cell),
            origin,
        )
        .map_err(|e| range_err(&format!("agent grid: {e}")))?;
        let mut state = BattleState::new(
            WorldState::new(RngStreams::new(seed)),
            scenario.terrain.grid.clone(),
            grid,
            tables,
        );
        let smoke_cell = cell.max(8000);
        state.smoke = SmokeGrid::new(
            u32::try_from(smoke_cell).map_err(|_| range_err("smoke cell"))?,
            cells(map.width_mm, smoke_cell),
            cells(map.height_mm, smoke_cell),
            origin,
        )
        .ok_or_else(|| range_err("smoke grid"))?;
        boundary::add_regiments(&mut state, scenario)?;
        Ok(Battle::assemble(state, scenario.clone(), fidelity))
    }

    /// Puts the scheduler over a hand-built state whose regiments are the non-headquarters units
    /// of `scenario`, in order. Arrivals are taken as done (the regiments are on the field), and
    /// the weather of the state is taken as the row in force at its tick. This is how the
    /// SPEC-005 set-ups run under the scheduler (T8-11).
    ///
    /// Errors: [`ScenarioError::Reference`] when the regiment count differs from the unit count.
    pub fn wrap(
        state: BattleState,
        scenario: Scenario,
        fidelity: FidelityTable,
    ) -> Result<Battle, ScenarioError> {
        let units = scenario.units.iter().filter(|u| u.arm != Arm::Hq).count();
        if units != state.regiments.len() {
            return Err(ScenarioError::Reference(format!(
                "{} regiments for {units} units",
                state.regiments.len()
            )));
        }
        let mut b = Battle::assemble(state, scenario, fidelity);
        b.fid.arrivals_done = b.scenario.arrivals.len();
        b.fid.weather_row = b.weather_row_at(b.state.world.tick);
        Ok(b)
    }

    fn assemble(state: BattleState, scenario: Scenario, table: FidelityTable) -> Battle {
        let mut reg_unit = vec![0; state.regiments.len()];
        for (ui, u) in scenario.units.iter().enumerate() {
            if let Some(slot) = u.regiment.and_then(|r| reg_unit.get_mut(usize::from(r.0))) {
                *slot = ui;
            }
        }
        let sectors = SectorMap::new(&scenario.map, &table);
        Battle {
            fid: FidelityState {
                table,
                sectors,
                arrivals_done: 0,
                departures_done: 0,
                triggers_done: 0,
                weather_row: None,
                arrived: [0; 2],
                departed: [0; 2],
                reg_unit,
            },
            state,
            scenario,
        }
    }

    /// One tick: the boundary work when `tick % assign_interval_ticks == 0`, then
    /// [`BattleState::step`].
    pub fn step(&mut self) {
        let interval = u64::from(self.fid.table.globals.assign_interval_ticks);
        if self.state.world.tick.is_multiple_of(interval) {
            self.boundary();
        }
        self.state.step();
    }

    /// Steps until the world is at tick `until` (nothing when it already is, or past it). The
    /// same `step` as a played run, so the hash is the same.
    pub fn run(&mut self, until: u64) {
        while self.state.world.tick < until {
            self.step();
        }
    }

    /// The hash of the battle: the `battle_hash` byte stream, then the FID1 block of every field
    /// the scheduler adds (SPEC-008 section 9, REGISTRY section 5).
    pub fn hash(&self) -> u64 {
        let mut h = StateHasher::new();
        battle_hash_into(&self.state, &mut h);
        fidhash::write_fid_block(&mut h, self);
        h.finish()
    }

    /// Soldiers and rounds of `team` with the arrivals and departures of the scheduler.
    pub fn ledger(&self, team: u8) -> BattleLedger {
        let side = usize::from(team.min(1));
        BattleLedger {
            ledger: self.state.ledger(team),
            arrived: self.fid.arrived[side],
            departed: self.fid.departed[side],
        }
    }

    /// The side with strictly more fit soldiers inside the radius of `objective`, or `None`.
    /// A class 2 regiment counts its fit men at its anchor.
    pub fn objective_holder(&self, objective: &Objective) -> Option<Side> {
        let (cx, cy) = (i128::from(objective.x_mm), i128::from(objective.y_mm));
        let r2 = i128::from(objective.radius_mm).pow(2);
        let inside = |p: Vec2Fx| {
            let (x, y) = anchor_mm(p);
            (x - cx).pow(2) + (y - cy).pow(2) <= r2
        };
        let mut fit = [0u64; 2];
        let agents = &self.state.world.agents;
        for id in agents.iter_alive() {
            let flags = agents.flags(id).unwrap_or(0);
            let team = usize::from(agents.team(id).unwrap_or(0).min(1));
            if flags & (F_DISABLED | F_WOUNDED) == 0 && agents.pos(id).is_some_and(inside) {
                fit[team] += 1;
            }
        }
        for reg in &self.state.regiments {
            if reg.status == RegimentStatus::Present && reg.class == F2 && inside(reg.anchor) {
                fit[usize::from(reg.team.min(1))] += u64::from(reg.aggregate.fit);
            }
        }
        match fit[0].cmp(&fit[1]) {
            std::cmp::Ordering::Greater => Some(Side::Union),
            std::cmp::Ordering::Less => Some(Side::Confederate),
            std::cmp::Ordering::Equal => None,
        }
    }

    /// The weather row in force at `tick`: the last with `from_s * tick_hz <= tick`.
    fn weather_row_at(&self, tick: u64) -> Option<usize> {
        let hz = u64::from(self.scenario.tick_hz);
        self.scenario
            .weather
            .iter()
            .rposition(|w| u64::from(w.from_s) * hz <= tick)
    }
}
