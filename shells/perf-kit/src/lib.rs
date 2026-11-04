//! Integer reference functions for the M1 renderer measurements (SPEC-016 sections 3.5 and 7.2).
//!
//! This crate is a prototype baseline: it fixes render-tier assignment, fire arrival delay,
//! cluster keys and the voice budget as plain integer functions so the prototypes have test
//! vectors. It is not the production audio model; the production host consumes the SPEC-012 mix
//! packets (ADR 0010, decision D-10). Everything here is deterministic integer arithmetic.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

/// Render tier of one soldier: A near and skinned, B mid, C far impostor, D horizon density block.
/// Ordering is nearest first (`A < B < C < D`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Tier {
    /// Near, skinned.
    A,
    /// Mid, baked bone texture.
    B,
    /// Far, impostor.
    C,
    /// Horizon density block.
    D,
}

/// Distance in millimetres below which a soldier enters tier A, B or C.
pub const ENTER_MM: [u32; 3] = [30_000, 150_000, 600_000];
/// Distance in millimetres at or above which a soldier leaves tier A, B or C.
pub const LEAVE_MM: [u32; 3] = [33_000, 165_000, 660_000];
/// Audio samples per simulation tick (48 kHz, 20 ticks per second).
pub const SAMPLES_PER_TICK: u64 = 2_400;
/// Cluster width and height in centimetres (50 m).
pub const CELL_CM: i32 = 5_000;
/// Clusters that receive a voice.
pub const CLUSTER_VOICES: usize = 48;
/// Voices in total.
pub const MAX_VOICES: usize = 64;
/// Voices reserved for interface, speech and music.
pub const RESERVED_VOICES: usize = 16;

const TIERS: [Tier; 4] = [Tier::A, Tier::B, Tier::C, Tier::D];

/// Tier for distance `d` and previous tier `prev`: the entry thresholds apply, except that a
/// soldier already in a nearer tier stays there until the leave threshold of that tier.
fn tier_for(d: u32, prev: Tier) -> Tier {
    let entry = ENTER_MM
        .iter()
        .position(|&e| d < e)
        .map_or(Tier::D, |i| TIERS[i]);
    if prev < entry {
        // `prev` is A, B or C here, so the index is in range.
        if d < LEAVE_MM[prev as usize] {
            return prev;
        }
    }
    entry
}

/// Assigns a tier to every soldier.
///
/// `d_mm[i]` is the distance of soldier `i`, `slots[i]` its stable slot (tie-break), `prev[i]` its
/// tier on the previous frame. A missing `prev` entry counts as `Tier::D` and a missing slot as
/// the index, so inputs of equal length are the intended use. `caps` is `(tier A cap, tier B
/// cap)`. When a tier is over its cap the farthest soldiers by `(d_mm, slot)` fall one tier; the
/// fall into B is counted before B's cap is applied. `out` is cleared and holds one tier per
/// soldier.
pub fn assign_tiers(
    d_mm: &[u32],
    slots: &[u32],
    prev: &[Tier],
    caps: (u32, u32),
    out: &mut Vec<Tier>,
) {
    out.clear();
    out.extend(
        d_mm.iter()
            .enumerate()
            .map(|(i, &d)| tier_for(d, prev.get(i).copied().unwrap_or(Tier::D))),
    );
    let slot = |i: usize| slots.get(i).copied().unwrap_or(i as u32);
    for (tier, cap, below) in [(Tier::A, caps.0, Tier::B), (Tier::B, caps.1, Tier::C)] {
        let mut members: Vec<usize> = (0..out.len()).filter(|&i| out[i] == tier).collect();
        if members.len() <= cap as usize {
            continue;
        }
        members.sort_unstable_by_key(|&i| (d_mm[i], slot(i), i));
        for &i in &members[cap as usize..] {
            out[i] = below;
        }
    }
}

/// Integer square root (floor) of a 128-bit value.
fn isqrt_u128(n: u128) -> u128 {
    let mut rem = n;
    let mut root = 0u128;
    let mut bit = 1u128 << 126;
    while bit > n {
        bit >>= 2;
    }
    while bit != 0 {
        if rem >= root + bit {
            rem -= root + bit;
            root = (root >> 1) + bit;
        } else {
            root >>= 1;
        }
        bit >>= 2;
    }
    root
}

/// Distance in millimetres: `floor(sqrt(dx^2 + dy^2 + dz^2))`, exact for every `i64` input.
pub fn range_mm(dx: i64, dy: i64, dz: i64) -> u64 {
    let sq = |v: i64| u128::from(v.unsigned_abs()) * u128::from(v.unsigned_abs());
    // The sum is below 3 * 2^126, and its root below 2^64, so both steps are lossless.
    isqrt_u128(sq(dx) + sq(dy) + sq(dz)) as u64
}

/// Sample at which a shot fired on `tick` reaches a listener `range_mm` away:
/// `tick * 2400 + floor(range_mm * 48 / 343)` (343 m/s at 48 kHz). Saturates at `u64::MAX`.
pub fn arrival_sample(tick: u64, range_mm: u64) -> u64 {
    let delay = u128::from(range_mm) * 48 / 343;
    let total = u128::from(tick) * u128::from(SAMPLES_PER_TICK) + delay;
    u64::try_from(total).unwrap_or(u64::MAX)
}

/// Cluster key `(regiment, floor(x_cm / 5000), floor(y_cm / 5000))`, using Euclidean division so
/// negative coordinates floor correctly.
pub fn cluster_key(regiment: u16, x_cm: i32, y_cm: i32) -> (u16, i32, i32) {
    (regiment, x_cm.div_euclid(CELL_CM), y_cm.div_euclid(CELL_CM))
}

/// One aggregated group of shots heard by the listener.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Cluster {
    /// Cluster key from [`cluster_key`].
    pub key: (u16, i32, i32),
    /// Shots counted for the cluster.
    pub shots: u32,
    /// Distance from the listener to the cluster centroid in millimetres.
    pub range_mm: u64,
}

/// Loudness score `shots * 10^12 / (range_mm + 1000)^2`, rounded down. Zero when the squared
/// denominator would not fit 128 bits (a range beyond any battlefield).
pub fn gain_q(shots: u32, range_mm: u64) -> u128 {
    let d = u128::from(range_mm) + 1_000;
    d.checked_mul(d)
        .map_or(0, |den| u128::from(shots) * 1_000_000_000_000 / den)
}

/// Indices of the loudest `max` clusters, ordered by `(gain_q` descending, index ascending)`.
/// Ties go to the lower index; fewer than `max` are returned when there are fewer clusters.
pub fn select_voices(clusters: &[Cluster], max: usize) -> Vec<usize> {
    let mut ranked: Vec<(u128, usize)> = clusters
        .iter()
        .enumerate()
        .map(|(i, c)| (gain_q(c.shots, c.range_mm), i))
        .collect();
    ranked.sort_unstable_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    ranked.into_iter().take(max).map(|(_, i)| i).collect()
}
