//! T7-02 (SPEC-007 sections 9 and 13): the dispatch, options and script payloads round-trip, the
//! dispatch tail is exactly the SPEC-004 order bytes, and bad payloads are refused.

use battle_rules::command::{
    decode_dispatch, decode_options, decode_script, encode_dispatch, encode_options, encode_script,
    CommandOptions, Dispatch, Latitude, Medium, OfficerId, ScriptEvent,
};
use battle_rules::{encode_order, Formation, Gait, Order, OrderDecodeError, RegimentId};
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

fn latitude(n: u8) -> Latitude {
    Latitude::from_tag(n).unwrap()
}

proptest! {
    #[test]
    fn dispatch_round_trip(
        from in any::<u16>(), to in any::<u16>(), medium in 0u8..4, lat in 0u8..2,
        order in order_strategy(),
    ) {
        let bytes = encode_dispatch(OfficerId(from), OfficerId(to), medium, latitude(lat), &order);
        prop_assert_eq!(
            decode_dispatch(&bytes),
            Ok(Dispatch {
                from: OfficerId(from),
                to: OfficerId(to),
                medium: Medium::from_tag(medium),
                latitude: latitude(lat),
                order,
            })
        );
        prop_assert_eq!(&bytes[5..], &encode_order(RegimentId(to), &order)[..]);
        for n in 0..bytes.len() {
            prop_assert_eq!(decode_dispatch(&bytes[..n]), Err(OrderDecodeError::Truncated));
        }
        let mut longer = bytes.clone();
        longer.push(0);
        prop_assert_eq!(decode_dispatch(&longer), Err(OrderDecodeError::TrailingBytes));
    }
}

#[test]
fn dispatch_exact_bytes() {
    let bytes = encode_dispatch(
        OfficerId(0x0102),
        OfficerId(0x0304),
        2,
        Latitude::Discretionary,
        &Order::Form(Formation::Skirmish),
    );
    assert_eq!(bytes, [0x10, 2, 1, 2, 1, 3, 4, 3, 2]);
}

#[test]
fn dispatch_refuses_unknown_values() {
    let good = encode_dispatch(
        OfficerId(1),
        OfficerId(2),
        1,
        Latitude::Strict,
        &Order::Hold,
    );
    let with = |i: usize, v: u8| {
        let mut b = good.clone();
        b[i] = v;
        decode_dispatch(&b)
    };
    assert_eq!(with(1, 4), Err(OrderDecodeError::UnknownMedium(4)));
    assert_eq!(with(2, 2), Err(OrderDecodeError::UnknownLatitude(2)));
    assert_eq!(with(0, 0x11), Err(OrderDecodeError::UnknownTag(0x11)));
    assert_eq!(with(5, 5), Err(OrderDecodeError::UnknownTag(5)));
    assert_eq!(with(5, 0), Err(OrderDecodeError::UnknownTag(0)));
}

#[test]
fn options_round_trip_and_refusals() {
    for side in 0..2u8 {
        for (o, i) in [(false, false), (true, false), (false, true), (true, true)] {
            let opt = CommandOptions {
                omniscient_view: o,
                instant_orders: i,
            };
            let bytes = encode_options(side, opt);
            assert_eq!(bytes, [0x12, side, u8::from(o) | (u8::from(i) << 1)]);
            assert_eq!(decode_options(&bytes), Ok((side, opt)));
        }
    }
    assert_eq!(decode_options(&[0x12, 0]), Err(OrderDecodeError::Truncated));
    assert_eq!(
        decode_options(&[0x12, 0, 0, 0]),
        Err(OrderDecodeError::TrailingBytes)
    );
    assert_eq!(
        decode_options(&[0x12, 2, 0]),
        Err(OrderDecodeError::UnknownValue(2))
    );
    assert_eq!(
        decode_options(&[0x12, 0, 4]),
        Err(OrderDecodeError::UnknownValue(4))
    );
    assert_eq!(
        decode_options(&[0x13, 0, 0]),
        Err(OrderDecodeError::UnknownTag(0x13))
    );
}

#[test]
fn script_round_trip_and_refusals() {
    let (a, b) = (OfficerId(7), OfficerId(9));
    let cases = [
        (ScriptEvent::OfficerDown(a), [0x13, 1, 7, 0, 0, 0]),
        (ScriptEvent::OfficerUp(a), [0x13, 2, 7, 0, 0, 0]),
        (ScriptEvent::LinkCut(a, b), [0x13, 3, 7, 0, 9, 0]),
        (ScriptEvent::LinkRestore(a, b), [0x13, 4, 7, 0, 9, 0]),
    ];
    for (event, bytes) in cases {
        assert_eq!(encode_script(event), bytes);
        assert_eq!(decode_script(&bytes), Ok(event));
    }
    assert_eq!(
        decode_script(&[0x13, 1, 7, 0, 0]),
        Err(OrderDecodeError::Truncated)
    );
    assert_eq!(
        decode_script(&[0x13, 1, 7, 0, 0, 0, 0]),
        Err(OrderDecodeError::TrailingBytes)
    );
    for kind in [0u8, 5, 255] {
        assert_eq!(
            decode_script(&[0x13, kind, 7, 0, 9, 0]),
            Err(OrderDecodeError::UnknownValue(kind))
        );
    }
    // An officer event has no second officer.
    assert_eq!(
        decode_script(&[0x13, 1, 7, 0, 9, 0]),
        Err(OrderDecodeError::UnknownValue(9))
    );
}
