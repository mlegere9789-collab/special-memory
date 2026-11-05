//! T12-15, engine half (SPEC-012 sections 8 and 9): `AcousticsEngine::end_tick` over a recorded
//! sound log equals the direct pipeline (aggregate, render, report), and the headquarters are
//! staggered over the twenty ticks after each window.

mod common;
use acoustics_model::{
    adapt_fire, hearing_report, render_packet, AcousticsEngine, Aggregator, HearingReport,
    Listener, ListenerKind, ListenerState, MixPacket, SoundEvent,
};
use battle_rules::{AmmoTable, FireRecord, FireRecordKind};
use common::*;
use std::collections::BTreeMap;

/// A deterministic log: the records of every tick, 0 to `ticks - 1`.
fn recorded_log(ticks: u64, a: &AmmoTable) -> Vec<Vec<FireRecord>> {
    let mut rng = Rng(0x5eed_1861);
    let ids = [
        "ball_69",
        "carbine_52",
        "shot_12",
        "shell_12",
        "case_12",
        "canister_12",
    ];
    (0..ticks)
        .map(|t| {
            (0..rng.below(14))
                .map(|k| {
                    let id = ids[rng.below(ids.len() as u64) as usize];
                    let kind = if rng.below(4) == 0 && id.ends_with("12") {
                        FireRecordKind::Burst
                    } else {
                        FireRecordKind::Shot
                    };
                    FireRecord {
                        kind,
                        seq: t * 100 + k,
                        unit: rng.below(12) as u32,
                        team: rng.below(2) as u8,
                        ammo: a.index_of(id).unwrap() as u8,
                        pos: pos(rng.below(600) as i32 - 300, rng.below(600) as i32 - 300),
                        z_mm: 1500,
                    }
                })
                .collect()
        })
        .collect()
}

fn lis(id: u32, kind: ListenerKind, x: i32, y: i32) -> Listener {
    Listener {
        id,
        kind,
        ..listener(x, y)
    }
}

/// Everything the direct pipeline says, keyed by tick.
#[derive(Default)]
struct Expected {
    packets: BTreeMap<u64, Vec<MixPacket>>,
    reports: BTreeMap<u64, Vec<HearingReport>>,
}

fn direct(log: &[Vec<SoundEvent>], listeners: &[Listener], weather: &str) -> Expected {
    let t = tables();
    let w = preset(&t, weather);
    let mut sorted = listeners.to_vec();
    sorted.sort_by_key(|l| l.id);
    let cams: Vec<_> = sorted
        .iter()
        .filter(|l| l.kind == ListenerKind::Camera)
        .collect();
    let hqs: Vec<_> = sorted
        .iter()
        .filter(|l| l.kind == ListenerKind::Headquarters)
        .collect();
    let mut states: BTreeMap<u32, ListenerState> = BTreeMap::new();
    let mut out = Expected::default();
    let (cw, hw) = (4u64, 20u64);
    for end in (cw - 1..log.len() as u64).step_by(cw as usize) {
        let mut a = Aggregator::new(cw as u32);
        log[(end + 1 - cw) as usize..=end as usize]
            .iter()
            .flatten()
            .for_each(|e| a.push(e));
        let cl = a.flush(end);
        let ps = cams
            .iter()
            .map(|l| render_packet(l, states.entry(l.id).or_default(), &cl, &t, &w, None))
            .collect();
        out.packets.insert(end, ps);
    }
    for end in (hw - 1..log.len() as u64).step_by(hw as usize) {
        let mut a = Aggregator::new(hw as u32);
        log[(end + 1 - hw) as usize..=end as usize]
            .iter()
            .flatten()
            .for_each(|e| a.push(e));
        let cl = a.flush(end);
        for (j, l) in hqs.iter().enumerate() {
            let tick = end + 1 + (j as u64 % hw);
            if tick < log.len() as u64 {
                let r = hearing_report(l, &cl, &t, &w, None, tick);
                out.reports.entry(tick).or_default().push(r);
            }
        }
    }
    out
}

fn run_engine(
    log: &[Vec<SoundEvent>],
    listeners: &[Listener],
    weather: &str,
    ticks: u64,
) -> Expected {
    let t = tables();
    let w = preset(&t, weather);
    let mut eng = AcousticsEngine::new(t, w, None);
    let mut out = Expected::default();
    for tick in 0..ticks {
        if let Some(evs) = log.get(tick as usize) {
            evs.iter().for_each(|e| eng.push(e));
        }
        let o = eng.end_tick(tick, listeners);
        if !o.packets.is_empty() {
            out.packets.insert(tick, o.packets);
        }
        if !o.reports.is_empty() {
            out.reports.insert(tick, o.reports);
        }
    }
    out
}

fn events(ticks: u64) -> Vec<Vec<SoundEvent>> {
    let ammo = AmmoTable::from_toml_str(include_str!("../../../data/rules/ammo.toml")).unwrap();
    recorded_log(ticks, &ammo)
        .iter()
        .map(|rs| rs.iter().filter_map(|r| adapt_fire(r, &ammo)).collect())
        .collect()
}

fn without_empty<K: Ord + Copy, V>(m: BTreeMap<K, Vec<V>>) -> BTreeMap<K, Vec<V>> {
    m.into_iter().filter(|(_, v)| !v.is_empty()).collect()
}

#[test]
fn end_tick_over_a_recorded_log_equals_the_direct_pipeline() {
    let log = events(100);
    // Unsorted ids: the engine orders listeners by id.
    let listeners = [
        lis(30, ListenerKind::Headquarters, 2000, 100),
        lis(5, ListenerKind::Camera, 900, 0),
        lis(10, ListenerKind::Headquarters, -3000, 0),
        lis(2, ListenerKind::Camera, 0, 700),
        lis(20, ListenerKind::Headquarters, 400, 400),
    ];
    for weather in ["calm", "toward_minus_x"] {
        let want = direct(&log, &listeners, weather);
        let got = run_engine(&log, &listeners, weather, 100);
        assert_eq!(got.packets.len(), 25, "a packet set per camera window");
        assert_eq!(got.packets, without_empty(want.packets));
        assert_eq!(got.reports.len(), 12, "three reports per completed window");
        assert_eq!(got.reports, without_empty(want.reports));
        assert!(got
            .packets
            .values()
            .flatten()
            .any(|p| !p.sources.is_empty()));
        assert!(got
            .packets
            .values()
            .all(|ps| ps.iter().map(|p| p.listener).collect::<Vec<_>>() == [2, 5]));
    }
}

#[test]
fn headquarters_are_staggered_over_the_twenty_ticks_after_each_window() {
    let log = events(100);
    let listeners: Vec<Listener> = (0..25)
        .map(|k| lis(100 + k, ListenerKind::Headquarters, 500 + 40 * k as i32, 0))
        .collect();
    let got = run_engine(&log, &listeners, "calm", 100);
    let want = direct(&log, &listeners, "calm");
    assert_eq!(got.reports, without_empty(want.reports));
    assert!(got.packets.is_empty(), "no camera listener, no packet");
    // One report per headquarters per completed window, on tick end + 1 + (j % 20).
    for end in [19u64, 39, 59, 79] {
        for j in 0..25u64 {
            let tick = end + 1 + j % 20;
            let n = got.reports.get(&tick).map_or(0, |v| {
                v.iter().filter(|r| r.listener == 100 + j as u32).count()
            });
            assert_eq!(n, 1, "hq {j} window {end}");
        }
    }
    let total: usize = got.reports.values().map(Vec::len).sum();
    assert_eq!(total, 25 * 4);
    // No tick carries more than two reports (j and j + 20 share a tick), which spreads the cost.
    assert!(got.reports.values().all(|v| v.len() <= 2));
}

#[test]
fn nothing_is_reported_before_the_first_window_completes() {
    let log = events(20);
    let listeners = [
        lis(1, ListenerKind::Camera, 800, 0),
        lis(2, ListenerKind::Headquarters, 800, 0),
    ];
    let got = run_engine(&log, &listeners, "calm", 20);
    assert_eq!(
        got.packets.keys().copied().collect::<Vec<_>>(),
        [3, 7, 11, 15, 19]
    );
    assert!(got.reports.is_empty());
}
