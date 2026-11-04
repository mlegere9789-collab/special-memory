//! The AI of one side (SPEC-010 sections 5, 6, 9 and 11): seats, planners, streams, notes and the
//! hash. The planners here are empty; the operational and tactical layers plug in through
//! [`Planner`] in later tickets.

use crate::notes::{AiNote, PlanKind, ReasonCode};
use crate::outbox::{Event, Outbox};
use crate::personality::{personality_for, Personality};
use crate::port::{Contact, HqPort, MapView, OrderBody, OutMessage, OwnReport, SeatId, UnitRef};
use crate::tables::{AiGlobals, AiTables, Difficulty, DifficultyRow};
use battle_rules::registry::{self, ACTOR_ADVISOR_BASE, ACTOR_AI_BASE};
use core_math::{Rng, RngStreams};
use sim_state::StateHasher;

/// First AI stream; stream `STREAM_AI_BASE + side * 16 + layer` (REGISTRY section 3).
pub const STREAM_AI_BASE: u32 = registry::STREAM_AI_BASE;
/// Replay actor of the AI seat of side 0; side `s` is `AI_ACTOR_BASE + s` (REGISTRY section 2).
pub const AI_ACTOR_BASE: u8 = ACTOR_AI_BASE;
/// Replay actor of the advisor reply of side 0; side `s` is `ADVISOR_ACTOR_BASE + s`.
pub const ADVISOR_ACTOR_BASE: u8 = ACTOR_ADVISOR_BASE;

/// Stream layers of one side.
const LAYER_OL: u8 = 0;
const LAYER_TL: u8 = 1;
const LAYERS: u8 = 5;

/// Which layer a seat runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeatLevel {
    /// Army and corps seats run the operational layer.
    Operational,
    /// Division and brigade seats run the tactical layer.
    Tactical,
}

/// A commander position of the AI's side.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Seat {
    /// Seat id (the officer id); ascending ids give the order of work and of draws.
    pub id: SeatId,
    /// The layer it runs.
    pub level: SeatLevel,
    /// The seat above, to which a silent recipient is reported.
    pub parent: Option<SeatId>,
    /// Commander record id; its traits give the personality.
    pub commander: String,
    /// A cavalry seat.
    pub cavalry: bool,
}

/// Per-side AI configuration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AiConfig {
    /// Behaviour level.
    pub difficulty: Difficulty,
    /// Whether the optional advisor is consulted.
    pub advisor: bool,
}

/// One decision of a planner: an order and the values behind it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Decision {
    /// Recipient.
    pub to: UnitRef,
    /// The order.
    pub body: OrderBody,
    /// The plan it belongs to.
    pub plan: PlanKind,
    /// Why.
    pub reason: ReasonCode,
    /// First deciding value.
    pub v0: i64,
    /// Second deciding value.
    pub v1: i64,
}

/// What a planner may read when a seat comes up for decision.
pub struct PlanCx<'a> {
    /// The current tick.
    pub tick: u64,
    /// The deciding seat.
    pub seat: &'a Seat,
    /// Its personality.
    pub personality: &'a Personality,
    /// The difficulty row.
    pub difficulty: &'a DifficultyRow,
    /// The global constants.
    pub globals: &'a AiGlobals,
    /// Own units as last reported, ascending unit id.
    pub reports: &'a [OwnReport],
    /// Contacts, ascending id.
    pub contacts: &'a [Contact],
    /// The public map.
    pub map: &'a MapView,
    /// The stream of this seat's layer (operational or tactical) on this side.
    pub rng: &'a mut Rng,
}

/// A layer's decision procedure. Decisions are queued in order and sent after the think delay.
pub trait Planner {
    /// Decides for one seat.
    fn plan(&mut self, cx: &mut PlanCx<'_>) -> Vec<Decision>;
}

/// The planner of a layer that has not been written yet: it decides nothing.
struct Empty;

impl Planner for Empty {
    fn plan(&mut self, _cx: &mut PlanCx<'_>) -> Vec<Decision> {
        Vec::new()
    }
}

struct SeatState {
    seat: Seat,
    personality: Personality,
}

/// The AI of one side.
pub struct Ai {
    side: u8,
    row: DifficultyRow,
    globals: AiGlobals,
    seats: Vec<SeatState>,
    streams: Vec<Rng>,
    ol: Box<dyn Planner>,
    tl: Box<dyn Planner>,
    outbox: Outbox,
    notes: Vec<AiNote>,
    tick: u64,
    reports: Vec<OwnReport>,
    contacts: Vec<Contact>,
}

impl Ai {
    /// Builds the AI of `side`. `seats` are put in ascending id order (a repeated id keeps its
    /// first entry); `seed` is the master seed all streams derive from.
    pub fn new(side: u8, cfg: AiConfig, tables: &AiTables, seats: &[Seat], seed: u64) -> Ai {
        let mut seats: Vec<Seat> = seats.to_vec();
        seats.sort_by_key(|s| s.id);
        seats.dedup_by_key(|s| s.id);
        let master = RngStreams::new(seed);
        Ai {
            side,
            row: tables.difficulty(cfg.difficulty).clone(),
            globals: tables.globals.clone(),
            seats: seats
                .into_iter()
                .map(|seat| SeatState {
                    personality: personality_for(&seat.commander, tables),
                    seat,
                })
                .collect(),
            streams: (0..LAYERS)
                .map(|layer| master.stream(registry::ai_stream(side, layer)))
                .collect(),
            ol: Box::new(Empty),
            tl: Box::new(Empty),
            outbox: Outbox::default(),
            notes: Vec::new(),
            tick: 0,
            reports: Vec::new(),
            contacts: Vec::new(),
        }
    }

    /// Installs the operational and tactical planners.
    pub fn set_planners(&mut self, ol: Box<dyn Planner>, tl: Box<dyn Planner>) {
        self.ol = ol;
        self.tl = tl;
    }

    /// The side this AI plays.
    pub fn side(&self) -> u8 {
        self.side
    }

    /// Runs one tick: seats due for a decision plan (ascending seat id, drawing from the layer
    /// stream), decisions join the outbox, and the outbox sends, resends and escalates.
    pub fn step(&mut self, port: &mut dyn HqPort) {
        let now = port.now();
        self.tick = now;
        let due: Vec<usize> = (0..self.seats.len())
            .filter(|&i| self.is_due(i, now))
            .collect();
        if !due.is_empty() {
            self.reports.clear();
            self.contacts.clear();
            port.own_reports(&mut self.reports);
            port.contacts(&mut self.contacts);
            let map = port.map();
            for i in due {
                self.plan_seat(i, now, &map);
            }
        }
        let events = self.outbox.step(
            port,
            now,
            self.globals.ack_timeout_ticks,
            self.globals.max_resends,
        );
        for e in events {
            self.note_event(now, e);
        }
    }

    fn is_due(&self, i: usize, now: u64) -> bool {
        let s = &self.seats[i].seat;
        let interval = match s.level {
            SeatLevel::Operational => self.row.op_interval_ticks,
            SeatLevel::Tactical => self.row.tac_interval_ticks,
        };
        (now + u64::from(s.id.0) * 7).is_multiple_of(u64::from(interval))
    }

    fn plan_seat(&mut self, i: usize, now: u64, map: &MapView) {
        let state = &self.seats[i];
        let (planner, layer, think) = match state.seat.level {
            SeatLevel::Operational => (&mut self.ol, LAYER_OL, self.row.op_think_ticks),
            SeatLevel::Tactical => (&mut self.tl, LAYER_TL, self.row.tac_think_ticks),
        };
        let mut cx = PlanCx {
            tick: now,
            seat: &state.seat,
            personality: &state.personality,
            difficulty: &self.row,
            globals: &self.globals,
            reports: &self.reports,
            contacts: &self.contacts,
            map,
            rng: &mut self.streams[usize::from(layer)],
        };
        let decisions = planner.plan(&mut cx);
        let due = now + u64::from(think) + u64::from(state.personality.reaction_ticks);
        for d in decisions {
            let msg = OutMessage {
                from: state.seat.id,
                to: d.to,
                body: d.body,
            };
            if self.outbox.enqueue(due, msg) {
                self.notes.push(AiNote {
                    tick: now,
                    seat: state.seat.id,
                    unit: d.to,
                    plan: d.plan,
                    reason: d.reason,
                    v0: d.v0,
                    v1: d.v1,
                });
            }
        }
    }

    fn note_event(&mut self, now: u64, e: Event) {
        let (seat, unit, plan, reason, v0, v1) = match e {
            Event::Resent {
                seat,
                unit,
                id,
                resends,
            } => (
                seat,
                unit,
                PlanKind::Resend,
                ReasonCode::Resent,
                i64::from(id.0),
                i64::from(resends),
            ),
            Event::Silent {
                seat,
                unit,
                resends,
            } => {
                let parent = self
                    .seats
                    .iter()
                    .find(|s| s.seat.id == seat)
                    .and_then(|s| s.seat.parent);
                (
                    parent.unwrap_or(seat),
                    unit,
                    PlanKind::Escalate,
                    ReasonCode::NoReply,
                    i64::from(seat.0),
                    i64::from(resends),
                )
            }
        };
        self.notes.push(AiNote {
            tick: now,
            seat,
            unit,
            plan,
            reason,
            v0,
            v1,
        });
    }

    /// Every note made so far, in order.
    pub fn notes(&self) -> &[AiNote] {
        &self.notes
    }

    /// The latest note about `unit`.
    pub fn explain(&self, unit: UnitRef) -> Option<&AiNote> {
        self.notes.iter().rev().find(|n| n.unit == unit)
    }

    /// Hash of the AI state: tick, outbox (objective tables, dwell timers, outstanding
    /// messages), stream states and the notes count. Never part of the battle hash.
    pub fn hash(&self) -> u64 {
        let mut h = StateHasher::new();
        h.write_u8(self.side);
        h.write_u64(self.tick);
        self.outbox.hash_into(&mut h);
        for rng in &self.streams {
            for word in rng.state() {
                h.write_u64(word);
            }
        }
        h.write_u64(self.notes.len() as u64);
        h.finish()
    }
}
