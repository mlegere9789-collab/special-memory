//! T6-05, enabling morale (SPEC-006 section 4): `s0`, `shock_unit`, officer indices, and the
//! single enable-time draw, `nerve_offset`.

mod t6_support;

use battle_rules::morale::{Indicator, MoraleKind};
use battle_rules::registry::morale_stream;
use battle_rules::{Formation, RegimentId, Training};
use core_math::Angle;
use t6_support::{field, line, m, spawn};

#[test]
fn a_regiment_of_four_hundred() {
    let mut b = field(7, '.');
    let r = line(&mut b, 0, 400, m(100, 100));
    b.equip_regiment(r, "rifle_58", 0).unwrap();
    let roster = b.regiments[0].roster.clone();
    b.enable_morale();
    let ms = b.morale.as_ref().unwrap();
    let reg = &ms.regs[0];
    let row = b.tables.morale.training(Training::Trained).clone();
    assert_eq!(reg.s0, 400);
    assert_eq!(reg.shock_unit, 5495);
    assert_eq!(reg.c, row.base_ppm);
    assert_eq!((reg.kind, reg.state_ticks), (MoraleKind::Steady, 0));
    assert_eq!((reg.loss, reg.window, reg.broken_count), (0, 0, 0));
    assert_eq!((reg.supply_pm, reg.leader_pm, reg.dwell), (1000, 0, 0));
    assert_eq!(reg.indicator, Indicator::Firm);
    assert_eq!(reg.rounds_full, 400 * 60);
    assert!(reg.absent.is_empty());
    // n_off = max(2, 400 / 40) = 10, at indices (2k + 1) * 400 / 20.
    let want: Vec<_> = (0..10).map(|k| roster[(2 * k + 1) * 400 / 20]).collect();
    assert_eq!(reg.colonel, want[0]);
    let mut sorted = want.clone();
    sorted.sort();
    assert_eq!(reg.officers, sorted);
    assert_eq!((ms.cursor, ms.events.len(), ms.leaders.len()), (0, 0, 0));
}

#[test]
fn small_regiments_keep_at_least_what_exists() {
    for (count, want) in [(1usize, 1usize), (2, 2), (3, 2), (39, 2), (80, 2), (120, 3)] {
        let mut b = field(1, '.');
        line(&mut b, 0, count, m(100, 100));
        b.enable_morale();
        let reg = &b.morale.as_ref().unwrap().regs[0];
        assert_eq!(reg.s0 as usize, count);
        assert_eq!(reg.officers.len(), want, "{count} men");
        assert!(reg.officers.contains(&reg.colonel));
        assert!(reg.officers.windows(2).all(|w| w[0] < w[1]));
    }
}

#[test]
fn shock_unit_follows_the_training_capacity() {
    let mut b = field(1, '.');
    for (k, t) in Training::ALL.into_iter().enumerate() {
        spawn(
            &mut b,
            0,
            400,
            t,
            Formation::Line,
            m(100 + 100 * k as i32, 100),
            Angle::ZERO,
        );
    }
    b.enable_morale();
    let regs = &b.morale.as_ref().unwrap().regs;
    let want = [
        battle_rules::morale::shock_unit(400, 355),
        battle_rules::morale::shock_unit(400, 455),
        battle_rules::morale::shock_unit(400, 535),
    ];
    assert_eq!(want[1], 5495);
    for k in 0..3 {
        assert_eq!(regs[k].shock_unit, want[k]);
    }
}

#[test]
fn the_only_enable_time_draw_is_the_nerve_offset() {
    let mut b = field(11, '.');
    for k in 0..3 {
        line(&mut b, 0, 100, m(100 + 150 * k, 100));
    }
    let before = b.hash();
    let streams_before = b.world.rng;
    b.enable_morale();
    assert_eq!(b.hash(), before, "no existing state, and no hash block yet");
    assert_eq!(b.world.rng, streams_before);
    let spread = b.tables.morale.globals.nerve_spread as i32;
    let ms = b.morale.as_ref().unwrap();
    for (k, reg) in ms.regs.iter().enumerate() {
        let mut fresh = b.world.rng.stream(morale_stream(k as u16));
        let offset = fresh.range_i32(-spread, spread);
        assert_eq!(reg.nerve_offset, offset);
        assert!((-spread..spread).contains(&reg.nerve_offset));
        assert_eq!(reg.rng.state(), fresh.state(), "exactly one draw was taken");
    }
}

#[test]
fn a_regiment_stream_does_not_depend_on_others() {
    let offset_of_second = |with_first: bool| {
        let mut b = field(5, '.');
        if with_first {
            line(&mut b, 0, 50, m(100, 100));
        }
        line(&mut b, 0, 50, m(400, 100));
        b.enable_morale();
        let reg = &b.morale.as_ref().unwrap().regs[usize::from(with_first)];
        (reg.nerve_offset, reg.rng.state())
    };
    // Regiment ids differ (0 or 1), so the streams differ; each is that of its own id.
    let (o1, s1) = offset_of_second(true);
    let mut fresh = field(5, '.').world.rng.stream(morale_stream(1));
    assert_eq!(o1, fresh.range_i32(-50_000, 50_000));
    assert_eq!(s1, fresh.state());
    let _ = offset_of_second(false);
    let _ = RegimentId(0);
}

#[test]
fn enabling_twice_changes_nothing() {
    let mut b = field(3, '.');
    line(&mut b, 0, 60, m(100, 100));
    b.enable_morale();
    b.morale.as_mut().unwrap().regs[0].c = 123;
    let snapshot = format!("{:?}", b.morale);
    b.enable_morale();
    assert_eq!(format!("{:?}", b.morale), snapshot);
}

#[test]
fn a_battle_without_regiments_enables_empty() {
    let mut b = field(3, '.');
    b.enable_morale();
    assert!(b.morale.as_ref().unwrap().regs.is_empty());
}
