//! T12-06 (knife edge, foliage) and T12-12 (terrain ordering), SPEC-012 section 6 items 4b, 4c.
#![allow(clippy::needless_range_loop)]

use acoustics_model::propagation::{path_result, PathResult};
use acoustics_model::{AcousticTables, Relief, Weather};
use core_math::Vec2Fx;

const D: i64 = 2_000_000;
const CELL: i32 = 10_000;
const RIDGE_CELL: usize = 100;

fn tables() -> AcousticTables {
    AcousticTables::from_toml_str(include_str!("../../../data/rules/acoustics.toml")).unwrap()
}

fn calm(t: &AcousticTables) -> Weather {
    Weather::from_preset(t.weather("calm").unwrap())
}

/// A strip of cells along +x from the origin, one cell wide, zero elevation, class 0.
fn strip() -> Relief {
    let width = 201;
    Relief {
        origin: Vec2Fx::ZERO,
        cell_mm: CELL,
        width,
        height: 1,
        elev_mm: vec![0; width as usize],
        class: vec![0; width as usize],
    }
}

fn run(t: &AcousticTables, s: i64, hs: u16, l: i64, hr: u16, r: Option<&Relief>) -> PathResult {
    path_result(t, &calm(t), [s, 0], hs, [l, 0], hr, r).unwrap()
}

fn terrain_loss(t: &AcousticTables, r: &Relief) -> [i32; 3] {
    let with = run(t, 0, 1500, D, 1500, Some(r)).loss_cdb;
    let without = run(t, 0, 1500, D, 1500, None).loss_cdb;
    [0, 1, 2].map(|b| with[b] - without[b])
}

/// Maekawa loss in `f64` for one knife edge `s` mm from the source, `h` mm above the line.
fn maekawa(d: f64, s: f64, h: f64, c: f64, f_hz: f64, cap: f64) -> f64 {
    let e = |a: f64| (a * a + h * h).sqrt();
    let delta = (e(s) + e(d - s) - d).max(0.0);
    let n = 2.0 * delta / (c / f_hz);
    (10.0 * (3.0 + 20.0 * n).log10() * 100.0).min(cap)
}

#[test]
fn knife_edge_matches_maekawa() {
    let t = tables();
    let mut r = strip();
    r.elev_mm[RIDGE_CELL] = 41_500;
    let got = terrain_loss(&t, &r);
    let c = 343_420.0;
    for (b, want) in [1420, 1983, 2400].into_iter().enumerate() {
        assert!((got[b] - want).abs() <= 3, "band {b}: {} vs {want}", got[b]);
        let f = t.bands()[b].f_hz as f64;
        let reference = maekawa(2e6, 1e6, 40_000.0, c, f, 2400.0);
        assert!((f64::from(got[b]) - reference).abs() <= 3.0, "band {b} f64");
    }
}

#[test]
fn no_ridge_means_no_terrain_loss() {
    let t = tables();
    assert_eq!(terrain_loss(&t, &strip()), [0, 0, 0]);
    // A ridge that stays below the line of sight (cell 1.5 m high) is not a barrier.
    let mut r = strip();
    r.elev_mm[RIDGE_CELL] = 1500;
    assert_eq!(terrain_loss(&t, &r), [0, 0, 0]);
}

#[test]
fn barrier_loss_is_capped_and_grows_with_the_ridge() {
    let t = tables();
    let cap = t.globals().barrier_cap_cdb as i32;
    let mut last = [0i32; 3];
    for height in [3_000, 6_000, 12_000, 41_500, 200_000, 2_000_000] {
        let mut r = strip();
        r.elev_mm[RIDGE_CELL] = height;
        let l = terrain_loss(&t, &r);
        for b in 0..3 {
            assert!(l[b] >= last[b] && l[b] <= cap, "height {height} band {b}");
        }
        last = l;
    }
    assert_eq!(last, [cap; 3]);
}

#[test]
fn woods_strip_adds_foliage_by_band() {
    let t = tables();
    let mut r = strip();
    // 500 m of woods (class 3: 600 cdB per 100 m, canopy 15 m): cells 50..100, 25 samples.
    for cell in 50..100 {
        r.class[cell] = 3;
    }
    let want = |pm: i64| (25 * 600 * 20_000 * pm / (100_000 * 1000)) as i32;
    let got = terrain_loss(&t, &r);
    for b in 0..3 {
        assert_eq!(got[b], want(t.bands()[b].foliage_pm), "band {b}");
    }
    assert_eq!(got, [600, 3000, 6000]);
    // A line of sight above the canopy passes clear of the foliage.
    let high = path_result(&t, &calm(&t), [0, 0], 16_000, [D, 0], 16_000, Some(&r)).unwrap();
    let flat = path_result(&t, &calm(&t), [0, 0], 16_000, [D, 0], 16_000, None).unwrap();
    assert_eq!(high.loss_cdb, flat.loss_cdb);
    // A class without foliage in the table adds nothing.
    let mut plain = strip();
    for cell in 50..100 {
        plain.class[cell] = 1;
    }
    assert_eq!(terrain_loss(&t, &plain), [0, 0, 0]);
}

#[test]
fn lookup_uses_the_nearest_cell_and_clamps() {
    let t = tables();
    // The origin is in metres; the grid starts 100 m west of the source.
    let mut r = strip();
    r.origin = Vec2Fx::new(core_math::Fx::from_int(-100), core_math::Fx::ZERO);
    r.elev_mm[110] = 41_500; // cell 110 covers x = 1,000,000 .. 1,010,000 mm
    let moved = terrain_loss(&t, &r);
    let mut base = strip();
    base.elev_mm[RIDGE_CELL] = 41_500;
    assert_eq!(moved, terrain_loss(&t, &base));
    // Points outside the grid take the edge cell: a high edge cell at the far end under
    // the listener raises the listener's own ground, which cannot add loss.
    let mut edge = strip();
    edge.elev_mm[200] = 50_000;
    assert_eq!(terrain_loss(&t, &edge), [0, 0, 0]);
}

#[test]
fn zero_relief_equals_no_relief_and_terrain_never_lowers_loss() {
    let t = tables();
    let zero = strip();
    for (s, hs, l, hr) in [
        (0, 1500, D, 1500),
        (0, 0, 640_000, 0),
        (0, 1500, 1_990_000, 1700),
    ] {
        assert_eq!(
            run(&t, s, hs, l, hr, Some(&zero)),
            run(&t, s, hs, l, hr, None)
        );
    }
    let base = run(&t, 0, 1500, D, 1500, None).loss_cdb;
    for cell in [20usize, 50, 99, 100, 101, 150, 180] {
        for h in [0, 1_000, 5_000, 30_000] {
            for class in [0u8, 2, 3, 4] {
                let mut r = strip();
                r.elev_mm[cell] = h;
                r.class[cell] = class;
                let l = run(&t, 0, 1500, D, 1500, Some(&r)).loss_cdb;
                for b in 0..3 {
                    assert!(l[b] >= base[b], "cell {cell} h {h} class {class} band {b}");
                }
            }
        }
    }
    // Woods and a ridge together never beat either alone.
    let mut both = strip();
    both.elev_mm[RIDGE_CELL] = 41_500;
    for cell in 60..100 {
        both.class[cell] = 3;
    }
    let mut ridge = strip();
    ridge.elev_mm[RIDGE_CELL] = 41_500;
    let (a, b) = (
        run(&t, 0, 1500, D, 1500, Some(&both)).loss_cdb,
        run(&t, 0, 1500, D, 1500, Some(&ridge)).loss_cdb,
    );
    for k in 0..3 {
        assert!(a[k] >= b[k]);
    }
}

#[test]
fn swapping_source_and_listener_gives_the_same_loss() {
    let t = tables();
    let mut r = strip();
    r.elev_mm[RIDGE_CELL] = 41_500;
    for cell in 120..140 {
        r.class[cell] = 3;
        r.elev_mm[cell] = 1_200;
    }
    for (hs, hr) in [(1500u16, 1500u16), (1500, 1700), (900, 2400)] {
        let ab = run(&t, 0, hs, D, hr, Some(&r));
        let ba = run(&t, D, hr, 0, hs, Some(&r));
        assert_eq!(ab.loss_cdb, ba.loss_cdb, "heights {hs} {hr}");
        assert_eq!(ab.delay_ms, ba.delay_ms);
    }
}

#[test]
fn path_without_relief_is_unchanged_and_long_paths_cap_the_samples() {
    let t = tables();
    // 100 km: the profile has at most max_samples samples and the call stays exact.
    let far = 100_000_000;
    let mut r = strip();
    r.width = 10_001;
    r.elev_mm = vec![0; 10_001];
    r.class = vec![0; 10_001];
    r.elev_mm[5_000] = 80_000;
    let a = path_result(&t, &calm(&t), [0, 0], 1500, [far, 0], 1500, Some(&r)).unwrap();
    let b = path_result(&t, &calm(&t), [0, 0], 1500, [far, 0], 1500, None).unwrap();
    assert!(a.loss_cdb[1] >= b.loss_cdb[1]);
    assert_eq!(a.delay_ms, b.delay_ms);
    assert_eq!(a.distance_mm, b.distance_mm);
}

#[test]
fn camera_packet_hears_the_ridge() {
    use acoustics_model::SoundEvent;
    use acoustics_model::{render_packet, Aggregator, Listener, ListenerKind, ListenerState};
    let t = tables();
    let w = calm(&t);
    let m = |v: i32| core_math::Fx::from_int(v);
    let mut agg = Aggregator::new(4);
    agg.push(&SoundEvent {
        seq: 0,
        source: 3,
        unit: 1,
        team: 0,
        pos: Vec2Fx::new(m(0), m(0)),
        height_mm: 1500,
        count: 1,
    });
    let cl = agg.flush(3);
    let lis = Listener {
        id: 1,
        kind: ListenerKind::Camera,
        pos: Vec2Fx::new(m(2000), m(0)),
        height_mm: 1500,
        ambient_cdb: [0, 0, 0],
    };
    let mut r = strip();
    r.elev_mm[RIDGE_CELL] = 41_500;
    let open = render_packet(&lis, &mut ListenerState::default(), &cl, &t, &w, None);
    let shut = render_packet(&lis, &mut ListenerState::default(), &cl, &t, &w, Some(&r));
    let (a, b) = (&open.sources[0], &shut.sources[0]);
    assert_eq!((a.arrival_ms, a.distance_mm), (b.arrival_ms, b.distance_mm));
    for k in 0..3 {
        assert!(b.level_cdb[k] < a.level_cdb[k], "band {k}");
    }
}
