//! T12-09: budgets and conservation (SPEC-012 sections 7 and 8, camera packet).

mod common;
use acoustics_model::intmath::{cdb_of_power, power_of_cdb};
use acoustics_model::propagation::path_result;
use acoustics_model::{render_packet, to_mm, Aggregator, Cluster, ListenerState};
use common::*;

fn clusters() -> Vec<Cluster> {
    let mut r = Rng(42);
    let mut a = Aggregator::new(4);
    for seq in 0..20_000u64 {
        let unit = r.below(100) as u32;
        let source = [0u8, 3, 5, 8, 6][r.below(5) as usize];
        let x = (unit as i32 % 10) * 250 + r.below(200) as i32;
        let y = (unit as i32 / 10) * 250 + r.below(200) as i32;
        a.push(&event(seq, source, unit, pos(x, y), 1));
    }
    a.flush(3)
}

#[test]
fn caps_and_order_hold() {
    let t = tables();
    let cl = clusters();
    assert!(cl.len() > 100, "the scene must exceed the budget");
    let lis = listener(1200, 1200);
    let p = render_packet(
        &lis,
        &mut ListenerState::default(),
        &cl,
        &t,
        &preset(&t, "calm"),
    );
    let b = t.budget();
    assert!(p.sources.len() <= b.max_total as usize);
    assert!(!p.sources.is_empty() && !p.bed.is_empty());
    assert!(p.sources.windows(2).all(|w| w[0].id < w[1].id));
    assert!(p.bed.windows(2).all(|w| w[0].sector < w[1].sector));
    let mut per_class = [0u32; 4];
    for s in &p.sources {
        per_class[usize::from(t.sources()[usize::from(s.source)].class)] += 1;
    }
    for (k, n) in per_class.iter().enumerate() {
        assert!(*n <= b.class_caps[k], "class {k}: {n}");
    }
    assert_eq!((p.listener, p.window_start_tick, p.window_ticks), (1, 0, 4));
}

#[test]
fn power_is_conserved_between_sources_and_bed() {
    let t = tables();
    let cl = clusters();
    let w = preset(&t, "calm");
    // A distant listener, so that every band has clusters near the drop threshold.
    for (lx, ly) in [(1200, 1200), (3000, 2500)] {
        let lis = listener(lx, ly);
        let p = render_packet(&lis, &mut ListenerState::default(), &cl, &t, &w);
        let mut kept = [0u128; 3];
        for c in &cl {
            let r = path_result(
                &t,
                &w,
                c.centroid_mm,
                c.height_mm,
                [to_mm(lis.pos.x), to_mm(lis.pos.y)],
                lis.height_mm,
            )
            .unwrap();
            let lv = c.level_cdb(&t);
            let loud = (0..3)
                .map(|b| lv[b] - r.loss_cdb[b] - lis.ambient_cdb[b])
                .max()
                .unwrap();
            if loud >= t.globals().drop_below_cdb {
                for b in 0..3 {
                    kept[b] += power_of_cdb(lv[b] - r.loss_cdb[b]);
                }
            }
        }
        let mut got = [0u128; 3];
        let levels = p
            .sources
            .iter()
            .map(|s| s.level_cdb)
            .chain(p.bed.iter().map(|s| s.level_cdb));
        for l in levels {
            for (g, v) in got.iter_mut().zip(l) {
                *g += power_of_cdb(v);
            }
        }
        for b in 0..3 {
            let (a, c) = (cdb_of_power(kept[b]), cdb_of_power(got[b]));
            assert!(
                (a - c).abs() <= 1,
                "listener ({lx},{ly}) band {b}: {a} vs {c}"
            );
        }
    }
}

#[test]
fn bed_counts_cover_the_unselected_clusters() {
    let t = tables();
    let cl = clusters();
    let lis = listener(1200, 1200);
    let p = render_packet(
        &lis,
        &mut ListenerState::default(),
        &cl,
        &t,
        &preset(&t, "calm"),
    );
    let in_sources: u64 = p.sources.iter().map(|s| u64::from(s.count)).sum();
    let in_bed: u64 = p.bed.iter().map(|s| u64::from(s.count)).sum();
    let total: u64 = cl.iter().map(|c| u64::from(c.count)).sum();
    assert!(in_sources + in_bed <= total);
    assert!(in_bed > 0 && p.bed.iter().all(|s| s.sector < 8));
}

#[test]
fn bed_sector_follows_the_bearing() {
    let t = tables();
    // Nine voice clusters exceed the class cap of eight, so one lands in the bed. Bearing zero is
    // sector 0 and a quarter turn is 2.
    for (x, y, sector) in [
        (500, 0, 0u8),
        (400, 400, 1),
        (0, 500, 2),
        (-500, 0, 4),
        (0, -500, 6),
    ] {
        let mut a = Aggregator::new(4);
        for unit in 0..9 {
            a.push(&event(u64::from(unit), 8, unit, pos(x, y), 1));
        }
        let cl = a.flush(3);
        let mut lis = listener(0, 0);
        lis.ambient_cdb = [0, 0, 0];
        let p = render_packet(
            &lis,
            &mut ListenerState::default(),
            &cl,
            &t,
            &preset(&t, "calm"),
        );
        assert_eq!(p.sources.len(), 8);
        assert_eq!(p.bed.len(), 1, "({x},{y})");
        assert_eq!(p.bed[0].sector, sector, "({x},{y})");
        assert_eq!(p.bed[0].count, 1);
    }
}

#[test]
fn faint_clusters_are_dropped() {
    let t = tables();
    let mut a = Aggregator::new(4);
    a.push(&event(1, 8, 1, pos(60_000, 0), 1));
    let cl = a.flush(3);
    let p = render_packet(
        &listener(0, 0),
        &mut ListenerState::default(),
        &cl,
        &t,
        &preset(&t, "calm"),
    );
    assert!(p.sources.is_empty() && p.bed.is_empty());
}
