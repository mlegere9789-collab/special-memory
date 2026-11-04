//! The flag and stream constants SPEC-004 section 1 names.

use battle_rules::{
    soldier_stream, F_DISABLED, F_GUN, F_KNEEL, F_PRONE, F_WOUNDED, STREAM_BATTLE_SETUP,
    STREAM_SOLDIER_BASE, TICK_HZ,
};

#[test]
fn flag_bits_and_streams_have_the_specified_values() {
    assert_eq!(TICK_HZ, 20);
    assert_eq!(
        [F_KNEEL, F_PRONE, F_DISABLED, F_WOUNDED, F_GUN],
        [1, 2, 4, 8, 16]
    );
    assert_eq!(STREAM_BATTLE_SETUP.get(), 0x0B01);
    assert_eq!(STREAM_SOLDIER_BASE, 0x4000_0000);
    assert_eq!(soldier_stream(0).get(), 0x4000_0000);
    assert_eq!(soldier_stream(19_999).get(), 0x4000_0000 + 19_999);
}
