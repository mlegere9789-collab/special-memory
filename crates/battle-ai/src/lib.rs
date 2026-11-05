//! Battle AI (SPEC-010). This part holds the data layer (the AI tables [`AiTables`] and the
//! personality [`personality_for`] yields), the headquarters port the AI reads and writes
//! ([`HqPort`]), the seats and streams of one side ([`Ai`]), decision notes ([`AiNote`]) and the
//! outbox that gives messages their think delay, resends and escalation. The layers and the
//! lookahead plug in through [`Planner`] in later tickets.
//!
//! State is integers only; iteration is ascending id; no `HashMap`; no wall-clock.

#![forbid(unsafe_code)]

mod ai;
mod belief;
mod lookahead;
mod notes;
mod outbox;
mod personality;
mod port;
mod tables;

pub use ai::{
    Ai, AiConfig, Decision, PlanCx, Planner, Seat, SeatLevel, ADVISOR_ACTOR_BASE, AI_ACTOR_BASE,
    STREAM_AI_BASE,
};
pub use notes::{AiNote, PlanKind, ReasonCode};
pub use port::{
    Contact, ContactId, FireOrder, HqPort, KeyPoint, MapView, MsgId, MsgStatus, ObjectiveBody,
    ObjectiveKind, OrderBody, OutMessage, OwnReport, SeatId, UnitRef,
};

pub use belief::{
    Area, BeliefView, Foe, Frame, GridSpec, BELIEF_MAX_SOLDIERS, BELIEF_RADIUS_MM,
    EST_LATENCY_TICKS,
};
pub use lookahead::{
    aggregate_scores, build_belief, forecast, plan_budget, score_sample, tactical_budget,
    BudgetPlan, EnemyModel, ForecastResult, Script, ScriptItem, HORIZON_MAX, HORIZON_MIN,
    TL_BUDGET_DIV, TL_CANDIDATES,
};
pub use personality::{personality_for, Personality};
pub use tables::{
    AiGlobals, AiTables, Bounds, Difficulty, DifficultyRow, RelationFact, RelationKind, Trait,
    TraitFact, TraitRow,
};
