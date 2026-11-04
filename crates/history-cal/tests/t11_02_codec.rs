//! T11-02: the Trigger payload (tag 0x31) round-trips, short and long payloads are refused, the
//! retired Arrive tag (0x30) is not a trigger, and the SPEC-004/005 order payloads are unchanged
//! (SPEC-011 section 8 and amendment A1).

use battle_rules::registry::{TAG_ARRIVE, TAG_TRIGGER};
use battle_rules::{
    decode_order, encode_fire_mode, encode_order, FireMode, Formation, Gait, Order,
    OrderDecodeError, RegimentId,
};
use core_math::{Angle, Fx, Vec2Fx};
use history_cal::{decode_trigger, encode_trigger};
use proptest::prelude::*;

proptest! {
    #[test]
    fn trigger_round_trips(index in any::<u16>()) {
        let bytes = encode_trigger(index);
        prop_assert_eq!(bytes.len(), 3);
        prop_assert_eq!(bytes[0], TAG_TRIGGER);
        prop_assert_eq!(decode_trigger(&bytes), Ok(index));
    }

    #[test]
    fn any_cut_or_extension_is_refused(index in any::<u16>(), extra in any::<u8>()) {
        let bytes = encode_trigger(index);
        for cut in 0..bytes.len() {
            prop_assert_eq!(decode_trigger(&bytes[..cut]), Err(OrderDecodeError::Truncated));
        }
        let mut longer = bytes;
        longer.push(extra);
        prop_assert_eq!(decode_trigger(&longer), Err(OrderDecodeError::TrailingBytes));
    }
}

#[test]
fn trigger_layout_is_tag_then_little_endian_index() {
    assert_eq!(encode_trigger(0x0102), vec![0x31, 0x02, 0x01]);
}

#[test]
fn other_tags_are_not_triggers() {
    for tag in [0x00, 0x01, 0x05, TAG_ARRIVE, 0x32, 0xFD] {
        assert_eq!(
            decode_trigger(&[tag, 0, 0]),
            Err(OrderDecodeError::UnknownTag(tag))
        );
    }
}

#[test]
fn order_payloads_of_tags_1_to_5_are_unchanged() {
    let r = RegimentId(0x0201);
    assert_eq!(encode_order(r, &Order::Hold), vec![1, 1, 2]);
    assert_eq!(
        encode_order(r, &Order::Form(Formation::Line)),
        vec![3, 1, 2, Formation::Line.tag()]
    );
    let face = Order::Face(Angle::from_raw(0x0403_0201));
    assert_eq!(encode_order(r, &face), vec![4, 1, 2, 1, 2, 3, 4]);
    let to = Vec2Fx::new(Fx::from_raw(0x0102), Fx::from_raw(-1));
    let mv = encode_order(
        r,
        &Order::Move {
            to,
            gait: Gait::Quick,
        },
    );
    assert_eq!(mv.len(), 20);
    assert_eq!(&mv[..5], &[2, 1, 2, 2, 1]);
    assert_eq!(decode_order(&mv).unwrap().0, r);
    let fire = encode_fire_mode(
        r,
        FireMode::AtWill {
            target: RegimentId(7),
        },
    );
    assert_eq!(fire, vec![5, 1, 2, 1, 7, 0]);
}
