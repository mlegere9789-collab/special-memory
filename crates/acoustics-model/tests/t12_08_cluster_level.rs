//! T12-08: cluster level (SPEC-012 section 7).

mod common;
use acoustics_model::Aggregator;
use common::*;

fn level(window: u32, source: u8, n: u16) -> [i32; 3] {
    let t = tables();
    let mut a = Aggregator::new(window);
    a.push(&event(1, source, 1, pos(0, 0), n));
    a.flush(u64::from(window) - 1)[0].level_cdb(&t)
}

#[test]
fn forty_muskets_in_twenty_ticks() {
    let t = tables();
    let sel = t.sources()[0].level_cdb;
    let l = level(20, 0, 40);
    for b in 0..3 {
        assert!(
            (l[b] - (sel[b] + 1602)).abs() <= 1,
            "band {b}: {} vs {}",
            l[b],
            sel[b] + 1602
        );
    }
}

#[test]
fn a_four_tick_window_adds_699() {
    let l20 = level(20, 0, 40);
    let l4 = level(4, 0, 40);
    for b in 0..3 {
        assert!((l4[b] - l20[b] - 699).abs() <= 1, "band {b}");
    }
}

#[test]
fn a_single_event_in_a_one_second_window_is_the_source_level() {
    let t = tables();
    for (i, s) in t.sources().iter().enumerate() {
        let l = level(20, i as u8, 1);
        assert_eq!(l, s.level_cdb);
    }
}

#[test]
fn the_level_is_capped() {
    let l = level(4, 3, u16::MAX);
    assert!(l.iter().all(|&v| v <= 18300));
}

#[test]
fn an_unknown_source_is_silent() {
    let l = level(20, 200, 1);
    assert_eq!(l, [-20000; 3]);
}
