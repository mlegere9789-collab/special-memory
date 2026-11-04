//! Slot geometry and soldier-to-slot assignment (SPEC-004 section 8), plus the roster helper.

use crate::{fx_ratio, DrillTable, Formation, F_DISABLED, F_WOUNDED};
use core_math::{Angle, Fx, Vec2Fx};
use sim_state::{AgentId, AgentStore};

/// A position in a regiment's local frame: `u` to the right of the heading, `v` ahead of the
/// anchor (negative behind it). Metres as `Fx`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SlotLocal {
    /// Lateral offset, positive to the right.
    pub u: Fx,
    /// Longitudinal offset, positive forward.
    pub v: Fx,
}

/// Appends a block of `n` men, `cols` to a full rank, rank-major with rank 0 at `top_mm` behind
/// the anchor and each further rank `gap_mm` further back. Ranks are centred on `u = 0`.
fn push_block(
    out: &mut Vec<SlotLocal>,
    n: usize,
    cols: usize,
    spacing_mm: i64,
    gap_mm: i64,
    top_mm: i64,
) {
    let cols = cols.max(1);
    for k in 0..n {
        let (rank, file) = (k / cols, k % cols);
        let in_rank = cols.min(n - rank * cols);
        // Twice the offset from the rank centre, in files: exact for odd and even ranks.
        let twice = 2 * file as i64 - (in_rank as i64 - 1);
        out.push(SlotLocal {
            u: fx_ratio(twice * spacing_mm, 2000),
            v: fx_ratio(-(top_mm + rank as i64 * gap_mm), 1000),
        });
    }
}

/// The canonical slot list of `n` men in `formation`: rank-major, rank 0 at the front, each rank
/// ordered by ascending `u`, centred on the anchor. Along the list `v` never increases, and it
/// strictly decreases between ranks.
///
/// - Line: `cols = ceil(n / ranks)`, spacing `file_width_mm`, ranks `rank_gap_mm` apart.
/// - Column: `cols` from the table.
/// - Skirmish: the first `ceil(n * deployed_pm / 1000)` slots form one rank at `interval_mm`
///   spacing on `v = 0`; the remainder is a reserve block of `cols` files starting
///   `reserve_offset_mm` behind it.
pub fn slot_layout(formation: Formation, n: usize, table: &DrillTable) -> Vec<SlotLocal> {
    let row = table.formation(formation);
    let mut out = Vec::with_capacity(n);
    let (spacing, gap) = (row.file_width_mm, row.rank_gap_mm);
    match formation {
        Formation::Line => {
            let ranks = row.ranks.unwrap_or(1).max(1) as usize;
            push_block(&mut out, n, n.div_ceil(ranks), spacing, gap, 0);
        }
        Formation::Column => {
            push_block(&mut out, n, row.cols.unwrap_or(1) as usize, spacing, gap, 0);
        }
        Formation::Skirmish => {
            let deployed = (n as u64 * u64::from(row.deployed_pm)).div_ceil(1000) as usize;
            let deployed = deployed.min(n);
            let interval = row.interval_mm.unwrap_or(spacing);
            push_block(&mut out, deployed, deployed, interval, gap, 0);
            let offset = row.reserve_offset_mm.unwrap_or(gap);
            let cols = row.cols.unwrap_or(1) as usize;
            push_block(&mut out, n - deployed, cols, spacing, gap, offset);
        }
    }
    out
}

/// Sizes of the ranks of `layout`, front first: maximal runs of equal `v`.
fn rank_sizes(layout: &[SlotLocal]) -> Vec<usize> {
    let mut sizes: Vec<usize> = Vec::new();
    let mut last: Option<Fx> = None;
    for slot in layout {
        if last == Some(slot.v) {
            if let Some(size) = sizes.last_mut() {
                *size += 1;
            }
        } else {
            sizes.push(1);
            last = Some(slot.v);
        }
    }
    sizes
}

/// Assigns soldiers to the slots of `layout`, front rank first, left to right.
///
/// Each soldier is projected into the frame given by `anchor` and `heading` (raw `Fx` keys, no
/// rounding). Soldiers are sorted by `(v descending, u ascending, id)`, cut into groups the size
/// of each rank of `layout`, and each group is sorted by `(u ascending, id)` and given that
/// rank's slots in order. The result lists `(soldier, slot index)` in ascending slot index, so it
/// is the same for every ordering of `positions`.
///
/// `positions` and `layout` should have equal length. With fewer soldiers the rearmost ranks
/// stay short; surplus soldiers (the rearmost) receive no slot.
pub fn assign_slots(
    positions: &[(AgentId, Vec2Fx)],
    layout: &[SlotLocal],
    anchor: Vec2Fx,
    heading: Angle,
) -> Vec<(AgentId, usize)> {
    let forward = Vec2Fx::from_angle(heading);
    let right = Vec2Fx::from_angle(heading - Angle::QUARTER);
    // (v, u, id) with raw keys.
    let mut keyed: Vec<(i64, i64, AgentId)> = positions
        .iter()
        .map(|&(id, p)| {
            let d = p - anchor;
            (d.dot(forward).raw(), d.dot(right).raw(), id)
        })
        .collect();
    keyed.sort_unstable_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));

    let mut out = Vec::with_capacity(keyed.len().min(layout.len()));
    let mut start = 0usize;
    for size in rank_sizes(layout) {
        let end = (start + size).min(keyed.len());
        let group = &mut keyed[start..end];
        group.sort_unstable_by(|a, b| a.1.cmp(&b.1).then(a.2.cmp(&b.2)));
        for (j, entry) in group.iter().enumerate() {
            out.push((entry.2, start + j));
        }
        start += size;
        if start >= keyed.len() {
            break;
        }
    }
    out
}

/// The roster of a regiment: live agents whose `unit` is `unit` and that are neither
/// `F_DISABLED` nor `F_WOUNDED`, in ascending slot index.
pub fn regiment_roster(store: &AgentStore, unit: u32) -> Vec<AgentId> {
    store
        .iter_alive()
        .filter(|&id| {
            store.unit(id) == Some(unit)
                && store
                    .flags(id)
                    .is_some_and(|f| f & (F_DISABLED | F_WOUNDED) == 0)
        })
        .collect()
}
