//! The snapshot (SPEC-015 sections 2 and 3): an immutable copy of everything the interface may
//! show for one tick, and `capture`, which builds it from shared references only.

use crate::source::{strength_of, InfoSource, MoraleReport, MoraleSource, OrderRecord};
use battle_rules::{BattleState, RegimentId, RegimentStatus, TICK_HZ};
use core_math::{Angle, Vec2Fx};

/// Which side headquarters is, and whether it sees through the information model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InfoMode {
    /// HQ view: delayed and partial, the default.
    Hq {
        /// The side whose headquarters it is.
        side: u8,
    },
    /// Full view: reads truth, flagged on the result screen.
    Full {
        /// The side whose headquarters it is.
        side: u8,
    },
}

impl InfoMode {
    /// The side of the seat.
    pub const fn side(self) -> u8 {
        match self {
            InfoMode::Hq { side } | InfoMode::Full { side } => side,
        }
    }
}

/// An own regiment, exact.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegimentView {
    /// Regiment id.
    pub id: u16,
    /// Side.
    pub side: u8,
    /// Anchor position, metres.
    pub pos: Vec2Fx,
    /// Heading.
    pub heading: Angle,
    /// Formation tag of the formation held or being taken up.
    pub formation: u8,
    /// Soldiers able to fight.
    pub strength: u32,
    /// Standing order tag.
    pub order: u8,
    /// Morale as reported.
    pub morale: MoraleReport,
}

/// An enemy regiment as headquarters believes it to be.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnemyView {
    /// Regiment id.
    pub unit: u16,
    /// Tick of the last sighting.
    pub last_seen_tick: u64,
    /// Believed position, metres.
    pub pos: Vec2Fx,
    /// Position error radius, mm.
    pub pos_err_mm: u32,
    /// Strength range, lower end rounded down to 50.
    pub strength_lo: u32,
    /// Strength range, upper end rounded up to 50.
    pub strength_hi: u32,
    /// Whether the formation was made out.
    pub formation_known: bool,
}

/// Everything the interface may show for one tick.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    /// Simulation tick.
    pub tick: u64,
    /// Clock time in seconds: the scenario start plus `tick / 20`.
    pub clock_s: u32,
    /// Side of the seat.
    pub side: u8,
    /// True in Full view.
    pub full_view: bool,
    /// Own regiments in ascending id.
    pub regiments: Vec<RegimentView>,
    /// Enemy regiments from the information source only, ascending by unit.
    pub enemies: Vec<EnemyView>,
    /// Order records, ascending by id.
    pub orders: Vec<OrderRecord>,
}

const STEP: u32 = 50;

/// Builds the snapshot. Own regiments come from `state`; enemies come only from `info`.
pub fn capture(
    state: &BattleState,
    info: &dyn InfoSource,
    morale: &dyn MoraleSource,
    mode: InfoMode,
    start_clock_s: u32,
) -> Snapshot {
    let side = mode.side();
    let tick = state.world.tick;
    let regiments = state
        .regiments
        .iter()
        .enumerate()
        .filter(|(_, r)| r.team == side && r.status == RegimentStatus::Present)
        .map(|(i, r)| RegimentView {
            id: r.id.0,
            side: r.team,
            pos: r.anchor,
            heading: r.heading,
            formation: r.state.target_formation().tag(),
            strength: strength_of(state, i),
            order: r.order.tag(),
            morale: morale.report(RegimentId(r.id.0)),
        })
        .collect();
    let mut sightings = Vec::new();
    info.sightings(side, tick, &mut sightings);
    let enemies = sightings
        .iter()
        .map(|s| EnemyView {
            unit: s.unit,
            last_seen_tick: s.last_seen_tick,
            pos: s.pos,
            pos_err_mm: s.pos_err_mm,
            strength_lo: u32::from(s.strength_lo) / STEP * STEP,
            strength_hi: u32::from(s.strength_hi).div_ceil(STEP) * STEP,
            formation_known: s.formation_known,
        })
        .collect();
    let mut orders = Vec::new();
    info.orders(side, &mut orders);
    let elapsed = u32::try_from(tick / u64::from(TICK_HZ)).unwrap_or(u32::MAX);
    Snapshot {
        tick,
        clock_s: start_clock_s.saturating_add(elapsed),
        side,
        full_view: matches!(mode, InfoMode::Full { .. }),
        regiments,
        enemies,
        orders,
    }
}
