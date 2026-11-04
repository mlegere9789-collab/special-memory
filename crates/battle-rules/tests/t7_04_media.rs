//! T7-04: smoke-blocked flags wait and then fall back to a courier; a cut telegraph falls back at
//! send; telegraph latency does not depend on distance.

mod t73_support;

use battle_rules::command::{Medium, OfficerId};
use battle_rules::{smoke::SmokeGrid, Order};
use t73_support::*;

fn smoky(b: &mut battle_rules::BattleState, units: u32) {
    let mut g = SmokeGrid::new(100_000, 64, 64, core_math::Vec2Fx::ZERO).unwrap();
    g.add(metres(0, 0), units);
    g.add(metres(1000, 0), units);
    b.smoke = g;
}

fn flags_battle() -> battle_rules::BattleState {
    let mut b = field(1, 1, metres(5000, 5000));
    place(&mut b, 6, 0, 0);
    place(&mut b, 5, 1000, 0);
    b
}

#[test]
fn smoke_blocked_flags_wait_then_fall_back_to_courier() {
    let t = calm_table();
    let mut b = flags_battle();
    smoky(&mut b, 5000);
    b.send_message(&t, OfficerId(6), OfficerId(5), Some(Medium::Flags), hold())
        .unwrap();
    let first = b.cmd.as_ref().unwrap().msgs[0].due;
    let g = &t.globals;
    let mut switched = None;
    for tick in 0..first + 3000 {
        b.world.tick = tick;
        assert!(
            b.step_messages(&t).is_empty(),
            "nothing arrives through smoke"
        );
        let m = &b.cmd.as_ref().unwrap().msgs[0];
        if m.medium == Medium::Courier && switched.is_none() {
            switched = Some(tick);
        }
    }
    let waits = u64::from(g.smoke_max_waits) * u64::from(g.smoke_wait_ticks);
    assert_eq!(switched, Some(first + waits));
}

#[test]
fn flags_go_through_when_the_smoke_clears() {
    let t = calm_table();
    let mut b = flags_battle();
    smoky(&mut b, 5000);
    b.send_message(&t, OfficerId(6), OfficerId(5), Some(Medium::Flags), hold())
        .unwrap();
    let first = b.cmd.as_ref().unwrap().msgs[0].due;
    let wait = u64::from(t.globals.smoke_wait_ticks);
    let mut arrived = None;
    for tick in 0..first + 3000 {
        b.world.tick = tick;
        if tick == first + wait + 1 {
            b.smoke = SmokeGrid::empty();
        }
        if !b.step_messages(&t).is_empty() {
            arrived = Some(tick);
        }
    }
    assert_eq!(arrived, Some(first + 2 * wait));
}

#[test]
fn cut_telegraph_falls_back_at_send() {
    let t = calm_table();
    let mut b = field(1, 1, metres(5000, 5000));
    let e = b
        .estimate(&t, OfficerId(10), OfficerId(11), Some(Medium::Telegraph), 4)
        .unwrap();
    assert_eq!(e.medium, Medium::Telegraph);
    b.cmd.as_mut().unwrap().links[1].cut = true;
    let e = b
        .estimate(&t, OfficerId(10), OfficerId(11), Some(Medium::Telegraph), 4)
        .unwrap();
    assert_eq!(e.medium, Medium::Courier);
    b.send_message(
        &t,
        OfficerId(10),
        OfficerId(11),
        Some(Medium::Telegraph),
        hold(),
    )
    .unwrap();
    assert_eq!(b.cmd.as_ref().unwrap().msgs[0].medium, Medium::Courier);
}

#[test]
fn telegraph_latency_ignores_distance() {
    let t = calm_table();
    let mut b = field(1, 1, metres(5000, 5000));
    let at = |b: &mut battle_rules::BattleState, km: i64| {
        place(b, 10, 0, 0);
        place(b, 11, km * 1000, 0);
        b.estimate(&t, OfficerId(10), OfficerId(11), Some(Medium::Telegraph), 5)
            .unwrap()
            .eta_lo
    };
    assert_eq!(at(&mut b, 1), at(&mut b, 10));
    let _ = Order::Hold;
}

#[test]
fn auto_picks_the_smallest_eta_with_ties_to_the_lower_medium() {
    let t = calm_table();
    let mut b = field(1, 1, metres(5000, 5000));
    place(&mut b, 10, 0, 0);
    place(&mut b, 11, 3000, 0);
    let e = b
        .estimate(&t, OfficerId(10), OfficerId(11), None, 4)
        .unwrap();
    assert_eq!(e.medium, Medium::Telegraph);
    // No link between these two: the courier is the only choice.
    let e = b.estimate(&t, OfficerId(1), OfficerId(2), None, 4).unwrap();
    assert_eq!(e.medium, Medium::Courier);
    // Flags out of range are unusable.
    place(&mut b, 6, 0, 0);
    place(&mut b, 5, 9000, 0);
    let e = b
        .estimate(&t, OfficerId(6), OfficerId(5), Some(Medium::Flags), 4)
        .unwrap();
    assert_eq!(e.medium, Medium::Courier);
}
