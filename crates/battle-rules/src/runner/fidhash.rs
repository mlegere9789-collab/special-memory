//! The FID1 hash block of [`Battle::hash`](crate::Battle::hash) (SPEC-008 section 9).

use crate::registry::MAGIC_FIDELITY;
use crate::{Battle, RegimentStatus};
use sim_state::StateHasher;

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// Writes the block: `u32 0x4649_4431`, `soldier_serial u32`, then per regiment ascending
/// `status u8 (0 not arrived, 1 present, 2 departed), class u8, since u64`, the aggregate
/// (`fit u32, wounded u32, killed u32, rounds u64, fatigue_sum u64, fouling_sum u64,
/// hit_carry_ppm u32, shot_carry_ppm u32`, then `u32` count of `(until u64, n u32)` pairs) and
/// the round account (`initial, resupplied, spent, lost`, each `u64`); then the sector states
/// (`u32` count, one `u8` each); then the cursors (arrivals, departures, triggers `u32`, weather
/// row `u32` as index plus one or 0) and `arrived`, `departed` per team (`u64`).
///
/// Later tickets append their fields after these: the smoke tiles and the F1 pending lists and
/// delay ring.
pub(super) fn write_fid_block(h: &mut StateHasher, b: &Battle) {
    h.write_u32(MAGIC_FIDELITY);
    h.write_u32(b.state.soldier_serial);
    h.write_u32(count(b.state.regiments.len()));
    for r in &b.state.regiments {
        h.write_u8(match r.status {
            RegimentStatus::NotArrived => 0,
            RegimentStatus::Present => 1,
            RegimentStatus::Departed => 2,
        });
        h.write_u8(r.class.0);
        h.write_u64(r.since);
        let a = &r.aggregate;
        h.write_u32(a.fit);
        h.write_u32(a.wounded);
        h.write_u32(a.killed);
        h.write_u64(a.rounds);
        h.write_u64(a.fatigue_sum);
        h.write_u64(a.fouling_sum);
        h.write_u32(a.hit_carry_ppm);
        h.write_u32(a.shot_carry_ppm);
        h.write_u32(count(a.disabled.len()));
        for &(until, n) in &a.disabled {
            h.write_u64(until);
            h.write_u32(n);
        }
        for v in [r.acct.initial, r.acct.resupplied, r.acct.spent, r.acct.lost] {
            h.write_u64(v);
        }
    }
    let states = b.fid.sectors.states();
    h.write_u32(count(states.len()));
    for s in states {
        h.write_u8(s.tag());
    }
    h.write_u32(count(b.fid.arrivals_done));
    h.write_u32(count(b.fid.departures_done));
    h.write_u32(count(b.fid.triggers_done));
    h.write_u32(b.fid.weather_row.map_or(0, |i| count(i + 1)));
    for v in b.fid.arrived.into_iter().chain(b.fid.departed) {
        h.write_u64(v);
    }
}
