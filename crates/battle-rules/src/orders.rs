//! Regiment orders and their wire encoding (SPEC-004 section 11).
//!
//! Wire format, little-endian: `u8 tag` (1 Hold, 2 Move, 3 Form, 4 Face), `u16 regiment`, then
//! Move: `i64 x, i64 y` (raw `Fx`), `u8 gait`; Form: `u8 formation`; Face: `u32` raw angle.

use crate::registry::{TAG_FACE, TAG_FORM, TAG_HOLD, TAG_MOVE};
use crate::{Formation, Gait, RegimentId};
use core::fmt;
use core_math::{Angle, Fx, Vec2Fx};

/// A command to a regiment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Order {
    /// Stand where you are.
    Hold,
    /// March the anchor to a point at a gait.
    Move {
        /// Destination in metres.
        to: Vec2Fx,
        /// Regiment gait.
        gait: Gait,
    },
    /// Change formation.
    Form(Formation),
    /// Turn to a heading.
    Face(Angle),
}

impl Order {
    /// Wire and hash tag: Hold 1, Move 2, Form 3, Face 4.
    pub const fn tag(&self) -> u8 {
        match self {
            Order::Hold => TAG_HOLD,
            Order::Move { .. } => TAG_MOVE,
            Order::Form(_) => TAG_FORM,
            Order::Face(_) => TAG_FACE,
        }
    }
}

/// Why a payload is not an order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrderDecodeError {
    /// The payload ended early.
    Truncated,
    /// The payload has bytes after the order.
    TrailingBytes,
    /// Unknown order tag.
    UnknownTag(u8),
    /// Unknown gait number.
    UnknownGait(u8),
    /// Unknown formation tag.
    UnknownFormation(u8),
    /// Unknown dispatch medium number (SPEC-007).
    UnknownMedium(u8),
    /// Unknown dispatch latitude number (SPEC-007).
    UnknownLatitude(u8),
    /// A side, flag or script field with an undefined value (SPEC-007).
    UnknownValue(u8),
}

impl fmt::Display for OrderDecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OrderDecodeError::Truncated => write!(f, "order payload is truncated"),
            OrderDecodeError::TrailingBytes => write!(f, "order payload has trailing bytes"),
            OrderDecodeError::UnknownTag(t) => write!(f, "unknown order tag {t}"),
            OrderDecodeError::UnknownGait(g) => write!(f, "unknown gait {g}"),
            OrderDecodeError::UnknownFormation(g) => write!(f, "unknown formation {g}"),
            OrderDecodeError::UnknownMedium(m) => write!(f, "unknown medium {m}"),
            OrderDecodeError::UnknownLatitude(l) => write!(f, "unknown latitude {l}"),
            OrderDecodeError::UnknownValue(v) => write!(f, "unknown value {v}"),
        }
    }
}

impl std::error::Error for OrderDecodeError {}

/// The gait number used on the wire and in the hash: its position in [`Gait::ALL`].
pub(crate) const fn gait_tag(gait: Gait) -> u8 {
    gait as u8
}

/// Encodes an order addressed to a regiment as a replay input payload.
pub fn encode_order(regiment: RegimentId, order: &Order) -> Vec<u8> {
    let mut out = Vec::with_capacity(20);
    out.push(order.tag());
    out.extend_from_slice(&regiment.0.to_le_bytes());
    match order {
        Order::Hold => {}
        Order::Move { to, gait } => {
            out.extend_from_slice(&to.x.raw().to_le_bytes());
            out.extend_from_slice(&to.y.raw().to_le_bytes());
            out.push(gait_tag(*gait));
        }
        Order::Form(f) => out.push(f.tag()),
        Order::Face(a) => out.extend_from_slice(&a.raw().to_le_bytes()),
    }
    out
}

/// A cursor over the payload bytes.
struct Cursor<'a>(&'a [u8]);

impl<'a> Cursor<'a> {
    fn take<const N: usize>(&mut self) -> Result<[u8; N], OrderDecodeError> {
        let (head, rest) = self
            .0
            .split_first_chunk::<N>()
            .ok_or(OrderDecodeError::Truncated)?;
        self.0 = rest;
        Ok(*head)
    }
}

/// Decodes a payload written by [`encode_order`].
///
/// Errors: [`OrderDecodeError`] for a short payload, an unknown tag, gait or formation, or bytes
/// after the order.
pub fn decode_order(bytes: &[u8]) -> Result<(RegimentId, Order), OrderDecodeError> {
    let mut c = Cursor(bytes);
    let [tag] = c.take::<1>()?;
    let regiment = RegimentId(u16::from_le_bytes(c.take::<2>()?));
    let order = match tag {
        TAG_HOLD => Order::Hold,
        TAG_MOVE => {
            let x = i64::from_le_bytes(c.take::<8>()?);
            let y = i64::from_le_bytes(c.take::<8>()?);
            let [g] = c.take::<1>()?;
            let gait = *Gait::ALL
                .get(usize::from(g))
                .ok_or(OrderDecodeError::UnknownGait(g))?;
            Order::Move {
                to: Vec2Fx::new(Fx::from_raw(x), Fx::from_raw(y)),
                gait,
            }
        }
        TAG_FORM => {
            let [f] = c.take::<1>()?;
            Order::Form(Formation::from_tag(f).ok_or(OrderDecodeError::UnknownFormation(f))?)
        }
        TAG_FACE => Order::Face(Angle::from_raw(u32::from_le_bytes(c.take::<4>()?))),
        other => return Err(OrderDecodeError::UnknownTag(other)),
    };
    if c.0.is_empty() {
        Ok((regiment, order))
    } else {
        Err(OrderDecodeError::TrailingBytes)
    }
}
