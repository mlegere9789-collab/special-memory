//! `BattleState`: the world plus regiments, per-soldier locomotion data and the drill state
//! machine (SPEC-004 sections 7 and 11). Steering and anchor movement belong to the movement
//! step; this module owns orders, the `Forming` countdown and the close-up of a thinned line.

use crate::command::CommandState;
use crate::fire::{RangeEntry, RegFire};
use crate::{
    assign_slots, slot_layout, transition_ticks, DrillTable, Formation, Order, Regiment,
    RegimentId, RegimentSpec, RegimentState, SlotLocal, Tables, TerrainGrid, F_DISABLED, F_WOUNDED,
};
use crate::{Counters, FireExt, Projectile, SmokeGrid, Weather};
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
const SPAWN_HP: u16 = 100;

/// Per-soldier locomotion data, indexed by agent slot index.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LocoExt {
    /// Exhaustion in ppm, `0..=1_000_000`.
    pub fatigue: u32,
    /// The soldier's assigned place in its regiment's frame.
    pub slot: SlotLocal,
    /// The soldier is using the catch-up gait.
    pub catchup: bool,
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
    /// Scenario weather.
    pub weather: Weather,
    /// `seq` of the next projectile.
    pub seq: u64,
    /// The command layer (SPEC-007); `None` leaves every SPEC-004 and SPEC-005 behaviour and the
    /// hash unchanged.
    pub cmd: Option<CommandState>,
    /// Weapon and fire mode per regiment.
    pub(crate) regfire: Vec<RegFire>,
    /// Range tables built for the equipped weapon and ammunition pairs.
    pub(crate) ranges: Vec<RangeEntry>,
    /// F0 counters.
    pub(crate) counters: Counters,
}

/// Drops soldiers that are gone, disabled or wounded; marks the regiment dirty if any left.
fn sync_roster(agents: &AgentStore, reg: &mut Regiment) {
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
        (other, RegimentState::Steady(_)) => reg.order = other,
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
            weather: Weather::DRY,
            seq: 0,
            cmd: None,
            regfire: Vec::new(),
            ranges: Vec::new(),
            counters: Counters::default(),
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

    /// The state hash, see [`battle_hash`](crate::battle_hash).
    pub fn hash(&self) -> u64 {
        crate::battle_hash(self)
    }
}
