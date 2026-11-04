//! Listeners and the camera packet pipeline (SPEC-012 section 8).

use crate::aggregate::Cluster;
use crate::intmath::{cdb_of_power, power_of_cdb};
use crate::packet::{BedOut, MixPacket, SourceOut};
use crate::propagation::{along_wind_mm_s, delay_ms, path_result, sound_speed_mm_s};
use crate::relief::Relief;
use crate::tables::{AcousticTables, CLASS_COUNT};
use crate::weather::{to_mm, Weather};
use core_math::Vec2Fx;

/// What a listener stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListenerKind {
    /// The player's camera; receives mix packets.
    Camera,
    /// A headquarters; receives hearing reports.
    Headquarters,
}

/// A place where sound is heard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Listener {
    /// Listener id (for a headquarters, its id in ascending order).
    pub id: u32,
    /// Camera or headquarters.
    pub kind: ListenerKind,
    /// Position, metres.
    pub pos: Vec2Fx,
    /// Height above the ground, mm.
    pub height_mm: u16,
    /// Ambient level per band, cdB.
    pub ambient_cdb: [i32; 3],
}

/// What a camera listener remembers from the last window: the selected cluster ids, ascending.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ListenerState {
    /// Cluster ids selected in the previous packet.
    pub prev: Vec<u64>,
}

/// A cluster as one listener hears it.
struct Heard {
    idx: usize,
    class: usize,
    score: i32,
    lev: [i32; 3],
    out: SourceOut,
    end_ms: u64,
}

/// Running totals of one bed sector.
#[derive(Clone, Copy)]
struct BedAcc {
    power: [u128; 3],
    count: u32,
    first_ms: u64,
    end_ms: u64,
}

/// Compass sector (0 to 7, eighths of a turn centred on the +x direction) of a bearing.
fn sector(raw: u32) -> u8 {
    (((u64::from(raw) + (1 << 28)) >> 29) & 7) as u8
}

/// Orders by score, highest first, ties to the lower cluster id.
fn rank(h: &mut [&Heard], cl: &[Cluster]) {
    h.sort_by(|a, b| b.score.cmp(&a.score).then(cl[a.idx].id.cmp(&cl[b.idx].id)));
}

/// Builds the camera packet for one listener and one window. `clusters` must come from one
/// `flush`. Clusters beyond the range limit or quieter than `drop_below_cdb` are lost; per class
/// and overall budgets pick the individual sources, with a bonus for last window's selection; the
/// rest become the bed. `state` is updated to the new selection. With a `relief`, barrier and
/// foliage losses join the path loss.
pub fn render_packet(
    listener: &Listener,
    state: &mut ListenerState,
    clusters: &[Cluster],
    t: &AcousticTables,
    weather: &Weather,
    relief: Option<&Relief>,
) -> MixPacket {
    let g = t.globals();
    let lis_mm = [to_mm(listener.pos.x), to_mm(listener.pos.y)];
    let c = sound_speed_mm_s(t, i32::from(weather.temp_dc));
    let mut heard: Vec<Heard> = Vec::new();
    for (idx, cl) in clusters.iter().enumerate() {
        let Some(p) = path_result(
            t,
            weather,
            cl.centroid_mm,
            cl.height_mm,
            lis_mm,
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
        if loudness < g.drop_below_cdb {
            continue;
        }
        let row = t.sources().get(usize::from(cl.source));
        let (dx, dy) = (lis_mm[0] - cl.centroid_mm[0], lis_mm[1] - cl.centroid_mm[1]);
        let w = along_wind_mm_s(weather.wind_mm_s, dx, dy, p.distance_mm);
        let extent = cl.extent_mm();
        let near = (p.distance_mm - extent / 2).max(1000);
        let c_eff = (c + w).max(c / 2).max(1);
        let start_ms = cl.window_start_tick * 50;
        let arrival_ms = start_ms + delay_ms(near, c, w);
        let span_ms =
            u64::from(cl.window_ticks) * 50 + ((extent * 1000 + c_eff - 1) / c_eff) as u64;
        let prev = state.prev.binary_search(&cl.id).is_ok();
        heard.push(Heard {
            idx,
            class: row.map_or(0, |r| usize::from(r.class)).min(CLASS_COUNT - 1),
            score: loudness + row.map_or(0, |r| r.priority_cdb) + if prev { g.hyst_cdb } else { 0 },
            lev,
            end_ms: arrival_ms + span_ms,
            out: SourceOut {
                id: cl.id,
                source: cl.source,
                count: cl.count,
                arrival_ms,
                span_ms,
                distance_mm: p.distance_mm,
                bearing: p.bearing,
                level_cdb: lev,
            },
        });
    }
    // Per class caps first, then the overall cap, both by score with ties to the lower id.
    let mut by_class: Vec<&Heard> = Vec::new();
    for class in 0..CLASS_COUNT {
        let mut members: Vec<&Heard> = heard.iter().filter(|h| h.class == class).collect();
        rank(&mut members, clusters);
        members.truncate(t.budget().class_caps[class] as usize);
        by_class.extend(members);
    }
    rank(&mut by_class, clusters);
    by_class.truncate(t.budget().max_total as usize);
    let mut chosen: Vec<usize> = by_class.iter().map(|h| h.idx).collect();
    chosen.sort_by_key(|&i| clusters[i].id);
    state.prev = chosen.iter().map(|&i| clusters[i].id).collect();

    let mut sources = Vec::new();
    let mut bed: [Option<BedAcc>; 8] = [None; 8];
    for h in &heard {
        if chosen
            .binary_search_by_key(&clusters[h.idx].id, |&i| clusters[i].id)
            .is_ok()
        {
            continue;
        }
        let e = bed[usize::from(sector(h.out.bearing.raw()))].get_or_insert(BedAcc {
            power: [0; 3],
            count: 0,
            first_ms: u64::MAX,
            end_ms: 0,
        });
        for b in 0..3 {
            e.power[b] += power_of_cdb(h.lev[b]);
        }
        e.count = e.count.saturating_add(h.out.count);
        e.first_ms = e.first_ms.min(h.out.arrival_ms);
        e.end_ms = e.end_ms.max(h.end_ms);
    }
    sources.extend(
        heard
            .iter()
            .filter(|h| state.prev.binary_search(&clusters[h.idx].id).is_ok())
            .map(|h| h.out),
    );
    sources.sort_by_key(|s| s.id);
    let bed = bed
        .iter()
        .enumerate()
        .filter_map(|(s, e)| {
            e.map(|a| BedOut {
                sector: s as u8,
                level_cdb: a.power.map(cdb_of_power),
                count: a.count,
                arrival_ms: a.first_ms,
                span_ms: a.end_ms - a.first_ms,
            })
        })
        .collect();
    MixPacket {
        listener: listener.id,
        window_start_tick: clusters.first().map_or(0, |c| c.window_start_tick),
        window_ticks: clusters
            .first()
            .map_or(g.camera_window_ticks, |c| c.window_ticks) as u16,
        sources,
        bed,
    }
}
