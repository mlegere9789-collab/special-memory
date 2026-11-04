//! SPEC-002 Amendment A1 and mutation-hardening tests.

use core_math::{Angle, Fx, RngStreams, Vec2Fx};
use sim_state::{
    canonical_bytes, distance_sq_raw, fnv1a64, state_hash, AgentId, AgentInit, AgentStore,
    GridError, ReplayError, ReplayReader, ReplayWriter, SpatialGrid, WorldState,
};

fn v(x: i64, y: i64) -> Vec2Fx {
    Vec2Fx {
        x: Fx::from_raw(x),
        y: Fx::from_raw(y),
    }
}

// ---------------------------------------------------------------- replay reader

/// Hand-assembles a replay with a correct body hash.
/// `inputs` are `(tick, actor, seq)` with empty payloads, `cps` are `(tick, hash)`.
fn build(
    tick_hz: u16,
    interval: u32,
    inputs: &[(u64, u8, u16)],
    cps: &[(u64, u64)],
    total: u64,
) -> Vec<u8> {
    let mut b = b"HDRP".to_vec();
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(b"ab");
    b.extend_from_slice(&7u64.to_le_bytes());
    b.extend_from_slice(&tick_hz.to_le_bytes());
    b.push(0);
    b.extend_from_slice(&interval.to_le_bytes());
    b.extend_from_slice(&(inputs.len() as u32).to_le_bytes());
    for &(t, a, s) in inputs {
        b.extend_from_slice(&t.to_le_bytes());
        b.push(a);
        b.extend_from_slice(&s.to_le_bytes());
        b.extend_from_slice(&0u32.to_le_bytes());
    }
    b.extend_from_slice(&(cps.len() as u32).to_le_bytes());
    for &(t, h) in cps {
        b.extend_from_slice(&t.to_le_bytes());
        b.extend_from_slice(&h.to_le_bytes());
    }
    b.extend_from_slice(&total.to_le_bytes());
    b.extend_from_slice(&0xABCDu64.to_le_bytes());
    let h = fnv1a64(&b);
    b.extend_from_slice(&h.to_le_bytes());
    b
}

#[test]
fn well_formed_hand_built_replay_is_accepted() {
    let b = build(
        20,
        10,
        &[(1, 1, 1), (1, 1, 2), (1, 2, 0)],
        &[(10, 1), (20, 2)],
        20,
    );
    let r = ReplayReader::parse(&b).unwrap();
    assert_eq!(r.inputs().len(), 3);
    assert_eq!(r.checkpoints().len(), 2);
}

/// Targets the mutant that accepts equal consecutive input keys (`>` weakened to `>=`'s
/// complement, i.e. duplicates allowed).
#[test]
fn duplicate_input_keys_rejected() {
    let dup = build(20, 10, &[(5, 1, 3), (5, 1, 3)], &[], 0);
    assert!(
        matches!(
            ReplayReader::parse(&dup),
            Err(ReplayError::DuplicateInput { index: 1 })
        ),
        "{:?}",
        ReplayReader::parse(&dup)
    );
    // Duplicate that is not the first pair.
    let dup2 = build(20, 10, &[(1, 0, 0), (5, 1, 3), (5, 1, 3)], &[], 0);
    assert!(matches!(
        ReplayReader::parse(&dup2),
        Err(ReplayError::DuplicateInput { index: 2 })
    ));
    // Keys differing only in tick, actor or seq are fine.
    for inputs in [
        [(5u64, 1u8, 3u16), (6, 1, 3)],
        [(5, 1, 3), (5, 2, 3)],
        [(5, 1, 3), (5, 1, 4)],
    ] {
        assert!(ReplayReader::parse(&build(20, 10, &inputs, &[], 0)).is_ok());
    }
    // Writer: duplicates cannot be encoded either.
    let mut w = ReplayWriter::new("x", 1, 20, 0, 10);
    w.push_input(1, 1, 1, vec![1]);
    w.push_input(1, 1, 1, vec![2]);
    assert!(matches!(
        w.finish(0, 0),
        Err(ReplayError::DuplicateInput { .. })
    ));
}

#[test]
fn unsorted_input_keys_still_rejected_distinctly() {
    let b = build(20, 10, &[(5, 1, 3), (5, 1, 2)], &[], 0);
    assert!(matches!(
        ReplayReader::parse(&b),
        Err(ReplayError::InputsNotSorted { index: 1 })
    ));
}

#[test]
fn checkpoints_must_strictly_increase() {
    for cps in [
        vec![(10u64, 1u64), (10, 2)],
        vec![(10, 1), (20, 2), (15, 3)],
        vec![(20, 1), (10, 2)],
    ] {
        let b = build(20, 10, &[], &cps, 30);
        assert!(
            matches!(
                ReplayReader::parse(&b),
                Err(ReplayError::CheckpointsNotIncreasing { .. })
            ),
            "{cps:?} -> {:?}",
            ReplayReader::parse(&b)
        );
    }
    // The error names the offending checkpoint.
    let b = build(20, 10, &[], &[(10, 1), (20, 2), (20, 3)], 30);
    assert!(matches!(
        ReplayReader::parse(&b),
        Err(ReplayError::CheckpointsNotIncreasing { index: 2 })
    ));
    assert!(ReplayReader::parse(&build(20, 10, &[], &[(10, 1), (11, 2)], 30)).is_ok());
    let mut w = ReplayWriter::new("x", 1, 20, 0, 10);
    w.push_checkpoint(10, 1).push_checkpoint(10, 2);
    assert!(matches!(
        w.finish(30, 0),
        Err(ReplayError::CheckpointsNotIncreasing { .. })
    ));
}

#[test]
fn checkpoint_beyond_total_ticks_rejected() {
    let b = build(20, 10, &[], &[(10, 1), (21, 2)], 20);
    assert!(
        matches!(
            ReplayReader::parse(&b),
            Err(ReplayError::CheckpointBeyondTotal {
                index: 1,
                tick: 21,
                total_ticks: 20
            })
        ),
        "{:?}",
        ReplayReader::parse(&b)
    );
    // Exactly total_ticks is allowed.
    assert!(ReplayReader::parse(&build(20, 10, &[], &[(10, 1), (20, 2)], 20)).is_ok());
    let mut w = ReplayWriter::new("x", 1, 20, 0, 10);
    w.push_checkpoint(10, 1);
    assert!(matches!(
        w.finish(9, 0),
        Err(ReplayError::CheckpointBeyondTotal { .. })
    ));
}

#[test]
fn zero_tick_hz_rejected() {
    let b = build(0, 10, &[], &[], 5);
    assert_eq!(ReplayReader::parse(&b), Err(ReplayError::ZeroTickHz));
    assert!(ReplayReader::parse(&build(1, 10, &[], &[], 5)).is_ok());
    let w = ReplayWriter::new("x", 1, 0, 0, 10);
    assert_eq!(w.finish(0, 0), Err(ReplayError::ZeroTickHz));
}

#[test]
fn zero_checkpoint_interval_rejected_only_with_ticks() {
    let b = build(20, 0, &[], &[], 1);
    assert_eq!(
        ReplayReader::parse(&b),
        Err(ReplayError::ZeroCheckpointInterval)
    );
    let b = build(20, 0, &[], &[], u64::MAX);
    assert_eq!(
        ReplayReader::parse(&b),
        Err(ReplayError::ZeroCheckpointInterval)
    );
    // total_ticks == 0 may carry interval 0.
    assert!(ReplayReader::parse(&build(20, 0, &[], &[], 0)).is_ok());
    // A non-zero interval is fine for any total.
    assert!(ReplayReader::parse(&build(20, 1, &[], &[], 1)).is_ok());
    let w = ReplayWriter::new("x", 1, 20, 0, 0);
    assert_eq!(w.finish(1, 0), Err(ReplayError::ZeroCheckpointInterval));
    assert!(ReplayWriter::new("x", 1, 20, 0, 0).finish(0, 0).is_ok());
}

/// The body hash is verified before structural parsing beyond the fixed-size prefix, so a
/// corrupted body reports `BodyHashMismatch` even when it also contains a structural error.
#[test]
fn corruption_reports_body_hash_mismatch_before_structure() {
    let good = build(20, 10, &[(1, 1, 1), (2, 1, 1)], &[(10, 1), (20, 2)], 20);
    // fidelity class byte (offset 4 + 2 + 2 + 2 + 8 + 2 = 20)
    let mut b = good.clone();
    b[20] = 9;
    assert!(matches!(
        ReplayReader::parse(&b),
        Err(ReplayError::BodyHashMismatch { .. })
    ));
    // scenario id bytes become invalid UTF-8
    let mut b = good.clone();
    b[8] = 0xFF;
    b[9] = 0xFE;
    assert!(matches!(
        ReplayReader::parse(&b),
        Err(ReplayError::BodyHashMismatch { .. })
    ));
    // scenario id length beyond data
    let mut b = good.clone();
    b[6] = 0xFF;
    b[7] = 0xFF;
    assert!(matches!(
        ReplayReader::parse(&b),
        Err(ReplayError::BodyHashMismatch { .. })
    ));
    // input count huge
    let mut b = good.clone();
    b[25 + 0..25 + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(matches!(
        ReplayReader::parse(&b),
        Err(ReplayError::BodyHashMismatch { .. })
    ));
    // tick_hz zeroed, interval zeroed
    let mut b = good.clone();
    b[18] = 0;
    b[19] = 0;
    assert!(matches!(
        ReplayReader::parse(&b),
        Err(ReplayError::BodyHashMismatch { .. })
    ));
    // trailing garbage after the footer also surfaces as a hash problem, not a panic
    let mut b = good.clone();
    b.push(0);
    assert!(ReplayReader::parse(&b).is_err());
    // Magic and version belong to the fixed-size prefix and are still reported first.
    let mut b = good.clone();
    b[0] = b'X';
    assert_eq!(ReplayReader::parse(&b), Err(ReplayError::BadMagic));
    let mut b = good;
    b[4] = 2;
    assert_eq!(
        ReplayReader::parse(&b),
        Err(ReplayError::UnsupportedVersion(2))
    );
}

/// Bytes between the last footer field and the body hash are trailing bytes when the hash is
/// valid over them.
#[test]
fn trailing_bytes_inside_hashed_region_reported() {
    let good = build(20, 10, &[], &[], 0);
    let mut body = good[..good.len() - 8].to_vec();
    body.extend_from_slice(&[1, 2, 3]);
    let h = fnv1a64(&body);
    body.extend_from_slice(&h.to_le_bytes());
    assert_eq!(
        ReplayReader::parse(&body),
        Err(ReplayError::TrailingBytes(3))
    );
}

/// Targets the mutant that changes the checkpoint record-size constant used by the
/// allocation guard (for example 16 -> 8 or 1): the declared count must be rejected by the
/// guard, not by a later truncation.
#[test]
fn checkpoint_count_guard_uses_the_full_record_size() {
    // Header (no inputs), declared checkpoint count 2, but only the 16-byte footer follows:
    // 2 * 16 = 32 > 16 remaining. With a record size of 8 the guard would let it through.
    let mut b = b"HDRP".to_vec();
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(b"ab");
    b.extend_from_slice(&7u64.to_le_bytes());
    b.extend_from_slice(&20u16.to_le_bytes());
    b.push(0);
    b.extend_from_slice(&10u32.to_le_bytes());
    b.extend_from_slice(&0u32.to_le_bytes());
    b.extend_from_slice(&2u32.to_le_bytes());
    b.extend_from_slice(&5u64.to_le_bytes());
    b.extend_from_slice(&6u64.to_le_bytes());
    let h = fnv1a64(&b);
    b.extend_from_slice(&h.to_le_bytes());
    match ReplayReader::parse(&b) {
        Err(ReplayError::LengthExceedsData { declared: 2, .. }) => {}
        other => panic!("expected LengthExceedsData, got {other:?}"),
    }
    // Same for a huge declared count on a real-sized file.
    let mut b = build(20, 10, &[], &[(10, 1)], 10);
    let count_at = b.len() - 8 - 16 - 16 - 4;
    b[count_at..count_at + 4].copy_from_slice(&3u32.to_le_bytes());
    let n = b.len();
    let h = fnv1a64(&b[..n - 8]);
    b[n - 8..].copy_from_slice(&h.to_le_bytes());
    assert!(matches!(
        ReplayReader::parse(&b),
        Err(ReplayError::LengthExceedsData { declared: 3, .. })
    ));
}

/// The input-record guard likewise uses the minimum input record size (13 bytes).
#[test]
fn input_count_guard_uses_the_minimum_record_size() {
    // Declared 2 inputs; after the count, only 5 + 4 + 24 = ... the rest holds one record.
    let good = build(20, 10, &[(1, 0, 0)], &[], 0);
    let mut b = good.clone();
    let count_at = 4 + 2 + 2 + 2 + 8 + 2 + 1 + 4;
    b[count_at..count_at + 4].copy_from_slice(&3u32.to_le_bytes());
    let n = b.len();
    let h = fnv1a64(&b[..n - 8]);
    b[n - 8..].copy_from_slice(&h.to_le_bytes());
    assert!(matches!(
        ReplayReader::parse(&b),
        Err(ReplayError::LengthExceedsData { declared: 3, .. })
    ));
}

// ---------------------------------------------------------------- spatial grid

#[test]
fn grid_new_rejects_invalid_parameters() {
    let one = Fx::from_raw(1 << 32);
    assert!(matches!(
        SpatialGrid::new(one, 0, 4, Vec2Fx::ZERO),
        Err(GridError::ZeroDimension)
    ));
    assert!(matches!(
        SpatialGrid::new(one, 4, 0, Vec2Fx::ZERO),
        Err(GridError::ZeroDimension)
    ));
    assert!(matches!(
        SpatialGrid::new(Fx::ZERO, 4, 4, Vec2Fx::ZERO),
        Err(GridError::NonPositiveCellSize)
    ));
    assert!(matches!(
        SpatialGrid::new(Fx::from_raw(-5), 4, 4, Vec2Fx::ZERO),
        Err(GridError::NonPositiveCellSize)
    ));
    assert!(matches!(
        SpatialGrid::new(Fx::MIN, 4, 4, Vec2Fx::ZERO),
        Err(GridError::NonPositiveCellSize)
    ));
    // width * height overflow (checked) and absurd sizes.
    assert!(matches!(
        SpatialGrid::new(one, u32::MAX, u32::MAX, Vec2Fx::ZERO),
        Err(GridError::TooManyCells { .. })
    ));
    assert!(matches!(
        SpatialGrid::new(one, u32::MAX, 2, Vec2Fx::ZERO),
        Err(GridError::TooManyCells { .. })
    ));
    // Smallest representable positive cell is fine.
    assert!(SpatialGrid::new(Fx::from_raw(1), 1, 1, Vec2Fx::ZERO).is_ok());
    let g = SpatialGrid::new(one, 3, 5, Vec2Fx::ZERO).unwrap();
    assert_eq!((g.width_cells(), g.height_cells()), (3, 5));
    // Errors are readable.
    assert!(!GridError::ZeroDimension.to_string().is_empty());
}

#[test]
fn grid_cell_limit_is_two_to_the_26() {
    let one = Fx::from_raw(1 << 32);
    // Exactly 2^26 cells is accepted ...
    let g = SpatialGrid::new(one, 8192, 8192, Vec2Fx::ZERO).unwrap();
    assert_eq!(
        u64::from(g.width_cells()) * u64::from(g.height_cells()),
        1 << 26
    );
    drop(g);
    // ... one more is rejected, whichever way the factors fall.
    assert!(matches!(
        SpatialGrid::new(one, 67_108_865, 1, Vec2Fx::ZERO),
        Err(GridError::TooManyCells { cells: 67_108_865 })
    ));
    assert!(matches!(
        SpatialGrid::new(one, 1, 67_108_865, Vec2Fx::ZERO),
        Err(GridError::TooManyCells { .. })
    ));
}

/// Targets the mutant that replaces the saturating add in `distance_sq_raw` with a wrapping add.
#[test]
fn distance_sq_raw_saturates_at_the_far_corners() {
    let lo = v(i64::MIN, i64::MIN);
    let hi = v(i64::MAX, i64::MAX);
    assert_eq!(distance_sq_raw(lo, hi), u128::MAX);
    assert_eq!(distance_sq_raw(hi, lo), u128::MAX);
    // One axis alone fits exactly.
    let d = u128::from(u64::MAX);
    assert_eq!(distance_sq_raw(v(i64::MIN, 0), v(i64::MAX, 0)), d * d);
    assert_eq!(distance_sq_raw(v(0, 0), v(3, 4)), 25);
    assert_eq!(distance_sq_raw(v(3, 4), v(0, 0)), 25);

    // Through a query: the agent at the far corner is never inside any legal radius.
    let mut store = AgentStore::new();
    store.spawn(AgentInit {
        pos: hi,
        ..AgentInit::default()
    });
    let near = store.spawn(AgentInit {
        pos: v(i64::MIN + 5, i64::MIN + 5),
        ..AgentInit::default()
    });
    let mut grid =
        SpatialGrid::new(Fx::from_raw(1 << 40), 1000, 1000, v(i64::MIN, i64::MIN)).unwrap();
    grid.rebuild(&store);
    let mut out = Vec::new();
    grid.query_radius(lo, Fx::MAX, &mut out);
    assert_eq!(out, vec![near]);
    assert_eq!(grid.nearest(lo, Fx::MAX), Some(near));
}

fn grid_4(cells: u32) -> SpatialGrid {
    SpatialGrid::new(Fx::from_raw(4 << 32), cells, cells, Vec2Fx::ZERO).unwrap()
}

/// Targets the mutant that shifts the upper bound of the query cell range by one: agents lying
/// exactly at `center + radius`, which is the first coordinate of the next cell, must be found.
#[test]
fn query_range_upper_bound_includes_agents_on_the_cell_edge() {
    let mut store = AgentStore::new();
    let east = store.spawn(AgentInit {
        pos: v(4 << 32, 0),
        ..AgentInit::default()
    });
    let north = store.spawn(AgentInit {
        pos: v(0, 4 << 32),
        ..AgentInit::default()
    });
    let mut grid = grid_4(10);
    grid.rebuild(&store);
    let mut out = Vec::new();
    grid.query_radius(v(0, 0), Fx::from_raw(4 << 32), &mut out);
    assert_eq!(out, vec![east, north]);
    assert_eq!(grid.nearest(v(0, 0), Fx::from_raw(4 << 32)), Some(east));

    // Center in the middle of the grid; agents exactly on the four boundary points, each on
    // the first edge of the next cell on the upper side and last coordinate on the lower side.
    let mut store = AgentStore::new();
    let ids: Vec<AgentId> = [
        v(12 << 32, 8 << 32),
        v(8 << 32, 12 << 32),
        v(4 << 32, 8 << 32),
        v(8 << 32, 4 << 32),
        v((12 << 32) + 1, 8 << 32),
    ]
    .into_iter()
    .map(|p| {
        store.spawn(AgentInit {
            pos: p,
            ..AgentInit::default()
        })
    })
    .collect();
    grid.rebuild(&store);
    grid.query_radius(v(8 << 32, 8 << 32), Fx::from_raw(4 << 32), &mut out);
    assert_eq!(out, ids[..4].to_vec());
}

/// Pins the exact cell block that a query visits, so an off-by-one on either bound of the
/// range shows up as a different `cells_touched` count.
#[test]
fn query_range_visits_the_exact_cell_block() {
    let mut grid = grid_4(10);
    grid.rebuild(&AgentStore::new());
    let mut out = Vec::new();
    // [0 - 4, 0 + 4] clamps to cell 0 and reaches cell 1 on the edge: 2 x 2.
    grid.reset_cells_touched();
    grid.query_radius(v(0, 0), Fx::from_raw(4 << 32), &mut out);
    assert_eq!(grid.cells_touched(), 4);
    // [8 - 4, 8 + 4] = cells 1..=3 on each axis: 3 x 3.
    grid.reset_cells_touched();
    grid.query_radius(v(8 << 32, 8 << 32), Fx::from_raw(4 << 32), &mut out);
    assert_eq!(grid.cells_touched(), 9);
    // One raw unit less than the edge: cells 1..=2 on each axis, 2 x 2.
    grid.reset_cells_touched();
    grid.query_radius(v(8 << 32, 8 << 32), Fx::from_raw((4 << 32) - 1), &mut out);
    assert_eq!(grid.cells_touched(), 4);
}

#[test]
fn alive_queries_filter_stale_ids_and_snapshots_are_documented_behaviour() {
    let mut store = AgentStore::new();
    let a = store.spawn(AgentInit {
        pos: v(1 << 32, 1 << 32),
        ..AgentInit::default()
    });
    let b = store.spawn(AgentInit {
        pos: v(2 << 32, 1 << 32),
        ..AgentInit::default()
    });
    let c = store.spawn(AgentInit {
        pos: v(3 << 32, 1 << 32),
        ..AgentInit::default()
    });
    let mut grid = grid_4(10);
    grid.rebuild(&store);
    let center = v(1 << 32, 1 << 32);
    let r = Fx::from_raw(10 << 32);
    let mut out = Vec::new();

    // Despawn the nearest agent and reuse its slot for a new one far away: the snapshot still
    // lists the old id, the alive helpers do not.
    assert!(store.despawn(a));
    let a2 = store.spawn(AgentInit {
        pos: v(30 << 32, 30 << 32),
        ..AgentInit::default()
    });
    assert_eq!(a2.index, a.index);
    grid.query_radius(center, r, &mut out);
    assert_eq!(
        out,
        vec![a, b, c],
        "plain query is a snapshot of the last rebuild"
    );
    assert_eq!(grid.nearest(center, r), Some(a));
    grid.query_radius_alive(&store, center, r, &mut out);
    assert_eq!(out, vec![b, c]);
    assert_eq!(grid.nearest_alive(&store, center, r), Some(b));

    // Despawn everything: nothing alive remains, nearest_alive is None.
    assert!(store.despawn(b));
    assert!(store.despawn(c));
    grid.query_radius_alive(&store, center, r, &mut out);
    assert!(out.is_empty());
    assert_eq!(grid.nearest_alive(&store, center, r), None);

    // After a rebuild the alive helpers equal the plain queries.
    grid.rebuild(&store);
    grid.query_radius_alive(&store, v(30 << 32, 30 << 32), r, &mut out);
    assert_eq!(out, vec![a2]);
    let mut plain = Vec::new();
    grid.query_radius(v(30 << 32, 30 << 32), r, &mut plain);
    assert_eq!(out, plain);
    // Negative radius is empty for the alive helpers too.
    grid.query_radius_alive(&store, center, Fx::from_raw(-1), &mut out);
    assert!(out.is_empty());
    assert_eq!(grid.nearest_alive(&store, center, Fx::from_raw(-1)), None);
}

// ---------------------------------------------------------------- hashing and store

fn free_list_world(despawn_order: [usize; 3]) -> WorldState {
    let mut w = WorldState::new(RngStreams::new(0x5EED));
    let ids: Vec<AgentId> = (0..6i64)
        .map(|i| {
            w.agents.spawn(AgentInit {
                pos: v((i + 1) << 32, -((i + 2) << 32)),
                vel: v(i * 7, -i * 9),
                facing: Angle::from_raw((i as u32).wrapping_mul(0x1234_5678)),
                hp: 10 + i as u16,
                flags: 1 << i,
                team: (i % 2) as u8,
                unit: 500 + i as u32,
            })
        })
        .collect();
    for s in despawn_order {
        assert!(w.agents.despawn(ids[s]));
    }
    for _ in 0..3 {
        w.advance_tick();
    }
    w
}

/// Targets the mutant that sorts, reverses or drops the free list when hashing: the free-list
/// order is part of the hash. The literals were produced by an independent script that
/// assembled the documented layout by hand.
#[test]
fn free_list_order_is_part_of_the_hash() {
    let a = free_list_world([4, 1, 3]);
    assert_eq!(a.agents.free_list(), &[4, 1, 3]);
    assert_eq!(state_hash(&a), 3_926_891_744_952_446_897);
    let b = free_list_world([1, 3, 4]);
    assert_eq!(b.agents.free_list(), &[1, 3, 4]);
    assert_eq!(state_hash(&b), 3_916_343_889_620_329_489);
    let c = free_list_world([3, 1, 4]);
    assert_eq!(c.agents.free_list(), &[3, 1, 4]);
    assert_eq!(state_hash(&c), 9_928_234_676_027_754_225);
    // Slot contents are identical in all three; only the free-list order differs.
    let bytes = canonical_bytes(&a);
    let tail = &bytes[bytes.len() - 16..];
    let mut expect = Vec::new();
    for x in [3u32, 4, 1, 3] {
        expect.extend_from_slice(&x.to_le_bytes());
    }
    assert_eq!(tail, expect.as_slice());
    assert_eq!(
        &canonical_bytes(&a)[..bytes.len() - 16],
        &canonical_bytes(&b)[..bytes.len() - 16]
    );
}

/// Targets the mutant that narrows the slot generation counter (for example to `u8`): a slot
/// reused more than 256 times must never accept an id from an earlier life.
#[test]
fn generation_counter_is_wider_than_eight_bits() {
    let mut s = AgentStore::new();
    let mut ids = Vec::new();
    let mut id = s.spawn(AgentInit::default());
    ids.push(id);
    for _ in 0..700 {
        assert!(s.despawn(id));
        id = s.spawn(AgentInit::default());
        assert_eq!(id.index, 0);
        ids.push(id);
    }
    assert_eq!(id.generation, 700);
    assert!(s.is_alive(id));
    for (n, old) in ids.iter().enumerate().take(700) {
        assert!(!s.is_alive(*old), "stale id from life {n} accepted");
        assert!(!s.despawn(*old));
        assert_eq!(s.pos(*old), None);
        assert_eq!(s.hp(*old), None);
    }
    assert_eq!(s.len(), 1);
    // The generation is hashed as a full little-endian u32 (700 = 0x02BC).
    let mut w = WorldState::new(RngStreams::new(1));
    w.agents = s;
    let bytes = canonical_bytes(&w);
    assert_eq!(&bytes[20..24], &700u32.to_le_bytes());
}
