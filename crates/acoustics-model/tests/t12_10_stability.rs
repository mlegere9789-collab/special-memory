//! T12-10: packet stability and hysteresis (SPEC-012 section 8, camera packet).

mod common;
use acoustics_model::{packet_hash, render_packet, Aggregator, Cluster, ListenerState};
use common::*;

/// Thirteen musket clusters (one more than the class cap) at one spot; unit `k` fires `counts[k]`.
fn window(counts: &[u16], window_end: u64) -> Vec<Cluster> {
    let mut a = Aggregator::new(4);
    for (k, &n) in counts.iter().enumerate() {
        if n > 0 {
            a.push(&event(k as u64, 0, k as u32, pos(800, 0), n));
        }
    }
    a.flush(window_end)
}

fn ids(p: &acoustics_model::MixPacket) -> Vec<u64> {
    p.sources.iter().map(|s| s.id).collect()
}

#[test]
fn same_inputs_same_packet_and_hash() {
    let t = tables();
    let w = preset(&t, "calm");
    let cl = window(&[40; 13], 3);
    let lis = listener(0, 0);
    let a = render_packet(&lis, &mut ListenerState::default(), &cl, &t, &w, None);
    let b = render_packet(&lis, &mut ListenerState::default(), &cl, &t, &w, None);
    assert_eq!(a, b);
    assert_eq!(packet_hash(&a), packet_hash(&b));
    let other = render_packet(
        &lis,
        &mut ListenerState::default(),
        &window(&[41; 13], 3),
        &t,
        &w,
        None,
    );
    assert_ne!(packet_hash(&a), packet_hash(&other));
}

#[test]
fn hysteresis_keeps_the_selected_set() {
    let t = tables();
    let w = preset(&t, "calm");
    let lis = listener(0, 0);
    let cap = t.budget().class_caps[0] as usize;
    assert_eq!(cap, 12);
    let mut st = ListenerState::default();
    let first = render_packet(&lis, &mut st, &window(&[40; 13], 3), &t, &w, None);
    assert_eq!(first.sources.len(), cap);
    assert!(
        !ids(&first).contains(&window(&[40; 13], 3)[12].id),
        "unit 12 loses the tie"
    );
    // Unit 12 becomes a little louder (+79 cdB, below the hysteresis of 300).
    let mut counts = [40u16; 13];
    counts[12] = 48;
    let second_window = window(&counts, 7);
    let second = render_packet(&lis, &mut st, &second_window, &t, &w, None);
    assert_eq!(ids(&second), ids(&first));
    // A fresh state has no memory, so the louder unit wins a place.
    let fresh = render_packet(
        &lis,
        &mut ListenerState::default(),
        &second_window,
        &t,
        &w,
        None,
    );
    assert_ne!(ids(&fresh), ids(&first));
    assert!(ids(&fresh).contains(&second_window[12].id));
}

#[test]
fn a_clearly_louder_cluster_still_wins() {
    let t = tables();
    let w = preset(&t, "calm");
    let lis = listener(0, 0);
    let mut st = ListenerState::default();
    let first = render_packet(&lis, &mut st, &window(&[40; 13], 3), &t, &w, None);
    let mut counts = [40u16; 13];
    counts[12] = 400; // +1000 cdB
    let cl = window(&counts, 7);
    let second = render_packet(&lis, &mut st, &cl, &t, &w, None);
    assert!(ids(&second).contains(&cl[12].id));
    assert_ne!(ids(&second), ids(&first));
}

#[test]
fn a_source_that_drops_out_and_returns_keeps_its_id() {
    let t = tables();
    let w = preset(&t, "calm");
    let lis = listener(0, 0);
    let mut st = ListenerState::default();
    let c1 = window(&[40, 40, 40], 3);
    let p1 = render_packet(&lis, &mut st, &c1, &t, &w, None);
    let p2 = render_packet(&lis, &mut st, &window(&[40, 0, 40], 7), &t, &w, None);
    let p3 = render_packet(&lis, &mut st, &window(&[40, 40, 40], 11), &t, &w, None);
    assert_eq!(p2.sources.len(), 2);
    assert_eq!(ids(&p1), ids(&p3));
    assert_eq!(ids(&p1), c1.iter().map(|c| c.id).collect::<Vec<_>>());
}

#[test]
fn arrival_follows_the_distance() {
    let t = tables();
    let w = preset(&t, "calm");
    let cl = window(&[40], 3);
    let p = render_packet(
        &listener(0, 0),
        &mut ListenerState::default(),
        &cl,
        &t,
        &w,
        None,
    );
    let s = &p.sources[0];
    assert_eq!(s.distance_mm, 800_000);
    // 800 m at 343.42 m/s is 2330 ms, from window start tick 0.
    assert_eq!(s.arrival_ms, 2330);
    assert_eq!(s.span_ms, 200);
    assert_eq!(s.bearing.raw(), 0);
}
