//! Battle rules: movement, drill and formations (SPEC-004), later weapons and fire (SPEC-005).
//!
//! This crate holds the shared conventions ([`fx_ratio`], tick rate, stream and flag constants),
//! the integer-only rules-data reader ([`parse_table`], [`Table`], [`Provenance`]) and the
//! [`DataError`] every loader returns. State is `Fx` and integers only; iteration is ascending
//! id; rules data never reach the simulation as floating point (ADR 0004).

#![forbid(unsafe_code)]

mod conventions;
mod error;
mod provenance;
mod reader;

pub use conventions::{
    fx_ratio, F_DISABLED, F_GUN, F_KNEEL, F_PRONE, F_WOUNDED, STREAM_BATTLE_SETUP, TICK_HZ,
};
pub use error::DataError;
pub use provenance::{Confidence, Provenance, SourceRef};
pub use reader::{parse_table, Table};

// T-016
mod fatigue;
mod locomotion;
mod terrain;
pub use fatigue::{fatigue_speed_pm, fatigue_step, fatigue_step_scaled};
pub use locomotion::{CurvePoint, Gait, GaitRow, LocomotionGlobals, LocomotionTable, FATIGUE_MAX};
pub use terrain::{TerrainClass, TerrainGrid, TerrainTable, DEFAULT_CELL_MM};

// T-017
pub mod drill;
pub mod slots;
pub use drill::{
    transition_ticks, DrillGlobals, DrillTable, Formation, FormationRow, Training, TrainingRow,
    TransitionRow,
};
pub use slots::{assign_slots, regiment_roster, slot_layout, SlotLocal};

// T-020
pub mod ammo;
pub mod damage;
pub mod weapons;
pub use ammo::{drag_k, sphere_mass_g, AmmoKind, AmmoRow, AmmoTable, Burst, BurstShape, Constants};
pub use damage::{
    energy_joules, DamageTable, HitOutcome, OutcomeCell, Posture, PostureRow, Region,
};
pub use weapons::{AmmoLoad, LoadStep, WeaponClass, WeaponRow, WeaponTable};

// T-018
mod battle;
mod battle_hash;
mod orders;
mod regiment;
mod tables;
pub use battle::{soldier_stream, BattleState, LocoExt, STREAM_SOLDIER_BASE};
pub use battle_hash::battle_hash;
pub use orders::{decode_order, encode_order, Order, OrderDecodeError};
pub use regiment::{
    can_fire, Regiment, RegimentId, RegimentSpec, RegimentState, RegimentStatus, RoundsAccount,
};
pub use tables::Tables;

// T-021
pub mod flight;
mod gauss;
pub mod range;
mod vec3;
pub use flight::{advance_projectile, step_projectile, Ballistics, Flight, FlightEnv, Projectile};
pub use gauss::{angle_from_urad, gauss_unit};
pub use range::RangeTable;
pub use vec3::Vec3Fx;

// T-019
mod movement;
pub mod scenarios;

// T-026
pub mod registry;

// T-061
pub mod morale;
pub use morale::{MoraleKind, MoraleTable};

// T-071
pub mod command;

// T-022
pub mod aim;
pub mod fire;
pub mod smoke;
pub use fire::{
    decode_fire_mode, encode_fire_mode, misfire_ppm, step_ticks, Counters, FireExt, FireMode,
    Weather,
};
pub use smoke::{SmokeGrid, SMOKE_REF_UNITS};

// T-023
mod burst;
mod casualty;
pub mod hit;
pub use burst::{burst_children, FUSE_INERT, FUSE_NONE, FUSE_PELLET};
pub use casualty::{Casualty, CasualtyKind};
pub use hit::{Obstacle, ObstacleLayer};

// T-081
mod scenario;
pub use scenario::{
    scenario_hash, Arm, Arrival, Departure, MapSpec, Objective, Scenario, ScenarioError, Side,
    StrengthCheck, TerrainSpec, Trigger, TriggerKind, TriggerOrder, Unit, WeatherRow,
};

// T-082
mod fidelity;
mod sector;
pub use fidelity::{
    classify, ArmRow, ClassInput, FidelityClass, FidelityGlobals, FidelityTable, F0, F1, F2,
};
pub use sector::{HotWindow, SectorBody, SectorMap, SectorState};

// T-083
mod aggregate;
pub use aggregate::{condense, hand_over, materialise, materialised_stream, Aggregate, Ledger};

// T-091
mod environment;
pub use environment::{
    apparent_f10, exp_table, EnvTable, Environment, HeatNow, HeatParams, HeatPoint, RainParams,
    RetainRow, SightParams, SmokeParams, WeatherKey, WeatherNow, WeatherTable, EXP_TABLE,
    WIND_MAX_MM_S,
};

// T-094
mod sight;
pub use sight::{
    aim_mm, distance_mm, eye_mm, optical_depth_micro, sight, signal_quality_pm, to_mm,
    ElevationGrid, Sight, SightWorld,
};

// T-024
pub mod calibration;
