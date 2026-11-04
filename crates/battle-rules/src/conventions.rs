//! Shared conventions of SPEC-004 section 1.

use core_math::Fx;

/// Simulation ticks per second (matches SPEC-003).
pub const TICK_HZ: u32 = 20;

// The stream and flag constants are owned by the registry module (T-026); these re-exports keep
// the paths `battle_rules::conventions::*` and `battle_rules::{F_*, STREAM_BATTLE_SETUP}` valid.
pub use crate::registry::{F_DISABLED, F_GUN, F_KNEEL, F_PRONE, F_WOUNDED, STREAM_BATTLE_SETUP};

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
