//! State of the command layer (SPEC-007 section 4): officers, links, messages, order records and
//! pictures, with enabling and the officer position refresh. Everything here is hashed by
//! [`write_command_block`](super::hash::write_command_block) when the layer is on.

use super::{CommandTable, Latitude, Level, LinkSpec, Medium, OfficerId, OfficerSpec, TraitMask};
use crate::{fx_ratio, BattleState, DataError, Order, RegimentId};
use core_math::{Fx, Vec2Fx};

/// Identifies a message; ids below 2^24 so every message owns a random stream.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MessageId(pub u32);

/// An order with the freedom its recipient has in carrying it out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OrderBody {
    /// The regiment-level order.
    pub order: Order,
    /// How strictly it binds.
    pub latitude: Latitude,
}

/// What a message is (wire numbers as in the discriminant).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageKind {
    /// An order going down.
    Order = 1,
    /// Receipt of an order going back up.
    Ack = 2,
    /// Contacts and friends going up.
    Report = 3,
}

/// Where a message is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fate {
    /// On its way.
    InFlight = 0,
    /// Gone; nothing tells the sender.
    Lost = 1,
    /// Taken by the enemy.
    Captured = 2,
}

/// How reading garbled an order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GarbleKind {
    /// The target of a move was displaced.
    Displaced = 1,
    /// The gait of a move was shifted.
    GaitShifted = 2,
    /// A form named another formation.
    FormSwapped = 3,
    /// A face turned the heading.
    Turned = 4,
    /// The order became a hold.
    Stalled = 5,
}

/// How interpretation changed an order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModKind {
    /// Executed as received.
    None = 0,
    /// The move was pushed further.
    Extended = 1,
    /// The move was pulled short.
    StoppedShort = 2,
    /// The officer declined.
    Declined = 3,
    /// The order was out of date and discarded.
    Stale = 4,
}

/// What truly became of an order (shown only with the omniscient view or after the battle).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Truth {
    /// Still travelling.
    InFlight = 0,
    /// Read by the addressee.
    Delivered = 1,
    /// Lost on the way.
    Lost = 2,
    /// Captured on the way.
    Captured = 3,
    /// Discarded as stale.
    Discarded = 4,
}

/// The two switches of one side (`registry::OPT_*`); both default off.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CommandOptions {
    /// The side's pictures show every enemy regiment.
    pub omniscient_view: bool,
    /// Orders arrive next tick, without friction.
    pub instant_orders: bool,
}

/// A node of the chain of command with the state the rules change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Officer {
    /// Identity; the officers of a battle ascend by id.
    pub id: OfficerId,
    /// Side, 0 or 1.
    pub side: u8,
    /// Rank.
    pub level: Level,
    /// Superior; none for the army.
    pub parent: Option<OfficerId>,
    /// The regiment a regiment-level officer owns.
    pub regiment: Option<RegimentId>,
    /// Command traits.
    pub mask: TraitMask,
    /// Successor when this officer falls.
    pub deputy: Option<OfficerId>,
    /// The officer has fallen.
    pub down: bool,
    /// Where he is, refreshed every `pos_interval_ticks`.
    pub pos: Vec2Fx,
    /// Fixed headquarters, if any.
    pub hq_fixed: Option<Vec2Fx>,
    /// Observer range multiplier of his post, permille.
    pub post_range_pm: u32,
    /// Last sequence number he issued.
    pub seq: u32,
    /// Highest sequence number read from each sender, ascending by sender.
    pub last_seq_from: Vec<(OfficerId, u32)>,
    /// Tick of his last report.
    pub last_report: u64,
}

fn mm_point(mm: (i64, i64)) -> Vec2Fx {
    Vec2Fx::new(fx_ratio(mm.0, 1000), fx_ratio(mm.1, 1000))
}

impl Officer {
    /// A fresh officer from a sidecar row and the traits of his commander record: standing at his
    /// fixed headquarters (or the origin until the first refresh), nothing issued or read.
    pub fn from_spec(spec: &OfficerSpec, mask: TraitMask) -> Officer {
        let hq_fixed = spec.hq.map(mm_point);
        Officer {
            id: spec.id,
            side: spec.side,
            level: spec.level,
            parent: spec.parent,
            regiment: spec.regiment,
            mask,
            deputy: spec.deputy,
            down: false,
            pos: hq_fixed.unwrap_or(Vec2Fx::ZERO),
            hq_fixed,
            post_range_pm: spec.post_range_pm,
            seq: 0,
            last_seq_from: Vec::new(),
            last_report: 0,
        }
    }
}

/// A signal link between two officers, symmetric.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Link {
    /// `Flags` or `Telegraph`.
    pub medium: Medium,
    /// Lower officer id.
    pub a: OfficerId,
    /// Higher officer id.
    pub b: OfficerId,
    /// The link has been cut.
    pub cut: bool,
}

impl Link {
    /// An intact link from a sidecar row.
    pub fn from_spec(spec: &LinkSpec) -> Link {
        Link {
            medium: spec.medium,
            a: spec.a,
            b: spec.b,
            cut: false,
        }
    }
}

/// An enemy contact in an officer's picture. Never carries the enemy's `RegimentId`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Contact {
    /// Identity within the picture.
    pub id: u32,
    /// Believed position.
    pub pos: Vec2Fx,
    /// Believed strength, men.
    pub strength: u32,
    /// Quality of the sighting, permille.
    pub quality_pm: u32,
    /// Tick it was last seen.
    pub seen: u64,
    /// Position uncertainty, mm.
    pub uncertainty_mm: i64,
    /// It came from a captured order.
    pub intercepted: bool,
}

/// A friendly regiment as an officer last heard of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FriendEntry {
    /// The regiment.
    pub regiment: RegimentId,
    /// Believed position.
    pub pos: Vec2Fx,
    /// Believed strength, men.
    pub strength: u32,
    /// Tick the entry was made.
    pub seen: u64,
}

/// What one officer knows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Picture {
    /// Enemy contacts ascending by id.
    pub contacts: Vec<Contact>,
    /// Friends ascending by regiment.
    pub friends: Vec<FriendEntry>,
    /// Id of the next new contact.
    pub next_contact: u32,
}

/// The receipt an order's issuer gets back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AckRecord {
    /// Tick the ack reached the issuer.
    pub tick: u64,
    /// The body as read, after garbling.
    pub received: OrderBody,
    /// The body as executed, after interpretation.
    pub executing: OrderBody,
    /// How interpretation changed it.
    pub kind: ModKind,
}

/// What a message carries besides its order body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MessagePayload {
    /// An order has nothing more.
    None,
    /// The receipt of an order.
    Ack(AckRecord),
    /// Contacts and friends of a report.
    Report {
        /// Contacts, confidence descending.
        contacts: Vec<Contact>,
        /// Friend entries ascending by regiment.
        friends: Vec<FriendEntry>,
    },
}

/// A message in flight or waiting to be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    /// Identity, ascending in the queue.
    pub id: MessageId,
    /// Order, ack or report.
    pub kind: MessageKind,
    /// Sender.
    pub from: OfficerId,
    /// Addressee.
    pub to: OfficerId,
    /// How it travels.
    pub medium: Medium,
    /// Hour of the order, from the issuer's counter.
    pub seq: u32,
    /// The order carried (a hold for an ack or a report).
    pub body: OrderBody,
    /// Length for latency.
    pub words: u32,
    /// The first message of the chain this one belongs to.
    pub root: MessageId,
    /// Where the current leg started.
    pub leg_from: Vec2Fx,
    /// Where the current leg is headed.
    pub aim: Vec2Fx,
    /// Courier legs taken so far.
    pub legs: u8,
    /// Tick it arrives.
    pub due: u64,
    /// Whether it is still on its way.
    pub fate: Fate,
    /// How it was garbled, once read.
    pub garbled: Option<GarbleKind>,
    /// State of the message's own random stream.
    pub rng: [u64; 4],
    /// Ack or report content.
    pub payload: MessagePayload,
}

/// The issuer's record of one order, from which its status is derived.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrderRecord {
    /// The message that carries it.
    pub id: MessageId,
    /// Issuer.
    pub from: OfficerId,
    /// Addressee.
    pub to: OfficerId,
    /// Tick it was sent.
    pub sent_tick: u64,
    /// Earliest estimated arrival.
    pub eta_lo: u64,
    /// Latest estimated arrival.
    pub eta_hi: u64,
    /// The order as sent.
    pub sent: OrderBody,
    /// The addressee's receipt, once it reached the issuer.
    pub ack: Option<AckRecord>,
    /// What truly became of it.
    pub truth: Truth,
    /// How it was garbled, if it was.
    pub garble: Option<GarbleKind>,
}

/// The command layer of a battle.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CommandState {
    /// Options of side 0 and side 1.
    pub opt: [CommandOptions; 2],
    /// Officers ascending by id.
    pub officers: Vec<Officer>,
    /// Signal links.
    pub links: Vec<Link>,
    /// Messages in flight, ascending by id.
    pub msgs: Vec<Message>,
    /// Order records ascending by id.
    pub records: Vec<OrderRecord>,
    /// Id of the next message.
    pub next_msg: u32,
    /// One picture per officer, in officer order.
    pub pics: Vec<Picture>,
}

impl CommandState {
    /// Index of an officer, if present.
    pub fn index_of(&self, id: OfficerId) -> Option<usize> {
        self.officers.binary_search_by_key(&id, |o| o.id).ok()
    }
}

impl BattleState {
    /// Turns the command layer on with these officers (ascending by id, every regiment named
    /// existing) and links, then places the officers. Replaces any earlier command state.
    ///
    /// Errors: [`DataError::Range`] for officers out of order or a regiment that does not exist;
    /// the battle is unchanged.
    pub fn enable_command(
        &mut self,
        officers: Vec<Officer>,
        links: Vec<Link>,
    ) -> Result<(), DataError> {
        if officers.windows(2).any(|w| w[0].id >= w[1].id) {
            return Err(DataError::Range("officers must ascend by id".to_owned()));
        }
        if let Some(r) = officers
            .iter()
            .filter_map(|o| o.regiment)
            .find(|r| usize::from(r.0) >= self.regiments.len())
        {
            return Err(DataError::Range(format!("regiment {} does not exist", r.0)));
        }
        let pics = vec![Picture::default(); officers.len()];
        self.cmd = Some(CommandState {
            officers,
            links,
            pics,
            ..CommandState::default()
        });
        self.refresh_officer_positions();
        Ok(())
    }

    /// Moves every officer to his fixed headquarters or to the integer mean (truncating) of the
    /// anchors of the regiments with men in his formation; a formation with none keeps its place.
    pub fn refresh_officer_positions(&mut self) {
        let Some(cmd) = self.cmd.as_mut() else { return };
        let mut sums = vec![(0i128, 0i128, 0i128); cmd.officers.len()];
        for o in &cmd.officers {
            let Some(reg) = o
                .regiment
                .and_then(|r| self.regiments.get(usize::from(r.0)))
            else {
                continue;
            };
            if reg.roster.is_empty() {
                continue;
            }
            let mut at = cmd.index_of(o.id);
            while let Some(i) = at {
                let s = &mut sums[i];
                *s = (
                    s.0 + i128::from(reg.anchor.x.raw()),
                    s.1 + i128::from(reg.anchor.y.raw()),
                    s.2 + 1,
                );
                at = cmd.officers[i].parent.and_then(|p| cmd.index_of(p));
            }
        }
        for (o, (x, y, n)) in cmd.officers.iter_mut().zip(sums) {
            if let Some(hq) = o.hq_fixed {
                o.pos = hq;
            } else if n > 0 {
                let mean = |v: i128| Fx::from_raw((v / n) as i64);
                o.pos = Vec2Fx::new(mean(x), mean(y));
            }
        }
    }

    /// One tick of the command layer. This part refreshes officer positions every
    /// `pos_interval_ticks`; later parts add observation, reports and arrivals. Nothing happens
    /// while the layer is off.
    pub fn step_command(&mut self, table: &CommandTable) {
        if self.cmd.is_some()
            && self
                .world
                .tick
                .is_multiple_of(u64::from(table.globals.pos_interval_ticks))
        {
            self.refresh_officer_positions();
        }
    }
}
