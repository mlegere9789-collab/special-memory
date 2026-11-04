//! Applying replay inputs when the command layer is on (SPEC-007 sections 4 and 9).

use super::codec::{decode_dispatch, decode_options, decode_script, ScriptEvent};
use super::{CommandState, Dispatch, OfficerId};
use crate::registry::{
    ACTOR_AI_BASE, TAG_DISPATCH, TAG_FACE, TAG_HOLD, TAG_OPTIONS, TAG_SCRIPT, TAG_SET_FIRE_MODE,
};
use crate::{decode_fire_mode, decode_order, BattleState, OrderDecodeError};
use core::fmt;

/// Why a dispatch cannot be sent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DispatchError {
    /// An officer named does not exist.
    UnknownOfficer(OfficerId),
    /// An officer is not on the side of the seat that sent the input.
    WrongSide,
    /// The sending officer has fallen.
    SenderDown,
}

/// Why an input was rejected; a rejected input changes nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputError {
    /// A direct regiment order from a seat while the command layer is on (use a dispatch).
    DirectOrder,
    /// A command input while the command layer is off.
    NoCommandLayer,
    /// The payload tag is not a command or regiment input.
    UnknownTag(u8),
    /// The payload does not decode.
    Decode(OrderDecodeError),
    /// The actor may not send this input (a script event from a seat, a dispatch from a script,
    /// options of the other side).
    WrongActor,
    /// An officer named does not exist.
    UnknownOfficer(OfficerId),
    /// No link joins the two officers.
    UnknownLink,
    /// The regiment does not exist.
    UnknownRegiment,
    /// A seat addressed a regiment of the other side.
    WrongSide,
    /// The dispatch is not allowed.
    Dispatch(DispatchError),
}

impl fmt::Display for InputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "input rejected: {self:?}")
    }
}

impl std::error::Error for InputError {}

impl From<OrderDecodeError> for InputError {
    fn from(e: OrderDecodeError) -> InputError {
        InputError::Decode(e)
    }
}

/// How an actor is treated: a seat of a side (human or AI), a reserved seat without a side, or
/// not a seat (scripts, advisors, the system).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Actor {
    Seat(Option<u8>),
    Other,
}

fn actor_kind(actor: u8) -> Actor {
    match actor {
        0 | 1 => Actor::Seat(Some(actor)),
        2..=0x5F => Actor::Seat(None),
        a if a == ACTOR_AI_BASE || a == ACTOR_AI_BASE + 1 => Actor::Seat(Some(a - ACTOR_AI_BASE)),
        _ => Actor::Other,
    }
}

impl BattleState {
    /// Applies one replay input of `actor` (the shell calls this for the tick's inputs in
    /// `(actor, seq)` order before the step). A rejected input changes nothing.
    ///
    /// - Tags `0x01` to `0x05` (regiment orders, fire mode): refused with
    ///   [`InputError::DirectOrder`] from a seat while the command layer is on, unless that
    ///   seat's `instant_orders` is set. Otherwise applied through `issue_order` or
    ///   `set_fire_mode`; a seat may only address regiments of its own side.
    /// - `0x10` Dispatch: the sender must be a seat; both officers must exist on the seat's side
    ///   and the sender must not be down. A valid dispatch is accepted here; sending it is the
    ///   work of the media and chain steps.
    /// - `0x12` Options: a seat sets its own side's options, a script actor any side's.
    /// - `0x13` Script: script actors only; marks an officer down or up, or cuts or restores the
    ///   links between two officers.
    pub fn apply_command_input(&mut self, actor: u8, payload: &[u8]) -> Result<(), InputError> {
        let Some(&tag) = payload.first() else {
            return Err(OrderDecodeError::Truncated.into());
        };
        match tag {
            TAG_HOLD..=TAG_FACE | TAG_SET_FIRE_MODE => self.apply_direct(actor, tag, payload),
            TAG_DISPATCH | TAG_OPTIONS | TAG_SCRIPT if self.cmd.is_none() => {
                Err(InputError::NoCommandLayer)
            }
            TAG_DISPATCH => {
                let d = decode_dispatch(payload)?;
                let Actor::Seat(Some(side)) = actor_kind(actor) else {
                    return Err(InputError::WrongActor);
                };
                self.check_dispatch(side, &d).map_err(InputError::Dispatch)
            }
            TAG_OPTIONS => {
                let (side, options) = decode_options(payload)?;
                match actor_kind(actor) {
                    Actor::Seat(s) if s != Some(side) => return Err(InputError::WrongActor),
                    _ => {}
                }
                if let Some(cmd) = self.cmd.as_mut() {
                    cmd.opt[usize::from(side)] = options;
                }
                Ok(())
            }
            TAG_SCRIPT => {
                let event = decode_script(payload)?;
                if matches!(actor_kind(actor), Actor::Seat(_)) {
                    return Err(InputError::WrongActor);
                }
                self.apply_script(event)
            }
            other => Err(InputError::UnknownTag(other)),
        }
    }

    fn apply_direct(&mut self, actor: u8, tag: u8, payload: &[u8]) -> Result<(), InputError> {
        let kind = actor_kind(actor);
        if let (Actor::Seat(side), Some(cmd)) = (kind, self.cmd.as_ref()) {
            if !side.is_some_and(|s| cmd.opt[usize::from(s)].instant_orders) {
                return Err(InputError::DirectOrder);
            }
        }
        let (regiment, apply) = if tag == TAG_SET_FIRE_MODE {
            let (r, mode) = decode_fire_mode(payload)?;
            (r, Direct::Fire(mode))
        } else {
            let (r, order) = decode_order(payload)?;
            (r, Direct::Order(order))
        };
        let team = self
            .regiments
            .get(usize::from(regiment.0))
            .map(|r| r.team)
            .ok_or(InputError::UnknownRegiment)?;
        if let Actor::Seat(side) = kind {
            if side != Some(team) {
                return Err(InputError::WrongSide);
            }
        }
        match apply {
            Direct::Order(order) => self.issue_order(regiment, order),
            Direct::Fire(mode) => self.set_fire_mode(regiment, mode),
        };
        Ok(())
    }

    /// Checks a dispatch of the seat of `side` against the officers.
    pub(crate) fn check_dispatch(&self, side: u8, d: &Dispatch) -> Result<(), DispatchError> {
        let Some(cmd) = self.cmd.as_ref() else {
            return Err(DispatchError::UnknownOfficer(d.from));
        };
        let find = |id: OfficerId| {
            cmd.index_of(id)
                .map(|i| &cmd.officers[i])
                .ok_or(DispatchError::UnknownOfficer(id))
        };
        let (from, to) = (find(d.from)?, find(d.to)?);
        if from.side != side || to.side != side {
            return Err(DispatchError::WrongSide);
        }
        if from.down {
            return Err(DispatchError::SenderDown);
        }
        Ok(())
    }

    fn apply_script(&mut self, event: ScriptEvent) -> Result<(), InputError> {
        let Some(cmd) = self.cmd.as_mut() else {
            return Err(InputError::NoCommandLayer);
        };
        let find = |cmd: &CommandState, id: OfficerId| {
            cmd.index_of(id).ok_or(InputError::UnknownOfficer(id))
        };
        match event {
            ScriptEvent::OfficerDown(a) | ScriptEvent::OfficerUp(a) => {
                let i = find(cmd, a)?;
                cmd.officers[i].down = matches!(event, ScriptEvent::OfficerDown(_));
            }
            ScriptEvent::LinkCut(a, b) | ScriptEvent::LinkRestore(a, b) => {
                find(cmd, a)?;
                find(cmd, b)?;
                let (lo, hi) = (a.min(b), a.max(b));
                let cut = matches!(event, ScriptEvent::LinkCut(..));
                let mut found = false;
                for l in cmd.links.iter_mut().filter(|l| (l.a, l.b) == (lo, hi)) {
                    l.cut = cut;
                    found = true;
                }
                if !found {
                    return Err(InputError::UnknownLink);
                }
            }
        }
        Ok(())
    }
}

enum Direct {
    Order(crate::Order),
    Fire(crate::FireMode),
}
