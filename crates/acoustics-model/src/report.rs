//! Hearing reports for headquarters listeners (SPEC-012 section 8) and their hash.

use crate::aggregate::Cluster;
use crate::intmath::{cdb_of_power, power_of_cdb};
use crate::listener::Listener;
use crate::propagation::{along_wind_mm_s, delay_ms, path_result, sound_speed_mm_s};
use crate::relief::Relief;
use crate::tables::AcousticTables;
use crate::weather::{to_mm, Weather};
use sim_state::StateHasher;

/// How well a headquarters hears the battle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum HearClass {
    /// Nothing rises above the ambient level.
    Silent = 0,
    /// Barely above the ambient level.
    Faint = 1,
    /// Plainly heard.
    Clear = 2,
    /// Loud.
    Loud = 3,
}

/// What one headquarters hears in one window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HearingReport {
    /// Listener id.
    pub listener: u32,
    /// Tick at which the report is made.
    pub tick: u64,
    /// Summed level over the ambient level in the best band, cdB (negative: drowned out).
    pub margin_cdb: i32,
    /// The margin as a class.
    pub class: HearClass,
    /// Compass sector (0 to 7, as in the camera bed) holding the largest power.
    pub loudest_sector: u8,
    /// Clusters whose own level reaches the ambient level in some band.
    pub audible_clusters: u32,
    /// Earliest arrival over clusters above the drop threshold, ms of simulation time (0 if none).
    pub earliest_arrival_ms: u64,
}

/// Compass sector of a bearing, eighths of a turn centred on +x (the camera bed's rule).
fn sector(raw: u32) -> usize {
    (((u64::from(raw) + (1 << 28)) >> 29) & 7) as usize
}

/// The report of `listener` over one flushed cluster set. All clusters in range add up in power,
/// with no budget and no drop threshold, so many faint clusters can add up to something heard.
/// `loudest_sector` is the sector whose power, summed over the three bands, is largest (the lowest
/// sector on a tie, sector 0 when nothing is heard). `tick` is stored in the report as given.
pub fn hearing_report(
    listener: &Listener,
    clusters: &[Cluster],
    t: &AcousticTables,
    weather: &Weather,
    relief: Option<&Relief>,
    tick: u64,
) -> HearingReport {
    let g = t.globals();
    let lis = [to_mm(listener.pos.x), to_mm(listener.pos.y)];
    let c = sound_speed_mm_s(t, i32::from(weather.temp_dc));
    let mut total = [0u128; 3];
    let mut by_sector = [0u128; 8];
    let (mut audible, mut earliest) = (0u32, u64::MAX);
    for cl in clusters {
        let Some(p) = path_result(
            t,
            weather,
            cl.centroid_mm,
            cl.height_mm,
            lis,
            listener.height_mm,
            relief,
        ) else {
            continue;
        };
        let level = cl.level_cdb(t);
        let lev = [0, 1, 2].map(|b| level[b] - p.loss_cdb[b]);
        let loudness = (0..3)
            .map(|b| lev[b] - listener.ambient_cdb[b])
            .max()
            .unwrap_or(i32::MIN);
        let power = lev.map(power_of_cdb);
        for b in 0..3 {
            total[b] += power[b];
        }
        by_sector[sector(p.bearing.raw())] += power.iter().sum::<u128>();
        if loudness >= 0 {
            audible += 1;
        }
        if loudness >= g.drop_below_cdb {
            let (dx, dy) = (lis[0] - cl.centroid_mm[0], lis[1] - cl.centroid_mm[1]);
            let w = along_wind_mm_s(weather.wind_mm_s, dx, dy, p.distance_mm);
            let near = (p.distance_mm - cl.extent_mm() / 2).max(1000);
            earliest = earliest.min(cl.window_start_tick * 50 + delay_ms(near, c, w));
        }
    }
    let margin_cdb = (0..3)
        .map(|b| cdb_of_power(total[b]) - listener.ambient_cdb[b])
        .max()
        .unwrap_or(i32::MIN);
    let class = if margin_cdb < g.class_faint_cdb {
        HearClass::Silent
    } else if margin_cdb < g.class_clear_cdb {
        HearClass::Faint
    } else if margin_cdb < g.class_loud_cdb {
        HearClass::Clear
    } else {
        HearClass::Loud
    };
    let loudest = (0..8).fold(0, |best, s| {
        if by_sector[s] > by_sector[best] {
            s
        } else {
            best
        }
    });
    HearingReport {
        listener: listener.id,
        tick,
        margin_cdb,
        class,
        loudest_sector: loudest as u8,
        audible_clusters: audible,
        earliest_arrival_ms: if earliest == u64::MAX { 0 } else { earliest },
    }
}

/// FNV-1a 64 over the report's little-endian fixed-width fields in declaration order.
pub fn report_hash(r: &HearingReport) -> u64 {
    let mut h = StateHasher::new();
    h.write_u32(r.listener);
    h.write_u64(r.tick);
    h.write_u32(r.margin_cdb as u32);
    h.write_u8(r.class as u8);
    h.write_u8(r.loudest_sector);
    h.write_u32(r.audible_clusters);
    h.write_u64(r.earliest_arrival_ms);
    h.finish()
}
