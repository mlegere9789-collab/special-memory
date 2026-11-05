//! Hearing reports (SPEC-012 section 8): power sum without budgets, classes, sector, arrival and
//! the report hash.

mod common;
use acoustics_model::propagation::{delay_ms, sound_speed_mm_s};
use acoustics_model::{
    hearing_report, report_hash, Aggregator, Cluster, HearClass, HearingReport, Listener,
};
use common::*;

/// Clusters of `n` musket units (40 shots each) around `at`, window ending at tick `end`.
fn clusters(at: &[(i32, i32)], end: u64) -> Vec<Cluster> {
    let mut a = Aggregator::new(20);
    for (k, &(x, y)) in at.iter().enumerate() {
        a.push(&event(k as u64, 0, k as u32, pos(x, y), 40));
    }
    a.flush(end)
}

fn report(lis: &Listener, cl: &[Cluster], w: &str) -> HearingReport {
    let t = tables();
    hearing_report(lis, cl, &t, &preset(&t, w), None, 77)
}

#[test]
fn no_clusters_is_silent_and_empty() {
    let r = report(&listener(0, 0), &[], "calm");
    assert_eq!(r.class, HearClass::Silent);
    assert_eq!((r.listener, r.tick), (1, 77));
    assert_eq!(r.margin_cdb, -20000 - 3000);
    assert_eq!((r.loudest_sector, r.audible_clusters), (0, 0));
    assert_eq!(r.earliest_arrival_ms, 0);
}

#[test]
fn class_follows_the_margin_thresholds() {
    let t = tables();
    let g = t.globals();
    let cl = clusters(&[(0, 0)], 19);
    let mut seen = [false; 4];
    let mut last = i32::MAX;
    for m in [200, 1000, 2000, 4000, 8000, 12_000, 16_000, 24_000, 40_000] {
        let r = report(&listener(m, 0), &cl, "calm");
        let want = if r.margin_cdb < g.class_faint_cdb {
            HearClass::Silent
        } else if r.margin_cdb < g.class_clear_cdb {
            HearClass::Faint
        } else if r.margin_cdb < g.class_loud_cdb {
            HearClass::Clear
        } else {
            HearClass::Loud
        };
        assert_eq!(r.class, want, "{m} m");
        assert!(r.margin_cdb <= last, "margin never grows with distance");
        last = r.margin_cdb;
        seen[r.class as usize] = true;
    }
    assert!(seen[0] && seen[3], "the sweep reaches both ends");
}

#[test]
fn many_faint_clusters_add_up_and_no_budget_applies() {
    let one = clusters(&[(0, 0)], 19);
    let many = clusters(&[(0, 0); 16], 19);
    assert_eq!(many.len(), 16);
    let lis = listener(6000, 0);
    let (a, b) = (report(&lis, &one, "calm"), report(&lis, &many, "calm"));
    // Sixteen equal clusters add 10 log10 16 = 1204 cdB.
    assert!(
        (b.margin_cdb - a.margin_cdb - 1204).abs() <= 2,
        "{a:?} {b:?}"
    );
    // 100 clusters exceed every packet budget, and every one counts.
    let hundred = clusters(&[(0, 0); 100], 19);
    let r = report(&listener(1000, 0), &hundred, "calm");
    assert_eq!(r.audible_clusters, 100);
}

#[test]
fn audible_counts_clusters_at_or_above_ambient() {
    let cl = clusters(&[(0, 0), (0, 0)], 19);
    assert_eq!(report(&listener(1000, 0), &cl, "calm").audible_clusters, 2);
    assert_eq!(
        report(&listener(60_000, 0), &cl, "calm").audible_clusters,
        0
    );
}

#[test]
fn loudest_sector_is_the_sector_of_the_largest_power() {
    // Bearings from the listener at the origin: +x is sector 0, +y sector 2, -x sector 4.
    let lis = listener(0, 0);
    for (at, sector) in [
        ((2000, 0), 0),
        ((0, 2000), 2),
        ((-2000, 0), 4),
        ((0, -2000), 6),
    ] {
        let cl = clusters(&[at], 19);
        assert_eq!(report(&lis, &cl, "calm").loudest_sector, sector, "{at:?}");
    }
    // Two clusters: the nearer (louder) one decides.
    let cl = clusters(&[(3000, 0), (0, 1000)], 19);
    assert_eq!(report(&lis, &cl, "calm").loudest_sector, 2);
}

#[test]
fn earliest_arrival_is_window_start_plus_travel_time() {
    let t = tables();
    let w = preset(&t, "calm");
    let cl = clusters(&[(2000, 0), (5000, 0)], 39);
    let r = report(&listener(0, 0), &cl, "calm");
    let near = delay_ms(2_000_000, sound_speed_mm_s(&t, 200), 0);
    assert_eq!(r.earliest_arrival_ms, 20 * 50 + near);
    assert_eq!(w.temp_dc, 200);
}

#[test]
fn report_hash_is_stable_and_covers_every_field() {
    let cl = clusters(&[(2000, 0)], 19);
    let r = report(&listener(0, 0), &cl, "calm");
    assert_eq!(report_hash(&r), report_hash(&r.clone()));
    let mut variants = Vec::new();
    for k in 0..7 {
        let mut v = r;
        match k {
            0 => v.listener += 1,
            1 => v.tick += 1,
            2 => v.margin_cdb += 1,
            3 => v.class = HearClass::Loud,
            4 => v.loudest_sector += 1,
            5 => v.audible_clusters += 1,
            _ => v.earliest_arrival_ms += 1,
        }
        variants.push(report_hash(&v));
    }
    variants.push(report_hash(&r));
    variants.sort_unstable();
    variants.dedup();
    assert_eq!(variants.len(), 8, "each field changes the hash");
}
