//! The camera packet handed to the mixer, and its hash (SPEC-012 sections 8 and 9).

use core_math::Angle;
use sim_state::StateHasher;

/// One individually mixed source (a cluster that won a place in the budget).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceOut {
    /// Cluster id.
    pub id: u64,
    /// Source row id.
    pub source: u8,
    /// Number of sounds in the cluster.
    pub count: u32,
    /// Arrival of the first sound after the start of the window, ms of simulation time.
    pub arrival_ms: u64,
    /// Time over which the cluster's sounds arrive, ms.
    pub span_ms: u64,
    /// Distance from the listener, mm.
    pub distance_mm: i64,
    /// Direction from the listener to the cluster.
    pub bearing: Angle,
    /// Level at the listener per band, cdB.
    pub level_cdb: [i32; 3],
}

/// The unselected clusters of one of eight compass sectors, mixed as one bed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BedOut {
    /// Sector 0 to 7, counting a full turn from the +x direction in eighths.
    pub sector: u8,
    /// Power sum of the clusters' levels at the listener per band, cdB.
    pub level_cdb: [i32; 3],
    /// Number of sounds in the sector.
    pub count: u32,
    /// Earliest arrival, ms.
    pub arrival_ms: u64,
    /// Time from the earliest arrival to the latest end, ms.
    pub span_ms: u64,
}

/// What the mixer receives for one listener and one window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MixPacket {
    /// Listener id.
    pub listener: u32,
    /// First tick of the window.
    pub window_start_tick: u64,
    /// Length of the window in ticks.
    pub window_ticks: u16,
    /// Selected sources in ascending `id`.
    pub sources: Vec<SourceOut>,
    /// Non-empty bed sectors in ascending order.
    pub bed: Vec<BedOut>,
}

/// FNV-1a 64 over the packet's little-endian fixed-width fields in output order.
pub fn packet_hash(p: &MixPacket) -> u64 {
    let mut h = StateHasher::new();
    h.write_u32(p.listener);
    h.write_u64(p.window_start_tick);
    h.write_u16(p.window_ticks);
    h.write_u32(p.sources.len() as u32);
    for s in &p.sources {
        h.write_u64(s.id);
        h.write_u8(s.source);
        h.write_u32(s.count);
        h.write_u64(s.arrival_ms);
        h.write_u64(s.span_ms);
        h.write_i64(s.distance_mm);
        h.write_u32(s.bearing.raw());
        for l in s.level_cdb {
            h.write_u32(l as u32);
        }
    }
    h.write_u32(p.bed.len() as u32);
    for b in &p.bed {
        h.write_u8(b.sector);
        for l in b.level_cdb {
            h.write_u32(l as u32);
        }
        h.write_u32(b.count);
        h.write_u64(b.arrival_ms);
        h.write_u64(b.span_ms);
    }
    h.finish()
}
