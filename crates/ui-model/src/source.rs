//! Read contracts of SPEC-015 section 3: what the interface asks of the information model and the
//! morale model, and `FullInfo`, the truth-reading implementation for Full view and the tutorial.

use battle_rules::{BattleState, Order, RegimentId, RegimentStatus};
use core_math::Vec2Fx;

/// One enemy regiment as headquarters last saw it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sighting {
    /// Regiment id of the sighted unit.
    pub unit: u16,
    /// Tick of the last sighting.
    pub last_seen_tick: u64,
    /// Believed position, metres.
    pub pos: Vec2Fx,
    /// Position error radius, mm.
    pub pos_err_mm: u32,
    /// Lower bound of the believed strength.
    pub strength_lo: u16,
    /// Upper bound of the believed strength.
    pub strength_hi: u16,
    /// Whether the formation was made out.
    pub formation_known: bool,
}

/// One order as the issuer's headquarters knows it (SPEC-015 section 5).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrderRecord {
    /// Order number.
    pub id: u32,
    /// Regiment addressed.
    pub regiment: RegimentId,
    /// The order as drafted.
    pub order: Order,
    /// Tick it was sent, `None` while drafted.
    pub sent_tick: Option<u64>,
    /// Earliest estimated arrival.
    pub eta_lo: u64,
    /// Latest estimated arrival.
    pub eta_hi: u64,
    /// Latest estimated arrival when the order was sent.
    pub eta_hi0: u64,
    /// Tick the acknowledgement came back.
    pub ack_tick: Option<u64>,
    /// The order as the addressee understood it.
    pub interpreted: Option<Order>,
    /// Tick the addressee began executing.
    pub executed_tick: Option<u64>,
    /// Tick the order was carried out.
    pub completed_tick: Option<u64>,
    /// The loss of the order is known to headquarters.
    pub lost_known: bool,
}

/// A regiment's morale as the morale model reports it (SPEC-015 section 6).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MoraleReport {
    /// 0 steady, 1 uneasy, 2 wavering, 3 breaking, 4 broken.
    pub level: u8,
    /// Model units above the next worse level.
    pub margin: i32,
    /// Direction of change.
    pub trend: i8,
    /// Factor id and signed contribution; sums to the model's total.
    pub factors: Vec<(u8, i32)>,
}

/// What headquarters knows. Implemented by the command layer (SPEC-007) and by [`FullInfo`].
pub trait InfoSource {
    /// Appends the sightings of enemies of `side` as known at tick `now`, ascending by unit.
    fn sightings(&self, side: u8, now: u64, out: &mut Vec<Sighting>);
    /// Appends the order records of `side`, ascending by id.
    fn orders(&self, side: u8, out: &mut Vec<OrderRecord>);
    /// Window of ticks in which `order` to `regiment` would arrive if sent at `now`.
    fn eta_window(&self, regiment: RegimentId, order: &Order, now: u64) -> (u64, u64);
}

/// The morale model's report for one regiment.
pub trait MoraleSource {
    /// The report for `r`.
    fn report(&self, r: RegimentId) -> MoraleReport;
}

/// Truth as an information source: every enemy regiment seen exactly, now, with no error.
pub struct FullInfo<'a> {
    state: &'a BattleState,
}

impl<'a> FullInfo<'a> {
    /// A source that reads `state`.
    pub fn new(state: &'a BattleState) -> FullInfo<'a> {
        FullInfo { state }
    }
}

/// Soldiers of a regiment: the fit count of an aggregate, else the roster length.
pub(crate) fn strength_of(state: &BattleState, id: usize) -> u32 {
    let r = &state.regiments[id];
    if r.class == battle_rules::F2 {
        r.aggregate.fit
    } else {
        u32::try_from(r.roster.len()).unwrap_or(u32::MAX)
    }
}

impl InfoSource for FullInfo<'_> {
    fn sightings(&self, side: u8, now: u64, out: &mut Vec<Sighting>) {
        for (i, r) in self.state.regiments.iter().enumerate() {
            if r.team == side || r.status != RegimentStatus::Present {
                continue;
            }
            let n = u16::try_from(strength_of(self.state, i)).unwrap_or(u16::MAX);
            out.push(Sighting {
                unit: r.id.0,
                last_seen_tick: now,
                pos: r.anchor,
                pos_err_mm: 0,
                strength_lo: n,
                strength_hi: n,
                formation_known: true,
            });
        }
    }

    fn orders(&self, _side: u8, _out: &mut Vec<OrderRecord>) {}

    fn eta_window(&self, _regiment: RegimentId, _order: &Order, now: u64) -> (u64, u64) {
        (now, now)
    }
}
