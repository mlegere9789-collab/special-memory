//! Sound events and their aggregation into clusters (SPEC-012 sections 4 and 7).
//!
//! One cluster holds all events of one source row, one unit and one 100 m cell in one window.
//! Only commutative operations touch a cluster, so the order of arrival never matters.

use crate::intmath::cdb_of_power;
use crate::tables::AcousticTables;
use crate::weather::to_mm;
use core_math::Vec2Fx;
use std::collections::BTreeMap;

/// Highest level a cluster can reach, cdB.
const LEVEL_CAP_CDB: i32 = 18300;
/// Level of a source with no row in the tables, cdB (the value of `cdb_of_power(0)`).
const SILENT_CDB: i32 = -20000;

/// One sound made in the simulation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SoundEvent {
    /// Sequence number of the fire record.
    pub seq: u64,
    /// Source row id.
    pub source: u8,
    /// Unit that made the sound (below 2^24).
    pub unit: u32,
    /// Team of the unit.
    pub team: u8,
    /// Where the sound was made, metres.
    pub pos: Vec2Fx,
    /// Height above the ground, mm.
    pub height_mm: u16,
    /// How many sounds this event stands for (raised to at least 1).
    pub count: u16,
}

/// All events of one key in one window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cluster {
    /// `source << 56 | unit << 32 | cell_x << 16 | cell_y` (cells as 16-bit two's complement).
    pub id: u64,
    /// Source row id.
    pub source: u8,
    /// Unit id.
    pub unit: u32,
    /// Team of the lowest-sequence event.
    pub team: u8,
    /// Sum of the event counts, saturating.
    pub count: u32,
    /// Count-weighted centroid, mm.
    pub centroid_mm: [i64; 2],
    /// Smallest x and y of any event, mm.
    pub min_mm: [i64; 2],
    /// Largest x and y of any event, mm.
    pub max_mm: [i64; 2],
    /// Count-weighted mean height, mm.
    pub height_mm: u16,
    /// Lowest event sequence number.
    pub first_seq: u64,
    /// First tick of the window.
    pub window_start_tick: u64,
    /// Length of the window in ticks.
    pub window_ticks: u32,
}

impl Cluster {
    /// The larger of the x and y ranges of the events, mm.
    pub fn extent_mm(&self) -> i64 {
        (self.max_mm[0] - self.min_mm[0]).max(self.max_mm[1] - self.min_mm[1])
    }

    /// Level per band at 1 m, cdB: the source level plus `10 log10(count)` minus
    /// `10 log10(window_ticks / 20)`, capped at 18300, so windows of different length compare.
    /// A source without a row is silent.
    pub fn level_cdb(&self, t: &AcousticTables) -> [i32; 3] {
        let Some(row) = t.sources().get(usize::from(self.source)) else {
            return [SILENT_CDB; 3];
        };
        let n = cdb_of_power(u128::from(self.count) << 16);
        let w = cdb_of_power(u128::from(self.window_ticks) * 65536 / 20);
        row.level_cdb.map(|sel| (sel + n - w).min(LEVEL_CAP_CDB))
    }
}

/// Running totals of one cluster. Sums are exact (`i128`), so no order can change them.
struct Acc {
    team_seq: (u64, u8),
    count: u32,
    sum: [i128; 3],
    min: [i64; 2],
    max: [i64; 2],
}

/// Collects events of one window and turns them into clusters.
pub struct Aggregator {
    window_ticks: u32,
    cell_mm: i64,
    acc: BTreeMap<u64, Acc>,
}

impl Aggregator {
    /// An aggregator for windows of `window_ticks` ticks (at least 1) with the 100 m cell of the
    /// shipped tables.
    pub fn new(window_ticks: u32) -> Aggregator {
        Aggregator::with_cell(window_ticks, 100_000)
    }

    /// As [`Aggregator::new`] with an explicit cell size, mm (at least 1).
    pub fn with_cell(window_ticks: u32, cell_mm: i64) -> Aggregator {
        Aggregator {
            window_ticks: window_ticks.max(1),
            cell_mm: cell_mm.max(1),
            acc: BTreeMap::new(),
        }
    }

    /// Adds one event.
    pub fn push(&mut self, e: &SoundEvent) {
        let (x, y) = (to_mm(e.pos.x), to_mm(e.pos.y));
        let cell = |v: i64| v.div_euclid(self.cell_mm).clamp(-32768, 32767) as i16 as u16;
        let id = u64::from(e.source) << 56
            | u64::from(e.unit & 0x00FF_FFFF) << 32
            | u64::from(cell(x)) << 16
            | u64::from(cell(y));
        let n = u32::from(e.count.max(1));
        let a = self.acc.entry(id).or_insert(Acc {
            team_seq: (u64::MAX, u8::MAX),
            count: 0,
            sum: [0; 3],
            min: [i64::MAX; 2],
            max: [i64::MIN; 2],
        });
        a.team_seq = a.team_seq.min((e.seq, e.team));
        a.count = a.count.saturating_add(n);
        for (s, v) in a.sum.iter_mut().zip([x, y, i64::from(e.height_mm)]) {
            *s += i128::from(n) * i128::from(v);
        }
        a.min = [a.min[0].min(x), a.min[1].min(y)];
        a.max = [a.max[0].max(x), a.max[1].max(y)];
    }

    /// Ends the window whose last tick is `window_end_tick`: the clusters in ascending `id`, and
    /// the aggregator is empty again.
    pub fn flush(&mut self, window_end_tick: u64) -> Vec<Cluster> {
        let start = (window_end_tick + 1).saturating_sub(u64::from(self.window_ticks));
        std::mem::take(&mut self.acc)
            .into_iter()
            .map(|(id, a)| {
                // Weighted sums of in-range values divide back into range; the weight is the
                // saturated total, which only differs from the true one in absurd inputs.
                let n = i128::from(a.count.max(1));
                let mean = |k: usize| (a.sum[k].div_euclid(n)) as i64;
                Cluster {
                    id,
                    source: (id >> 56) as u8,
                    unit: ((id >> 32) & 0x00FF_FFFF) as u32,
                    team: a.team_seq.1,
                    count: a.count,
                    centroid_mm: [
                        mean(0).clamp(a.min[0], a.max[0]),
                        mean(1).clamp(a.min[1], a.max[1]),
                    ],
                    min_mm: a.min,
                    max_mm: a.max,
                    height_mm: mean(2).clamp(0, i64::from(u16::MAX)) as u16,
                    first_seq: a.team_seq.0,
                    window_start_tick: start,
                    window_ticks: self.window_ticks,
                }
            })
            .collect()
    }
}
