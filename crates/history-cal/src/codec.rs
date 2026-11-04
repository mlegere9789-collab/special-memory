//! The Trigger payload (REGISTRY section 2.2, tag `0x31`): `u8 0x31, u16 index`, little-endian.
//!
//! Tag `0x30` (Arrive) stays allocated to SPEC-011 but has no codec: arrivals belong to the
//! SPEC-008 scenario (ADR 0010 decision D-06, amendment A1), so a payload with that tag is
//! rejected here like any other foreign tag.

use battle_rules::registry::TAG_TRIGGER;
use battle_rules::OrderDecodeError;

/// Encodes the trigger at `index` (ascending-id rank of the trigger row) as a replay input payload.
pub fn encode_trigger(index: u16) -> Vec<u8> {
    let mut out = vec![TAG_TRIGGER];
    out.extend_from_slice(&index.to_le_bytes());
    out
}

/// Decodes a payload written by [`encode_trigger`].
///
/// Errors: [`OrderDecodeError::Truncated`] for fewer than three bytes,
/// [`OrderDecodeError::TrailingBytes`] for more, [`OrderDecodeError::UnknownTag`] for a first byte
/// other than `0x31`.
pub fn decode_trigger(bytes: &[u8]) -> Result<u16, OrderDecodeError> {
    match bytes.first() {
        None => Err(OrderDecodeError::Truncated),
        Some(&tag) if tag != TAG_TRIGGER => Err(OrderDecodeError::UnknownTag(tag)),
        Some(_) => match bytes {
            [_, lo, hi] => Ok(u16::from_le_bytes([*lo, *hi])),
            b if b.len() < 3 => Err(OrderDecodeError::Truncated),
            _ => Err(OrderDecodeError::TrailingBytes),
        },
    }
}
