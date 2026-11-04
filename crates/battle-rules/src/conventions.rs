//! Shared conventions of SPEC-004 section 1.

use core_math::{Fx, StreamId};

/// Simulation ticks per second (matches SPEC-003).
pub const TICK_HZ: u32 = 20;

/// Stream for scenario jitter only. Per-soldier streams `0x4000_0000 + slot_index` belong to SPEC-005.
pub const STREAM_BATTLE_SETUP: StreamId = StreamId(0x0B01);

/// `AgentStore.flags` bit: kneeling.
pub const F_KNEEL: u16 = 1 << 0;
/// `AgentStore.flags` bit: prone.
pub const F_PRONE: u16 = 1 << 1;
/// `AgentStore.flags` bit: down for a number of ticks.
pub const F_DISABLED: u16 = 1 << 2;
/// `AgentStore.flags` bit: out for the battle.
pub const F_WOUNDED: u16 = 1 << 3;
/// `AgentStore.flags` bit: a gun crew member.
pub const F_GUN: u16 = 1 << 4;

/// The ratio `num / den` as `Fx`: raw = `(num << 32) / den` truncating toward zero, saturated to
/// `i64`. A zero denominator follows the `core_math` `Div` rules (`MAX`, `MIN` or zero by the
/// sign of `num`). Rules data become `Fx` only through this function.
///
/// ```
/// use battle_rules::fx_ratio;
/// assert_eq!(fx_ratio(1, 2), core_math::Fx::HALF);
/// ```
pub fn fx_ratio(num: i64, den: i64) -> Fx {
    if den == 0 {
        return match num.signum() {
            1 => Fx::MAX,
            -1 => Fx::MIN,
            _ => Fx::ZERO,
        };
    }
    let raw = (i128::from(num) << 32) / i128::from(den);
    Fx::from_raw(i64::try_from(raw).unwrap_or(if raw < 0 { i64::MIN } else { i64::MAX }))
}
