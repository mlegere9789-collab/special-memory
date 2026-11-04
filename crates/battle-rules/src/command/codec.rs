//! Wire format of the command inputs (SPEC-007 section 9, REGISTRY 2.2).
//!
//! - Dispatch `0x10`: `u8 0x10, u8 medium (0 auto, 1 courier, 2 flags, 3 telegraph), u8 latitude,
//!   u16 from`, then the unchanged [`encode_order`] bytes with the addressee officer in the
//!   regiment field.
//! - Options `0x12`: `u8 0x12, u8 side, u8 flags` (bit 0 omniscient view, bit 1 instant orders).
//! - Script `0x13`: `u8 0x13, u8 kind (1 officer down, 2 officer up, 3 link cut, 4 link restore),
//!   u16 a, u16 b`; `b` is zero for the two officer events.
//!
//! Decoders reject truncation, trailing bytes and unknown values.

use super::{CommandOptions, Latitude, Medium, OfficerId};
use crate::registry::{
    OPT_INSTANT_ORDERS, OPT_OMNISCIENT_VIEW, TAG_DISPATCH, TAG_OPTIONS, TAG_SCRIPT,
};
use crate::{decode_order, encode_order, Order, OrderDecodeError, RegimentId};

/// A decoded dispatch: who sends which order to whom, by which medium.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Dispatch {
    /// Sending officer.
    pub from: OfficerId,
    /// Addressee officer.
    pub to: OfficerId,
    /// The medium, or `None` to let the rules choose.
    pub medium: Option<Medium>,
    /// How strictly the order binds.
    pub latitude: Latitude,
    /// The order.
    pub order: Order,
}

/// Encodes a dispatch; `medium` is 0 for automatic or a [`Medium::tag`].
pub fn encode_dispatch(
    from: OfficerId,
    to: OfficerId,
    medium: u8,
    latitude: Latitude,
    order: &Order,
) -> Vec<u8> {
    let mut out = vec![TAG_DISPATCH, medium, latitude.tag()];
    out.extend_from_slice(&from.0.to_le_bytes());
    out.extend_from_slice(&encode_order(RegimentId(to.0), order));
    out
}

/// Decodes a payload written by [`encode_dispatch`].
///
/// Errors: [`OrderDecodeError`] for truncation, trailing bytes, a wrong leading tag, an unknown
/// medium or latitude, or an invalid order tail (tags 1 to 4 only).
pub fn decode_dispatch(bytes: &[u8]) -> Result<Dispatch, OrderDecodeError> {
    let Some((&[tag, medium, latitude, f0, f1], tail)) = bytes.split_first_chunk::<5>() else {
        return Err(OrderDecodeError::Truncated);
    };
    if tag != TAG_DISPATCH {
        return Err(OrderDecodeError::UnknownTag(tag));
    }
    let medium = match medium {
        0 => None,
        m => Some(Medium::from_tag(m).ok_or(OrderDecodeError::UnknownMedium(m))?),
    };
    let latitude =
        Latitude::from_tag(latitude).ok_or(OrderDecodeError::UnknownLatitude(latitude))?;
    let (to, order) = decode_order(tail)?;
    Ok(Dispatch {
        from: OfficerId(u16::from_le_bytes([f0, f1])),
        to: OfficerId(to.0),
        medium,
        latitude,
        order,
    })
}

/// Encodes the options of one side.
pub fn encode_options(side: u8, options: CommandOptions) -> Vec<u8> {
    let flags = if options.omniscient_view {
        OPT_OMNISCIENT_VIEW
    } else {
        0
    } | if options.instant_orders {
        OPT_INSTANT_ORDERS
    } else {
        0
    };
    vec![TAG_OPTIONS, side, flags]
}

/// Decodes a payload written by [`encode_options`] into `(side, options)`.
///
/// Errors: [`OrderDecodeError`]; a side above 1 or an undefined flag bit is `UnknownValue`.
pub fn decode_options(bytes: &[u8]) -> Result<(u8, CommandOptions), OrderDecodeError> {
    let &[tag, side, flags] = fixed::<3>(bytes)?;
    if tag != TAG_OPTIONS {
        return Err(OrderDecodeError::UnknownTag(tag));
    }
    if side > 1 {
        return Err(OrderDecodeError::UnknownValue(side));
    }
    if flags & !(OPT_OMNISCIENT_VIEW | OPT_INSTANT_ORDERS) != 0 {
        return Err(OrderDecodeError::UnknownValue(flags));
    }
    Ok((
        side,
        CommandOptions {
            omniscient_view: flags & OPT_OMNISCIENT_VIEW != 0,
            instant_orders: flags & OPT_INSTANT_ORDERS != 0,
        },
    ))
}

/// A scenario script event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScriptEvent {
    /// The officer falls (kind 1).
    OfficerDown(OfficerId),
    /// The officer is up again (kind 2).
    OfficerUp(OfficerId),
    /// The links between two officers are cut (kind 3).
    LinkCut(OfficerId, OfficerId),
    /// The links between two officers are restored (kind 4).
    LinkRestore(OfficerId, OfficerId),
}

/// Encodes a script event.
pub fn encode_script(event: ScriptEvent) -> Vec<u8> {
    let (kind, a, b) = match event {
        ScriptEvent::OfficerDown(a) => (1u8, a, OfficerId(0)),
        ScriptEvent::OfficerUp(a) => (2, a, OfficerId(0)),
        ScriptEvent::LinkCut(a, b) => (3, a, b),
        ScriptEvent::LinkRestore(a, b) => (4, a, b),
    };
    let mut out = vec![TAG_SCRIPT, kind];
    out.extend_from_slice(&a.0.to_le_bytes());
    out.extend_from_slice(&b.0.to_le_bytes());
    out
}

/// Decodes a payload written by [`encode_script`].
///
/// Errors: [`OrderDecodeError`]; an unknown kind, or a nonzero second officer on an officer event,
/// is `UnknownValue`.
pub fn decode_script(bytes: &[u8]) -> Result<ScriptEvent, OrderDecodeError> {
    let &[tag, kind, a0, a1, b0, b1] = fixed::<6>(bytes)?;
    if tag != TAG_SCRIPT {
        return Err(OrderDecodeError::UnknownTag(tag));
    }
    let (a, b) = (
        OfficerId(u16::from_le_bytes([a0, a1])),
        OfficerId(u16::from_le_bytes([b0, b1])),
    );
    match kind {
        1 | 2 if b0 | b1 != 0 => Err(OrderDecodeError::UnknownValue(b0 | b1)),
        1 => Ok(ScriptEvent::OfficerDown(a)),
        2 => Ok(ScriptEvent::OfficerUp(a)),
        3 => Ok(ScriptEvent::LinkCut(a, b)),
        4 => Ok(ScriptEvent::LinkRestore(a, b)),
        k => Err(OrderDecodeError::UnknownValue(k)),
    }
}

fn fixed<const N: usize>(bytes: &[u8]) -> Result<&[u8; N], OrderDecodeError> {
    match bytes.len() {
        n if n < N => Err(OrderDecodeError::Truncated),
        n if n > N => Err(OrderDecodeError::TrailingBytes),
        _ => Ok(bytes.first_chunk::<N>().unwrap_or_else(|| unreachable!())),
    }
}
