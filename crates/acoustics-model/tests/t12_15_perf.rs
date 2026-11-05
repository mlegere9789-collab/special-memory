//! T12-15 (SPEC-012 sections 10 and 11): the performance budget H12, as an ignored test that prints
//! mean, p99 and max. Run with
//! `cargo test --release -p acoustics-model --test t12_15_perf -- --ignored --nocapture`.
//! The limits asserted are the section 10 numbers (the volley against its maximum, the rest
//! against means and the p99); they stay provisional until reference hardware records them.

mod common;
use acoustics_model::propagation::path_result;
use acoustics_model::{
    hearing_report, render_packet, scenes, to_mm, Aggregator, Cluster, Listener, ListenerKind,
    ListenerState, SoundEvent,
};
use common::*;
use std::time::Instant;

/// Mean, 99th percentile and maximum of `samples`, in the unit the caller names in `name`.
fn stats(name: &str, mut samples: Vec<u128>) -> (u128, u128, u128) {
    samples.sort_unstable();
    let mean = samples.iter().sum::<u128>() / samples.len() as u128;
    let p99 = samples[(samples.len() * 99 / 100).min(samples.len() - 1)];
    let max = *samples.last().unwrap();
    println!(
        "{name}: n={} mean={mean} p99={p99} max={max}",
        samples.len()
    );
    (mean, p99, max)
}

fn timed<R>(f: impl FnOnce() -> R) -> (R, u128) {
    let t = Instant::now();
    let r = f();
    (r, t.elapsed().as_micros())
}

/// 128 clusters: 16 units, 8 cells each.
fn clusters_128() -> Vec<Cluster> {
    let mut a = Aggregator::new(20);
    let mut seq = 0;
    for unit in 0..16u32 {
        for cell in 0..8 {
            let p = pos(150 * cell + 20 * unit as i32, 90 * unit as i32);
            a.push(&event(seq, (unit % 9) as u8, unit, p, 20));
            seq += 1;
        }
    }
    let c = a.flush(19);
    assert_eq!(c.len(), 128);
    c
}

fn listeners_64(kind: ListenerKind) -> Vec<Listener> {
    (0..64)
        .map(|k| Listener {
            id: k,
            kind,
            ..listener(1000 + 150 * k as i32, 200 * (k as i32 % 7))
        })
        .collect()
}

#[test]
#[ignore = "performance measurement; run on its own in release"]
fn volley_20k_aggregates_within_budget() {
    let events: Vec<SoundEvent> = scenes::volley_20k().events;
    assert_eq!(events.len(), 20_000);
    let samples = (0..50)
        .map(|_| {
            let (c, us) = timed(|| {
                let mut a = Aggregator::new(20);
                events.iter().for_each(|e| a.push(e));
                a.flush(19)
            });
            assert!(!c.is_empty());
            us
        })
        .collect();
    let (_, _, max) = stats("volley_20k aggregate (us)", samples);
    assert!(
        max <= 3_000,
        "aggregation max {max} us over the 3 ms budget"
    );
}

#[test]
#[ignore = "performance measurement; run on its own in release"]
fn sixty_four_listeners_over_128_clusters_stay_within_budget() {
    let t = tables();
    let w = preset(&t, "calm_sunny");
    let cl = clusters_128();
    let cams = listeners_64(ListenerKind::Camera);
    let hqs = listeners_64(ListenerKind::Headquarters);

    // A path: one listener against 128 clusters, divided by 128 (nanoseconds per path).
    let path_ns: Vec<u128> = (0..200)
        .map(|k| {
            let l = &cams[k % 64];
            let lis = [to_mm(l.pos.x), to_mm(l.pos.y)];
            let t0 = Instant::now();
            for c in &cl {
                std::hint::black_box(path_result(
                    &t,
                    &w,
                    c.centroid_mm,
                    c.height_mm,
                    lis,
                    1500,
                    None,
                ));
            }
            t0.elapsed().as_nanos() / 128
        })
        .collect();
    let (path_mean, _, _) = stats("path (ns)", path_ns);

    // Camera: one packet for one listener over 128 clusters, the cost of one window.
    let mut states = vec![ListenerState::default(); 64];
    let camera: Vec<u128> = (0..200)
        .map(|k| {
            let i = k % 64;
            timed(|| {
                std::hint::black_box(render_packet(&cams[i], &mut states[i], &cl, &t, &w, None))
            })
            .1
        })
        .collect();
    let (cam_mean, _, _) = stats("camera packet, 128 clusters (us)", camera);

    // Headquarters: the reports due on one tick, 64 listeners staggered over 20 ticks.
    let per_tick: Vec<u128> = (0..400)
        .map(|k| {
            timed(|| {
                for l in hqs.iter().skip(k % 20).step_by(20) {
                    std::hint::black_box(hearing_report(l, &cl, &t, &w, None, 20));
                }
            })
            .1
        })
        .collect();
    let (hq_mean, hq_p99, _) = stats("headquarters tick, 64 listeners staggered (us)", per_tick);

    assert!(path_mean <= 5_000, "a path costs {path_mean} ns over 5 us");
    assert!(
        cam_mean <= 1_000,
        "a camera packet costs {cam_mean} us over 1 ms"
    );
    assert!(hq_mean <= 2_000, "headquarters mean {hq_mean} us over 2 ms");
    assert!(hq_p99 <= 6_000, "headquarters p99 {hq_p99} us over 6 ms");
}
