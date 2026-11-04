//! `BattleState`: the world plus regiments, per-soldier locomotion data and the drill state
//! machine (SPEC-004 sections 7 and 11). Steering and anchor movement belong to the movement
//! step; this module owns orders, the `Forming` countdown and the close-up of a thinned line.

use crate::command::CommandState;
use crate::fire::{RangeEntry, RegFire};
use crate::hit::{ObstacleLayer, QueuedHit};
use crate::{
    assign_slots, slot_layout, transition_ticks, DrillTable, Formation, Order, Regiment,
    RegimentId, RegimentSpec, RegimentState, SlotLocal, Tables, TerrainGrid, F_DISABLED, F_WOUNDED,
};
use crate::{Aggregate, RegimentStatus, RoundsAccount, F0};
use crate::{
    Casualty, Counters, EnvTable, Environment, FireExt, Projectile, SmokeGrid, Weather,
    WeatherTable,
};
use core_math::{StreamId, Vec2Fx};
use sim_state::{AgentId, AgentInit, AgentStore, SpatialGrid, WorldState};

// The constant lives in the registry module (T-026); the path `battle_rules::STREAM_SOLDIER_BASE`
// is unchanged.
pub use crate::registry::STREAM_SOLDIER_BASE;

/// The named stream of the soldier in agent slot `slot_index`.
pub const fn soldier_stream(slot_index: u32) -> StreamId {
    StreamId(STREAM_SOLDIER_BASE.wrapping_add(slot_index))
}

/// Hit points of a freshly spawned soldier.
pub(crate) const SPAWN_HP: u16 = 100;

/// Per-soldier locomotion data, indexed by agent slot index.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LocoExt {
    /// Exhaustion in ppm, `0..=1_000_000`.
    pub fatigue: u32,
    /// The soldier's assigned place in its regiment's frame.
    pub slot: SlotLocal,
    /// The soldier is using the catch-up gait.
    pub catchup: bool,
    /// Forward offset of the steering target in mm, set by the sight period (SPEC-004
    /// amendment A6); 0 with `env` `None`.
    pub peek_mm: u16,
    /// The soldier found no visible target since the last sight period.
    pub blind: bool,
    /// The soldier took a shot since the last sight period.
    pub fired: bool,
}

/// The whole battle: world, regiments and the tables they obey.
#[derive(Clone, Debug)]
pub struct BattleState {
    /// Tick, agents and random streams.
    pub world: WorldState,
    /// Regiments in ascending id order; `regiments[i].id == RegimentId(i)`.
    pub regiments: Vec<Regiment>,
    /// Locomotion data, one entry per agent slot.
    pub loco: Vec<LocoExt>,
    /// Terrain.
    pub terrain: TerrainGrid,
    /// Spatial index of the agents, rebuilt by the movement step.
    pub grid: SpatialGrid,
    /// Rules tables.
    pub tables: Tables,
    /// Fire state, one entry per agent slot once a regiment is equipped (empty before).
    pub fire: Vec<FireExt>,
    /// Projectiles in flight in ascending `seq`.
    pub pool: Vec<Projectile>,
    /// Smoke density; an empty grid until the scenario sets one.
    pub smoke: SmokeGrid,
    /// Walls and earthworks (scenario data, not hashed).
    pub obstacles: ObstacleLayer,
    /// The casualty log, in order of application.
    pub casualties: Vec<Casualty>,
    /// Scenario weather.
    pub weather: Weather,
    /// `seq` of the next projectile.
    pub seq: u64,
    /// The command layer (SPEC-007); `None` leaves every SPEC-004 and SPEC-005 behaviour and the
    /// hash unchanged.
    pub cmd: Option<CommandState>,
    /// Morale and cohesion (SPEC-006); `None` leaves every earlier behaviour and the hash
    /// unchanged.
    pub morale: Option<crate::morale::MoraleState>,
    /// The environment layer (SPEC-009): weather timeline, heat, powder damp, ground wetness;
    /// `None` leaves every SPEC-004 and SPEC-005 behaviour and the hash unchanged.
    pub env: Option<Environment>,
    /// Weapon and fire mode per regiment.
    pub(crate) regfire: Vec<RegFire>,
    /// Range tables built for the equipped weapon and ammunition pairs.
    pub(crate) ranges: Vec<RangeEntry>,
    /// Hits found by the flight phase, applied by the casualty step (empty between ticks).
    pub(crate) pending: Vec<QueuedHit>,
    /// F0 counters.
    pub(crate) counters: Counters,
    /// Serial of the next materialised soldier (SPEC-005 amendment A7).
    pub soldier_serial: u32,
}

/// Drops soldiers that are gone, disabled or wounded; marks the regiment dirty if any left.
pub(crate) fn sync_roster(agents: &AgentStore, reg: &mut Regiment) {
    let before = reg.roster.len();
    reg.roster.retain(|&id| {
        agents.is_alive(id)
            && agents
                .flags(id)
                .is_some_and(|f| f & (F_DISABLED | F_WOUNDED) == 0)
    });
    if reg.roster.len() != before {
        reg.dirty = true;
    }
}

/// Builds the layout of `formation` for the current roster at the regiment's anchor and heading,
/// assigns every soldier a slot and stores it. Clears `dirty`.
fn relayout(
    agents: &AgentStore,
    loco: &mut [LocoExt],
    reg: &mut Regiment,
    formation: Formation,
    drill: &DrillTable,
) {
    let positions: Vec<(AgentId, Vec2Fx)> = reg
        .roster
        .iter()
        .map(|&id| (id, agents.pos(id).unwrap_or_default()))
        .collect();
    let layout = slot_layout(formation, positions.len(), drill);
    for (id, k) in assign_slots(&positions, &layout, reg.anchor, reg.heading) {
        if let Some(entry) = loco.get_mut(id.index as usize) {
            entry.slot = layout[k];
        }
    }
    reg.slots = layout;
    reg.dirty = false;
}

/// Applies an order to a regiment now. See [`BattleState::issue_order`].
fn apply_order(
    agents: &AgentStore,
    loco: &mut [LocoExt],
    drill: &DrillTable,
    reg: &mut Regiment,
    order: Order,
) {
    match (order, reg.state) {
        (Order::Hold, _) => {
            reg.order = Order::Hold;
            reg.pending = None;
        }
        (Order::Form(f), RegimentState::Steady(g)) => {
            if f == g {
                return;
            }
            sync_roster(agents, reg);
            relayout(agents, loco, reg, f, drill);
            reg.state = RegimentState::Forming {
                from: g,
                to: f,
                ticks_left: transition_ticks(g, f, reg.training, drill),
            };
            reg.order = Order::Hold;
            reg.pending = None;
        }
        (other, RegimentState::Steady(_)) => {
            if matches!(other, Order::Move { .. }) {
                // A new march starts from the plain slots (SPEC-004 amendment A6).
                for id in &reg.roster {
                    if let Some(l) = loco.get_mut(id.index as usize) {
                        l.peek_mm = 0;
                    }
                }
                reg.peek_mm = 0;
            }
            reg.order = other;
        }
        (other, RegimentState::Forming { .. }) => reg.pending = Some(other),
    }
}

impl BattleState {
    /// An empty battle at the world's tick with the given terrain, spatial index and tables.
    /// The spatial index must cover the battlefield; it is rebuilt every movement step.
    pub fn new(
        world: WorldState,
        terrain: TerrainGrid,
        grid: SpatialGrid,
        tables: Tables,
    ) -> BattleState {
        let loco = vec![LocoExt::default(); world.agents.slot_count()];
        BattleState {
            world,
            regiments: Vec::new(),
            loco,
            terrain,
            grid,
            tables,
            fire: Vec::new(),
            pool: Vec::new(),
            smoke: SmokeGrid::empty(),
            obstacles: ObstacleLayer::default(),
            casualties: Vec::new(),
            weather: Weather::DRY,
            seq: 0,
            cmd: None,
            morale: None,
            env: None,
            regfire: Vec::new(),
            ranges: Vec::new(),
            pending: Vec::new(),
            counters: Counters::default(),
            soldier_serial: 0,
        }
    }

    /// Spawns a regiment standing on the slots of its starting formation and returns its id.
    /// Soldiers are created in slot order, so on a store without free slots their ids ascend with
    /// slot order. Each soldier is at full hit points, unflagged, facing the heading, with
    /// `unit` set to the regiment id and `team` to the side.
    ///
    /// # Panics
    /// Panics when more than 65,536 regiments are spawned.
    pub fn spawn_regiment(&mut self, spec: RegimentSpec) -> RegimentId {
        let id = RegimentId(
            u16::try_from(self.regiments.len()).expect("a battle holds at most 65536 regiments"),
        );
        let layout = slot_layout(spec.formation, spec.count, &self.tables.drill);
        let forward = Vec2Fx::from_angle(spec.heading);
        let right = Vec2Fx::from_angle(spec.heading - core_math::Angle::QUARTER);
        let mut roster = Vec::with_capacity(layout.len());
        for slot in &layout {
            let pos = spec.anchor + right.scale(slot.u) + forward.scale(slot.v);
            let agent = self.world.agents.spawn(AgentInit {
                pos,
                vel: Vec2Fx::ZERO,
                facing: spec.heading,
                hp: SPAWN_HP,
                flags: 0,
                team: spec.team,
                unit: u32::from(id.0),
            });
            self.loco
                .resize(self.world.agents.slot_count(), LocoExt::default());
            self.loco[agent.index as usize] = LocoExt {
                slot: *slot,
                ..LocoExt::default()
            };
            roster.push(agent);
        }
        roster.sort_unstable();
        self.regiments.push(Regiment {
            id,
            team: spec.team,
            training: spec.training,
            state: RegimentState::Steady(spec.formation),
            anchor: spec.anchor,
            heading: spec.heading,
            order: Order::Hold,
            pending: None,
            roster,
            dirty: false,
            slots: layout,
            status: RegimentStatus::Present,
            class: F0,
            since: 0,
            aggregate: Aggregate::default(),
            acct: RoundsAccount::default(),
            peek_mm: 0,
            view_blocked: false,
        });
        self.regfire.push(RegFire::NONE);
        id
    }

    /// Gives a regiment an order; returns `false` for an unknown regiment.
    ///
    /// No step runs between issuing and the start of the next step, so the order is applied here
    /// and is in force when that step begins.
    ///
    /// - `Hold` applies at once in any state and cancels a pending order.
    /// - In `Steady`, `Move` and `Face` become the standing order. `Form(f)` with `f` different
    ///   from the current formation builds the slots of `f` at the current anchor and heading,
    ///   assigns the soldiers, enters `Forming` for `transition_ticks`, and replaces the standing
    ///   order with `Hold` (the anchor does not move while forming). `Form` of the current
    ///   formation changes nothing.
    /// - In `Forming`, any order other than `Hold` replaces `pending` and starts when the
    ///   transition ends.
    pub fn issue_order(&mut self, id: RegimentId, order: Order) -> bool {
        let BattleState {
            world,
            regiments,
            loco,
            tables,
            ..
        } = self;
        match regiments.get_mut(usize::from(id.0)) {
            Some(reg) => {
                apply_order(&world.agents, loco, &tables.drill, reg, order);
                true
            }
            None => false,
        }
    }

    /// One tick of the drill state machine for every regiment in ascending id order:
    ///
    /// 1. members that were despawned, disabled or wounded leave the roster and set `dirty`;
    /// 2. a `dirty` regiment on a tick divisible by `closeup_interval_ticks` regenerates its
    ///    layout for the new head count (the formation it is in, or is taking up) and reassigns
    ///    every soldier;
    /// 3. a `Forming` regiment counts down and, at zero, becomes `Steady(to)` and starts its
    ///    pending order.
    ///
    /// The tick counter is not advanced here; the movement step owns that.
    pub fn step_drill(&mut self) {
        let BattleState {
            world,
            regiments,
            loco,
            tables,
            ..
        } = self;
        if loco.len() < world.agents.slot_count() {
            loco.resize(world.agents.slot_count(), LocoExt::default());
        }
        let drill = &tables.drill;
        let closeup_now = world.tick % u64::from(drill.globals.closeup_interval_ticks) == 0;
        for reg in regiments.iter_mut() {
            sync_roster(&world.agents, reg);
            if reg.dirty && closeup_now {
                let formation = reg.state.target_formation();
                relayout(&world.agents, loco, reg, formation, drill);
            }
            if let RegimentState::Forming {
                from,
                to,
                ticks_left,
            } = reg.state
            {
                let ticks_left = ticks_left.saturating_sub(1);
                if ticks_left == 0 {
                    reg.state = RegimentState::Steady(to);
                    if let Some(next) = reg.pending.take() {
                        apply_order(&world.agents, loco, drill, reg, next);
                    }
                } else {
                    reg.state = RegimentState::Forming {
                        from,
                        to,
                        ticks_left,
                    };
                }
            }
        }
    }

    /// Whether a regiment may fire (see [`can_fire`](crate::can_fire)); `false` for an unknown id.
    pub fn can_fire(&self, id: RegimentId) -> bool {
        self.regiments
            .get(usize::from(id.0))
            .is_some_and(crate::can_fire)
    }

    /// One whole tick in the order of REGISTRY section 6.2 for the steps that exist: TK-04
    /// movement, TK-05 grid rebuild and release of expired disables, TK-06 fire, TK-07 flight and
    /// hit, TK-10 casualties, TK-11 morale (only with `morale` `Some`), TK-12 smoke (decay, or with `env` `Some` the SPEC-009 update), TK-13 the peek update (only with `env` `Some`), TK-14 roster updates, TK-15 `advance_tick`
    /// (SPEC-005 amendment A5); with `env` `Some`, TK-02 first (SPEC-009). A later layer adds its step here at its place.
    pub fn step(&mut self) {
        self.step_with(false);
    }

    /// [`BattleState::step`] with the flight phase resolved forwards or backwards (test hook;
    /// the result is the same).
    #[doc(hidden)]
    pub fn step_with(&mut self, reverse_flight: bool) {
        self.step_environment();
        self.move_agents();
        self.grid.rebuild(&self.world.agents);
        self.release_disables();
        self.step_fire();
        self.step_flight_with(reverse_flight);
        self.step_casualties();
        self.step_morale();
        self.step_smoke();
        self.step_sight();
        self.step_rosters();
        self.world.advance_tick();
    }

    /// Switches the environment layer on at the current tick: dry powder and ground, and
    /// `weather` set from the timeline for the period the tick lies in (no rain step), so the
    /// first tick is consistent. Setting `env` to `None` switches it off.
    pub fn set_environment(&mut self, table: EnvTable, weather: WeatherTable) {
        let env = Environment::new(table, weather, self.world.tick);
        self.weather = env.weather_state();
        self.env = Some(env);
    }

    /// Step TK-02 (SPEC-009 section 7): when `tick % smoke.period_ticks == 0`, refreshes the
    /// weather, powder damp, ground wetness and heat, and fills `weather`. Does nothing with
    /// `env` `None`.
    pub fn step_environment(&mut self) {
        let tick = self.world.tick;
        if let Some(env) = self.env.as_mut() {
            if env.refresh(tick) {
                self.weather = env.weather_state();
            }
        }
    }

    /// The SPEC-009 smoke update of step TK-12: on the last tick of each smoke period the grid is
    /// updated with the wind and rain of the weather held for the period and the retention of the
    /// terrain class under each cell centre. Does nothing with `env` `None`.
    pub(crate) fn update_smoke_env(&mut self) {
        let BattleState {
            world,
            smoke,
            terrain,
            env,
            ..
        } = self;
        let Some(env) = env.as_ref() else {
            return;
        };
        let period = u64::from(env.table().smoke.period_ticks.max(1));
        if world.tick % period != period - 1 {
            return;
        }
        let now = env.now();
        let table = env.table();
        smoke.update(
            &table.smoke,
            now.wind_x_mm_s,
            now.wind_y_mm_s,
            now.rain_pm,
            &|pos| table.retain_pm(terrain.class_at(pos)),
        );
    }

    /// Step TK-13 (SPEC-009 section 6), on the last tick of each `[sight]` period: soldiers of
    /// regiments that are firing at will in line or skirmish order step their peek offset
    /// forward when blind and back when idle; every soldier's `blind` and `fired` are cleared;
    /// each regiment's `view_blocked`, `peek_mm` and [`EnvInputs`] are refreshed. Does nothing
    /// with `env` `None`.
    pub(crate) fn step_sight(&mut self) {
        let BattleState {
            world,
            regiments,
            loco,
            smoke,
            env,
            regfire,
            ..
        } = self;
        let Some(env) = env.as_mut() else {
            return;
        };
        let sp = &env.table().sight;
        let period = u64::from(sp.period_ticks.max(1));
        if world.tick % period != period - 1 {
            return;
        }
        let max = sp.peek_max_mm.min(u32::from(u16::MAX)) as u16;
        let step = sp.peek_step_mm.min(u32::from(u16::MAX)) as u16;
        let back = sp.peek_back_mm.min(u32::from(u16::MAX)) as u16;
        let report = u64::from(sp.blind_report_pm);
        let ref_units = env.table().smoke.ref_units;
        let drain = u16::try_from(env.heat().drain_pm).unwrap_or(u16::MAX);
        let mut inputs = Vec::with_capacity(regiments.len());
        for (ri, reg) in regiments.iter_mut().enumerate() {
            let at_will = matches!(regfire[ri].mode, crate::FireMode::AtWill { .. });
            let active = at_will && crate::can_fire(reg);
            let (mut blind, mut stuck, mut top) = (0u64, 0u64, 0u16);
            for id in &reg.roster {
                let Some(l) = loco.get_mut(id.index as usize) else {
                    continue;
                };
                if active {
                    if l.blind && l.peek_mm < max {
                        l.peek_mm = l.peek_mm.saturating_add(step).min(max);
                    } else if !l.blind && !l.fired {
                        l.peek_mm -= l.peek_mm.min(back);
                    }
                }
                blind += u64::from(l.blind);
                stuck += u64::from(l.blind && l.peek_mm >= max);
                top = top.max(l.peek_mm);
                l.blind = false;
                l.fired = false;
            }
            let len = reg.roster.len() as u64;
            reg.peek_mm = top;
            reg.view_blocked = len > 0 && stuck * 1000 >= report * len;
            inputs.push(crate::EnvInputs {
                smoke_pm: smoke.smoke_pm(reg.anchor, ref_units) as u16,
                blind_pm: (blind * 1000).checked_div(len).unwrap_or(0) as u16,
                view_blocked: reg.view_blocked,
                heat_drain_pm: drain,
            });
        }
        env.set_inputs(inputs);
    }

    /// What the morale layer reads of regiment `id` as of the last sight period; `None` with
    /// `env` `None`, for an unknown regiment, or before the first period has ended.
    pub fn env_inputs(&self, id: RegimentId) -> Option<crate::EnvInputs> {
        self.env.as_ref()?.inputs().get(usize::from(id.0)).copied()
    }

    /// Whether the regiment's view is blocked (SPEC-009 section 6): at least `blind_report_pm`
    /// permille of its soldiers stand at the peek limit and still cannot see. `false` for an
    /// unknown regiment.
    pub fn view_blocked(&self, id: RegimentId) -> bool {
        self.regiments
            .get(usize::from(id.0))
            .is_some_and(|r| r.view_blocked)
    }

    /// `(drain_pm, rest_pm)`, the heat multipliers of fatigue; both 1000 with `env` `None`.
    pub(crate) fn heat_pm(&self) -> (u32, u32) {
        self.env.as_ref().map_or((1000, 1000), |e| {
            let h = e.heat();
            (h.drain_pm, h.rest_pm)
        })
    }

    /// The state hash, see [`battle_hash`](crate::battle_hash).
    pub fn hash(&self) -> u64 {
        crate::battle_hash(self)
    }
}
