//! The headquarters port (SPEC-010 section 3): everything the AI may read and send.
//!
//! The AI never receives the world state, a mutable rules table, an enemy regiment id or a count
//! the port did not report. `MsgStatus` offers no more than the player's interface shows.

use battle_rules::command::{encode_dispatch, Latitude, OfficerId};
use battle_rules::{encode_fire_mode, FireMode, Formation, Gait, Order, RegimentId};
use battle_rules::{ObstacleLayer, TerrainGrid};
use core_math::{Angle, Vec2Fx};

/// A commander position (army, corps, division or brigade); the officer id of the command layer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SeatId(pub u16);

/// One of the AI side's own units as the port names it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UnitRef(pub u16);

/// An enemy contact as the headquarters holds it; never an enemy regiment id.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContactId(pub u32);

/// Identifies a submitted message.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MsgId(pub u32);

/// Delivery state of a submitted message, no more than the player's interface shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MsgStatus {
    /// Not yet acknowledged (in transit, lost or misread: the AI cannot tell).
    Pending,
    /// The recipient acknowledged it.
    Acked,
}

/// What a headquarters knows of one of its own units, as last reported.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OwnReport {
    /// The unit.
    pub unit: UnitRef,
    /// Reported position.
    pub pos: Vec2Fx,
    /// Reported heading.
    pub heading: Angle,
    /// Reported formation.
    pub formation: Formation,
    /// Men present.
    pub strength: u16,
    /// Men at the start of the action.
    pub strength_ref: u16,
    /// Rounds left, permille of the full load.
    pub rounds_pm: u16,
    /// Fatigue, permille.
    pub fatigue_pm: u16,
    /// Cohesion, permille, when the report carries it.
    pub cohesion_pm: Option<u16>,
    /// Ticks since the report was made.
    pub age_ticks: u32,
}

/// An enemy sighting as the headquarters holds it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Contact {
    /// Identity within the headquarters' picture.
    pub id: ContactId,
    /// Believed position.
    pub pos: Vec2Fx,
    /// Believed strength, men.
    pub strength_est: u16,
    /// Believed formation, when made out.
    pub formation_est: Option<Formation>,
    /// Believed weapon class number, when made out.
    pub weapon_est: Option<u8>,
    /// Tick of the last sighting.
    pub last_seen: u64,
    /// Quality of the sighting, permille.
    pub quality_pm: u16,
}

/// A fire order; `AtContact` becomes `AtWill` on a regiment inside the port.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FireOrder {
    /// Fire at the regiment behind this contact.
    AtContact(ContactId),
    /// Cease fire.
    Cease,
}

/// The purpose of an objective.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectiveKind {
    /// Take the ground.
    Seize,
    /// Keep the ground.
    Hold,
    /// Cover the main body.
    Screen,
    /// Back up another unit.
    Support,
    /// Buy time on the ground.
    Delay,
    /// Give the ground up.
    Withdraw,
}

/// The operational-to-tactical order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObjectiveBody {
    /// Purpose.
    pub kind: ObjectiveKind,
    /// Centre of the area, metres.
    pub center: Vec2Fx,
    /// Radius of the area, millimetres.
    pub radius_mm: i64,
    /// Tick by which it should be done.
    pub deadline_tick: u64,
    /// Priority, higher first.
    pub priority: u16,
    /// "If practicable".
    pub discretionary: bool,
}

/// What a message orders.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrderBody {
    /// A regiment order of the rules.
    Reg(Order),
    /// A fire order.
    Fire(FireOrder),
    /// An objective for a tactical seat.
    Objective(ObjectiveBody),
}

/// A message to submit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutMessage {
    /// Sending seat.
    pub from: SeatId,
    /// Addressee.
    pub to: UnitRef,
    /// The order.
    pub body: OrderBody,
}

impl OutMessage {
    /// The replay payload of this message through the existing codecs, or `None` when the port
    /// alone can encode it.
    ///
    /// A regiment order is a SPEC-007 dispatch (automatic medium, strict latitude: the AI's own
    /// gate has approved it). An objective is sent through the fallback of SPEC-010 section 15,
    /// a `Move` of the recipient to the centre at `Quick`. `Cease` is the SPEC-005 hold-fire
    /// input. `AtContact` has no bytes here because the port resolves the contact to a regiment.
    pub fn wire(&self) -> Option<Vec<u8>> {
        let dispatch = |order: &Order| {
            Some(encode_dispatch(
                OfficerId(self.from.0),
                OfficerId(self.to.0),
                0,
                Latitude::Strict,
                order,
            ))
        };
        match &self.body {
            OrderBody::Reg(order) => dispatch(order),
            OrderBody::Objective(o) => dispatch(&Order::Move {
                to: o.center,
                gait: Gait::Quick,
            }),
            OrderBody::Fire(FireOrder::Cease) => {
                Some(encode_fire_mode(RegimentId(self.to.0), FireMode::HoldFire))
            }
            OrderBody::Fire(FireOrder::AtContact(_)) => None,
        }
    }
}

/// A named piece of ground the scenario marks as important.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyPoint {
    /// Identity, ascending.
    pub id: u16,
    /// Position, metres.
    pub pos: Vec2Fx,
    /// Weight in the value of the ground, permille.
    pub weight_pm: u16,
}

/// Public map knowledge.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapView {
    /// Terrain classes.
    pub terrain: TerrainGrid,
    /// Walls and earthworks.
    pub obstacles: ObstacleLayer,
    /// Ground height per terrain cell in millimetres, row-major, when the map has heights.
    pub heights_mm: Option<Vec<i32>>,
    /// Key points ascending by id.
    pub key_points: Vec<KeyPoint>,
}

/// Everything the AI reads from and sends to the game, and nothing else.
pub trait HqPort {
    /// The current tick.
    fn now(&self) -> u64;
    /// Appends the reports of own units, ascending unit id, as delivered.
    fn own_reports(&self, out: &mut Vec<OwnReport>);
    /// Appends the contacts, ascending id.
    fn contacts(&self, out: &mut Vec<Contact>);
    /// The public map.
    fn map(&self) -> MapView;
    /// Sends a message; the same latency and loss apply as to a player.
    fn submit(&mut self, msg: OutMessage) -> MsgId;
    /// Delivery state of a message.
    fn status(&self, id: MsgId) -> MsgStatus;
}
