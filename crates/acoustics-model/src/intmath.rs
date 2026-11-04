//! Integer math of SPEC-012 section 5: square root, base-2 logarithm and the centibel conversions.
//!
//! Powers are Q16 (`P = 10^(L/10) * 2^16`), levels are centibels (cdB, 0.01 dB). Everything is
//! integer and platform independent. The square root is this crate's own copy of the Newton
//! iteration, so no private item of `core-math` is exposed for it.

use std::sync::OnceLock;

/// Floor of the square root of `n` (Newton iteration).
pub fn isqrt_u128(n: u128) -> u128 {
    if n < 2 {
        return n;
    }
    let bits = 128 - n.leading_zeros();
    // 2^ceil(bits/2) is at least sqrt(n), so the iteration decreases monotonically to the floor.
    let mut x = 1u128 << bits.div_ceil(2);
    loop {
        let y = (x + n / x) >> 1;
        if y >= x {
            return x;
        }
        x = y;
    }
}

/// Base-2 logarithm of `x >= 1` in Q16 (16 fractional bits); the error is below 3 units.
/// `x = 0` is treated as 1 (result 0).
pub fn log2_q16(x: u128) -> u32 {
    let x = x.max(1);
    let n = 127 - x.leading_zeros();
    // Mantissa in Q1.32, in [2^32, 2^33).
    let mut y = (x << (127 - n)) >> 95;
    let mut bits = 0u32;
    for _ in 0..16 {
        y = (y * y) >> 32;
        bits <<= 1;
        if y >= 1 << 33 {
            y >>= 1;
            bits |= 1;
        }
    }
    (n << 16) | bits
}

/// `10 log10` of a Q16 power (or of a Q16 ratio) in cdB, rounded down. `p = 0` gives `-20000`.
pub fn cdb_of_power(p: u128) -> i32 {
    if p == 0 {
        return -20_000;
    }
    let octaves_q16 = i64::from(log2_q16(p)) - (1 << 20);
    // 10 log10(2) = 3.01029996 dB per octave, so 301.029996 cdB.
    let cdb = (octaves_q16 * 301_029_996).div_euclid(65_536 * 1_000_000);
    i32::try_from(cdb).unwrap_or(i32::MAX)
}

/// `floor(2^62 * 2^(1/256))`, the integer 256th root step of the table.
const STEP: u128 = 4_624_189_567_668_517_720;

/// `T[k] = floor(2^62 * 2^(k/256))` for `k` in `0..=256`, built once by the recurrence.
fn table() -> &'static [u64; 257] {
    static TABLE: OnceLock<[u64; 257]> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut t = [0u64; 257];
        t[0] = 1 << 62;
        for k in 0..255 {
            t[k + 1] = ((u128::from(t[k]) * STEP) >> 62) as u64;
        }
        t[256] = 1 << 63;
        t
    })
}

/// Q16 power of a level in cdB; `l` is clamped to `-6000..=19000`.
pub fn power_of_cdb(l: i32) -> u128 {
    let l = i64::from(l.clamp(-6000, 19_000));
    // l/1000 * log2(10) in Q16.16 (`log2(10)` in Q32), rounded down.
    let x = (l * 14_267_572_527).div_euclid(1000 * 65_536);
    let n = x >> 16;
    let f = (x & 0xFFFF) as usize;
    let (i, r) = (f >> 8, (f & 255) as u128);
    let t = table();
    let lo = u128::from(t[i]);
    let v = lo + (u128::from(t[i + 1]) - lo) * r / 256;
    // v * 2^(n + 16 - 62)
    let shift = n + 16 - 62;
    if shift >= 0 {
        v << shift
    } else {
        v >> (-shift)
    }
}
