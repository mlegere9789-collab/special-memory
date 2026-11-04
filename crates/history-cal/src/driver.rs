//! The replay driver (SPEC-011 section 4.2): per tick, apply the inputs of that tick in `seq`
//! order, then step the battle. Trigger inputs are logged and handed to a hook; the log joins the
//! battle hash in [`ReplayDriver::history_hash`]. The driver draws no random numbers.
//!
//! The driver wraps a [`BattleState`] (fidelity class 0). SPEC-011 amendment A1 has it wrap the
//! SPEC-008 `Battle`; that wrapper is not in the tree yet.

use crate::codec::decode_trigger;
use crate::compile::ScriptInput;
use crate::observe::{Observation, Observers};
use crate::{ReplayScript, ScriptError, UnitRecord};
use battle_rules::registry::{
    ACTOR_HISTORY_SCRIPT, TAG_FACE, TAG_FORM, TAG_HOLD, TAG_MOVE, TAG_SET_FIRE_MODE, TAG_TRIGGER,
};
use battle_rules::{
    decode_fire_mode, decode_order, BattleState, Casualty, FireMode, OrderDecodeError, RegimentId,
    TICK_HZ,
};
use sim_state::{Replay, ReplayError, ReplayWriter, StateHasher};

/// Why the driver refused an input or could not write a replay.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DriverError {
    /// A payload of a known tag that does not decode.
    Payload(OrderDecodeError),
    /// A payload whose tag the driver does not apply (including the retired Arrive tag `0x30`).
    UnknownTag(u8),
    /// An order or fire mode addressed to a regiment that does not exist.
    UnknownRegiment(u16),
    /// A trigger index beyond the script's trigger rows.
    UnknownTrigger(u16),
    /// An input is dated before the battle's current tick.
    PastInput {
        /// The input's tick.
        tick: u64,
    },
    /// The replay could not be built.
    Replay(ReplayError),
}

impl core::fmt::Display for DriverError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            DriverError::Payload(e) => write!(f, "bad input payload: {e}"),
            DriverError::UnknownTag(t) => write!(f, "input tag {t:#04x} is not applied"),
            DriverError::UnknownRegiment(r) => write!(f, "no regiment {r}"),
            DriverError::UnknownTrigger(i) => write!(f, "no trigger {i}"),
            DriverError::PastInput { tick } => write!(f, "input at tick {tick} is already past"),
            DriverError::Replay(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for DriverError {}

impl From<OrderDecodeError> for DriverError {
    fn from(e: OrderDecodeError) -> Self {
        DriverError::Payload(e)
    }
}

/// Called when a trigger input fires, after it is logged and before the tick's step.
pub trait TriggerHook {
    /// `index` is the rank of the trigger row in ascending id order.
    fn on_trigger(&mut self, state: &mut BattleState, index: u16);
}

/// The default hook (`marker`): the trigger is logged and nothing else happens.
#[derive(Clone, Copy, Debug, Default)]
pub struct MarkerHook;

impl TriggerHook for MarkerHook {
    fn on_trigger(&mut self, _state: &mut BattleState, _index: u16) {}
}

/// Which time of a trigger row a run uses (SPEC-011 section 4.5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Variant {
    /// The row's `at_s`.
    Nominal,
    /// `early_s` where the row has one.
    Early,
    /// `late_s` where the row has one.
    Late,
}

impl Variant {
    /// The name used in the replay header, `<scenario>/<variant>`.
    pub const fn name(self) -> &'static str {
        match self {
            Variant::Nominal => "nominal",
            Variant::Early => "early",
            Variant::Late => "late",
        }
    }

    /// The variant called `name`.
    pub fn from_name(name: &str) -> Option<Variant> {
        [Variant::Nominal, Variant::Early, Variant::Late]
            .into_iter()
            .find(|v| v.name() == name)
    }
}

/// The script with every trigger row moved to its `variant` time; rows without that time keep
/// `at_s`.
pub fn apply_variant(script: &ReplayScript, variant: Variant) -> ReplayScript {
    let mut out = script.clone();
    for t in &mut out.triggers {
        let chosen = match variant {
            Variant::Nominal => None,
            Variant::Early => t.early_s,
            Variant::Late => t.late_s,
        };
        t.at_s = chosen.unwrap_or(t.at_s);
    }
    out
}

/// Applies compiled script inputs to a battle tick by tick.
pub struct ReplayDriver {
    state: BattleState,
    inputs: Vec<ScriptInput>,
    next: usize,
    triggers: usize,
    log: Vec<(u64, u16)>,
    hook: Box<dyn TriggerHook>,
    clock_start_s: i64,
    observers: Observers,
}

impl ReplayDriver {
    /// A driver over `state` for `inputs` (sorted here by `(tick, seq)`, stably) and the trigger
    /// rows of `script`.
    ///
    /// Errors: [`DriverError::PastInput`] for an input dated before the state's tick.
    pub fn new(
        state: BattleState,
        mut inputs: Vec<ScriptInput>,
        script: &ReplayScript,
    ) -> Result<ReplayDriver, DriverError> {
        inputs.sort_by_key(|i| (i.tick, i.seq));
        if let Some(i) = inputs.first().filter(|i| i.tick < state.world.tick) {
            return Err(DriverError::PastInput { tick: i.tick });
        }
        Ok(ReplayDriver {
            state,
            inputs,
            next: 0,
            triggers: script.triggers.len(),
            log: Vec::new(),
            hook: Box::new(MarkerHook),
            clock_start_s: script.meta.clock_start_s,
            observers: Observers::from_script(script),
        })
    }

    /// Replaces the trigger hook.
    pub fn set_hook(&mut self, hook: Box<dyn TriggerHook>) {
        self.hook = hook;
    }

    /// Clock second of tick 0.
    pub fn clock_start_s(&self) -> i64 {
        self.clock_start_s
    }

    /// Starts watching the script's `[[observe]]` rows over `units`, before the first step.
    /// Casualties already in the log are scanned; a `reached` or loss-rate condition is first
    /// evaluated after the next step.
    ///
    /// Errors: [`ScriptError::Reference`] for a unit or trigger a row names that does not exist.
    pub fn observe(&mut self, units: &[UnitRecord]) -> Result<(), ScriptError> {
        self.observers.bind(units)
    }

    /// What each observation row has seen so far, ascending row id (empty until
    /// [`ReplayDriver::observe`]).
    pub fn observations(&self) -> Vec<Observation> {
        self.observers.report(&self.log)
    }

    /// The battle.
    pub fn state(&self) -> &BattleState {
        &self.state
    }

    /// The casualty log of the battle.
    pub fn casualties(&self) -> &[Casualty] {
        &self.state.casualties
    }

    /// Every trigger that fired as `(tick, index)`, in order.
    pub fn timeline_log(&self) -> &[(u64, u16)] {
        &self.log
    }

    /// FNV-1a 64 of the battle hash, the `u32` log length, then each log entry as `u64` tick and
    /// `u16` index, little-endian.
    pub fn history_hash(&self) -> u64 {
        let mut h = StateHasher::new();
        h.write_u64(self.state.hash());
        h.write_u32(u32::try_from(self.log.len()).unwrap_or(u32::MAX));
        for &(tick, index) in &self.log {
            h.write_u64(tick);
            h.write_u16(index);
        }
        h.finish()
    }

    fn apply(&mut self, tick: u64, payload: &[u8]) -> Result<(), DriverError> {
        let unknown = |ok: bool, r: u16| {
            if ok {
                Ok(())
            } else {
                Err(DriverError::UnknownRegiment(r))
            }
        };
        match payload.first().copied() {
            None => Err(OrderDecodeError::Truncated.into()),
            Some(TAG_TRIGGER) => {
                let index = decode_trigger(payload)?;
                if usize::from(index) >= self.triggers {
                    return Err(DriverError::UnknownTrigger(index));
                }
                self.log.push((tick, index));
                self.hook.on_trigger(&mut self.state, index);
                Ok(())
            }
            Some(TAG_HOLD | TAG_MOVE | TAG_FORM | TAG_FACE) => {
                let (id, order) = decode_order(payload)?;
                unknown(self.state.issue_order(id, order), id.0)
            }
            Some(TAG_SET_FIRE_MODE) => {
                // The compiler writes `u8 5, u16 regiment, u16 target` (amendment A1); the SPEC-005
                // encoder writes six bytes with a mode byte.
                let (id, mode) = match payload {
                    &[_, r0, r1, t0, t1] => (
                        RegimentId(u16::from_le_bytes([r0, r1])),
                        FireMode::AtWill {
                            target: RegimentId(u16::from_le_bytes([t0, t1])),
                        },
                    ),
                    _ => decode_fire_mode(payload)?,
                };
                unknown(self.state.set_fire_mode(id, mode), id.0)
            }
            Some(other) => Err(DriverError::UnknownTag(other)),
        }
    }

    /// One tick: applies the inputs dated at the battle's tick in `seq` order, then steps.
    ///
    /// Errors: whatever [`DriverError`] the first refused input gives; that input is not skipped,
    /// so the next call fails the same way.
    pub fn step(&mut self) -> Result<(), DriverError> {
        let tick = self.state.world.tick;
        while let Some(input) = self.inputs.get(self.next) {
            if input.tick > tick {
                break;
            }
            let payload = input.payload.clone();
            self.apply(tick, &payload)?;
            self.next += 1;
        }
        self.state.step();
        self.observers.update(&self.state);
        Ok(())
    }

    /// Steps until the battle's tick is `tick` (no-op when it is already there or past).
    pub fn run_to(&mut self, tick: u64) -> Result<(), DriverError> {
        while self.state.world.tick < tick {
            self.step()?;
        }
        Ok(())
    }

    /// Runs to `total_ticks` and writes the replay: header `scenario_id` (`<scenario>/<variant>`),
    /// every input under actor `0xFD`, a checkpoint of [`ReplayDriver::history_hash`] every
    /// `interval` ticks (none when `interval` is 0) and the final history hash.
    pub fn record(
        mut self,
        scenario_id: &str,
        seed: u64,
        fidelity_class: u8,
        total_ticks: u64,
        interval: u32,
    ) -> Result<Replay, DriverError> {
        let mut w = ReplayWriter::new(scenario_id, seed, TICK_HZ as u16, fidelity_class, interval);
        for i in &self.inputs {
            w.push_input(i.tick, ACTOR_HISTORY_SCRIPT, i.seq, i.payload.clone());
        }
        while self.state.world.tick < total_ticks {
            self.step()?;
            let tick = self.state.world.tick;
            if interval > 0 && tick.is_multiple_of(u64::from(interval)) {
                w.push_checkpoint(tick, self.history_hash());
            }
        }
        w.finish(total_ticks, self.history_hash())
            .map_err(DriverError::Replay)
    }
}
