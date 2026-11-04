//! The order wire format (SPEC-004 section 11): round trip and rejection of bad payloads.

use battle_rules::{
    decode_order, encode_order, Formation, Gait, Order, OrderDecodeError, RegimentId,
};
use core_math::{Angle, Fx, Vec2Fx};
use proptest::prelude::*;

fn order_strategy() -> impl Strategy<Value = Order> {
    prop_oneof![
        Just(Order::Hold),
        (any::<i64>(), any::<i64>(), 0usize..4).prop_map(|(x, y, g)| Order::Move {
            to: Vec2Fx::new(Fx::from_raw(x), Fx::from_raw(y)),
            gait: Gait::ALL[g],
        }),
        (0usize..3).prop_map(|f| Order::Form(Formation::ALL[f])),
        any::<u32>().prop_map(|a| Order::Face(Angle::from_raw(a))),
    ]
}

proptest! {
    #[test]
    fn round_trip(id in any::<u16>(), order in order_strategy()) {
        let bytes = encode_order(RegimentId(id), &order);
        prop_assert_eq!(decode_order(&bytes), Ok((RegimentId(id), order)));
    }

    #[test]
    fn every_proper_prefix_is_truncated(id in any::<u16>(), order in order_strategy()) {
        let bytes = encode_order(RegimentId(id), &order);
        for n in 0..bytes.len() {
            prop_assert_eq!(decode_order(&bytes[..n]), Err(OrderDecodeError::Truncated));
        }
    }

    #[test]
    fn extra_bytes_are_refused(id in any::<u16>(), order in order_strategy(), extra in any::<u8>()) {
        let mut bytes = encode_order(RegimentId(id), &order);
        bytes.push(extra);
        prop_assert_eq!(decode_order(&bytes), Err(OrderDecodeError::TrailingBytes));
    }
}

#[test]
fn exact_bytes() {
    let r = RegimentId(0x0102);
    assert_eq!(encode_order(r, &Order::Hold), [1, 2, 1]);
    let mv = Order::Move {
        to: Vec2Fx::new(Fx::from_raw(0x0807_0605_0403_0201), Fx::from_raw(-1)),
        gait: Gait::DoubleQuick,
    };
    assert_eq!(
        encode_order(r, &mv),
        [2, 2, 1, 1, 2, 3, 4, 5, 6, 7, 8, 255, 255, 255, 255, 255, 255, 255, 255, 2]
    );
    assert_eq!(
        encode_order(r, &Order::Form(Formation::Skirmish)),
        [3, 2, 1, 2]
    );
    assert_eq!(
        encode_order(r, &Order::Face(Angle::from_raw(0xA0B0_C0D0))),
        [4, 2, 1, 0xD0, 0xC0, 0xB0, 0xA0]
    );
}

#[test]
fn unknown_values_are_named() {
    assert_eq!(
        decode_order(&[0, 0, 0]),
        Err(OrderDecodeError::UnknownTag(0))
    );
    assert_eq!(
        decode_order(&[5, 0, 0]),
        Err(OrderDecodeError::UnknownTag(5))
    );
    assert_eq!(
        decode_order(&[3, 0, 0, 3]),
        Err(OrderDecodeError::UnknownFormation(3))
    );
    let mut mv = encode_order(
        RegimentId(0),
        &Order::Move {
            to: Vec2Fx::ZERO,
            gait: Gait::Walk,
        },
    );
    *mv.last_mut().unwrap() = 4;
    assert_eq!(decode_order(&mv), Err(OrderDecodeError::UnknownGait(4)));
    assert_eq!(decode_order(&[]), Err(OrderDecodeError::Truncated));
}
