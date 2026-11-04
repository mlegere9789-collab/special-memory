//! T12-07: aggregation invariance (SPEC-012 section 7).

mod common;
use acoustics_model::{Aggregator, SoundEvent};
use common::*;
use core_math::{Fx, Vec2Fx};
use std::collections::BTreeSet;

/// 20,000 events over 40 units, four sources, scattered over a 2 km square.
fn events() -> Vec<SoundEvent> {
    let mut r = Rng(0x1234_5678_9ABC_DEF1);
    (0..20_000u64)
        .map(|seq| {
            let unit = r.below(40) as u32;
            let source = [0u8, 3, 5, 8][r.below(4) as usize];
            let base_x = (unit as i32 % 8) * 300;
            let base_y = (unit as i32 / 8) * 300;
            let p = pos(
                base_x + r.below(250) as i32 - 125,
                base_y + r.below(250) as i32 - 125,
            );
            event(seq, source, unit, p, 1 + r.below(3) as u16)
        })
        .collect()
}

fn run(evs: &[SoundEvent]) -> Vec<acoustics_model::Cluster> {
    let mut a = Aggregator::new(20);
    for e in evs {
        a.push(e);
    }
    a.flush(19)
}

#[test]
fn shuffled_order_gives_identical_clusters() {
    let evs = events();
    let want = run(&evs);
    for seed in 1..=4u64 {
        let mut shuffled = evs.clone();
        Rng(seed * 7919).shuffle(&mut shuffled);
        assert_eq!(run(&shuffled), want, "seed {seed}");
    }
    let mut rev = evs.clone();
    rev.reverse();
    assert_eq!(run(&rev), want);
}

#[test]
fn counts_are_conserved_and_centroids_lie_in_the_box() {
    let evs = events();
    let cl = run(&evs);
    let total: u64 = evs.iter().map(|e| u64::from(e.count)).sum();
    assert_eq!(cl.iter().map(|c| u64::from(c.count)).sum::<u64>(), total);
    for c in &cl {
        for k in 0..2 {
            assert!(c.min_mm[k] <= c.centroid_mm[k] && c.centroid_mm[k] <= c.max_mm[k]);
        }
        assert!(c.extent_mm() >= 0);
    }
}

#[test]
fn flush_is_ascending_and_bounded_by_the_distinct_keys() {
    let evs = events();
    let cl = run(&evs);
    assert!(cl.windows(2).all(|w| w[0].id < w[1].id));
    let cell = 100_000i64;
    let keys: BTreeSet<(u8, u32, i64, i64)> = evs
        .iter()
        .map(|e| {
            let x = acoustics_model::to_mm(e.pos.x).div_euclid(cell);
            let y = acoustics_model::to_mm(e.pos.y).div_euclid(cell);
            (e.source, e.unit, x, y)
        })
        .collect();
    assert!(cl.len() <= keys.len());
    assert_eq!(cl.len(), keys.len());
}

#[test]
fn the_id_packs_source_unit_and_cell() {
    let mut a = Aggregator::new(4);
    a.push(&event(1, 3, 77, pos(250, -150), 1));
    let c = &a.flush(3)[0];
    let want = (3u64 << 56) | (77u64 << 32) | (2u64 << 16) | u64::from((-2i16) as u16);
    assert_eq!(c.id, want);
    assert_eq!((c.source, c.unit), (3, 77));
}

#[test]
fn flush_clears_and_records_the_window() {
    let mut a = Aggregator::new(20);
    a.push(&event(5, 0, 1, pos(0, 0), 2));
    a.push(&event(3, 0, 1, pos(10, 0), 0));
    let cl = a.flush(39);
    assert_eq!(cl.len(), 1);
    let c = &cl[0];
    assert_eq!((c.window_start_tick, c.window_ticks), (20, 20));
    assert_eq!(c.count, 3, "a count of zero is raised to one");
    assert_eq!(c.first_seq, 3);
    assert!(a.flush(59).is_empty());
}

#[test]
fn centroid_and_height_use_floor_division() {
    let mut a = Aggregator::new(4);
    // -50,000 mm and -49,999 mm: the mean -49,999.5 floors to -50,000.
    let near = Vec2Fx {
        x: Fx::from_int(-50) + Fx::from_raw(4_294_968),
        y: Fx::from_int(50),
    };
    let mut e1 = event(1, 0, 1, pos(-50, 50), 1);
    let mut e2 = event(2, 0, 1, near, 1);
    e1.height_mm = 1000;
    e2.height_mm = 1001;
    a.push(&e1);
    a.push(&e2);
    let c = &a.flush(3)[0];
    assert_eq!(c.centroid_mm, [-50_000, 50_000]);
    assert_eq!(c.height_mm, 1000);
    assert_eq!(c.extent_mm(), 1);
}
