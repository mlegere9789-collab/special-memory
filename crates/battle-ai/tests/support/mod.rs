//! Shared test support: a scripted `HqPort` and scripted planners.
#![allow(dead_code)]

use battle_ai::{
    Ai, AiConfig, AiTables, Contact, Decision, Difficulty, HqPort, MapView, MsgId, MsgStatus,
    OrderBody, OutMessage, OwnReport, PlanCx, PlanKind, Planner, ReasonCode, Seat, SeatId,
    SeatLevel, UnitRef,
};
use battle_rules::{Order, Tables, TerrainGrid};
use core_math::{Fx, Vec2Fx};

pub fn tables() -> AiTables {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/rules");
    AiTables::load_dir(dir).expect("committed AI tables load")
}

pub fn map() -> MapView {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/rules");
    let t = Tables::load_dir(dir).expect("rules tables load");
    let rows = vec!["........"; 8];
    let terrain = TerrainGrid::from_rows(8000, 0, 0, &[('.', 0)], &rows, &t.terrain).unwrap();
    MapView {
        terrain,
        obstacles: Default::default(),
        heights_mm: None,
        key_points: Vec::new(),
    }
}

/// One submission the port saw.
#[derive(Clone, Debug)]
pub struct Sent {
    pub tick: u64,
    pub id: MsgId,
    pub msg: OutMessage,
    pub lost: bool,
}

/// A port whose clock, reports and acknowledgements the test scripts. A lost message never
/// acknowledges; any other acknowledges `ack_delay` ticks after it was submitted.
pub struct ScriptPort {
    pub now: u64,
    pub reports: Vec<OwnReport>,
    pub contacts: Vec<Contact>,
    pub map: MapView,
    pub ack_delay: u64,
    pub lose: Box<dyn FnMut(usize) -> bool>,
    pub log: Vec<Sent>,
    pub calls: std::cell::RefCell<Vec<&'static str>>,
}

impl ScriptPort {
    pub fn new(ack_delay: u64) -> ScriptPort {
        ScriptPort {
            now: 0,
            reports: Vec::new(),
            contacts: Vec::new(),
            map: map(),
            ack_delay,
            lose: Box::new(|_| false),
            log: Vec::new(),
            calls: Default::default(),
        }
    }

    pub fn losing_all(mut self) -> ScriptPort {
        self.lose = Box::new(|_| true);
        self
    }

    fn call(&self, name: &'static str) {
        self.calls.borrow_mut().push(name);
    }
}

impl HqPort for ScriptPort {
    fn now(&self) -> u64 {
        self.call("now");
        self.now
    }
    fn own_reports(&self, out: &mut Vec<OwnReport>) {
        self.call("own_reports");
        out.extend_from_slice(&self.reports);
    }
    fn contacts(&self, out: &mut Vec<Contact>) {
        self.call("contacts");
        out.extend_from_slice(&self.contacts);
    }
    fn map(&self) -> MapView {
        self.call("map");
        self.map.clone()
    }
    fn submit(&mut self, msg: OutMessage) -> MsgId {
        self.call("submit");
        let lost = (self.lose)(self.log.len());
        let id = MsgId(self.log.len() as u32 + 1);
        self.log.push(Sent {
            tick: self.now,
            id,
            msg,
            lost,
        });
        id
    }
    fn status(&self, id: MsgId) -> MsgStatus {
        self.call("status");
        let s = &self.log[id.0 as usize - 1];
        if !s.lost && self.now >= s.tick + self.ack_delay {
            MsgStatus::Acked
        } else {
            MsgStatus::Pending
        }
    }
}

pub fn seat(id: u16, level: SeatLevel, parent: Option<u16>, commander: &str) -> Seat {
    Seat {
        id: SeatId(id),
        level,
        parent: parent.map(SeatId),
        commander: commander.to_owned(),
        cavalry: false,
    }
}

pub fn ai(side: u8, d: Difficulty, seats: &[Seat], seed: u64) -> Ai {
    Ai::new(
        side,
        AiConfig {
            difficulty: d,
            advisor: false,
        },
        &tables(),
        seats,
        seed,
    )
}

/// Orders `unit` to hold at a distinct point every pass, recording a draw of the layer stream as
/// the note's first value.
pub struct Draws {
    pub unit: u16,
    pub n: i64,
}

impl Planner for Draws {
    fn plan(&mut self, cx: &mut PlanCx<'_>) -> Vec<Decision> {
        self.n += 1;
        let draw = i64::from(cx.rng.next_u32());
        vec![Decision {
            to: UnitRef(self.unit),
            body: OrderBody::Reg(Order::Move {
                to: Vec2Fx::new(Fx::from_int(self.n as i32), Fx::from_int(0)),
                gait: battle_rules::Gait::Quick,
            }),
            plan: PlanKind::Place,
            reason: ReasonCode::Assigned,
            v0: draw,
            v1: cx.tick as i64,
        }]
    }
}

/// Decides exactly once, at the first pass.
pub struct Once(pub Option<Decision>);

impl Planner for Once {
    fn plan(&mut self, _cx: &mut PlanCx<'_>) -> Vec<Decision> {
        self.0.take().into_iter().collect()
    }
}

/// Runs `ai` against `port` for ticks `0..=to`.
pub fn run(ai: &mut Ai, port: &mut ScriptPort, to: u64) {
    for t in 0..=to {
        port.now = t;
        ai.step(port);
    }
}
