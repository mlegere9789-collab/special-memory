//! T6-07, flight and rally (SPEC-006 section 5.5 entry actions and rally conditions).

mod t64_support;
mod t6_support;

use battle_rules::morale::{Leader, MoraleKind};
use battle_rules::{Formation, Gait, Order, RegimentState};
use core_math::Fx;
use t64_support::{duel, kind, period, rm, rm_mut, set, tick};
use t6_support::m;

/// Metres of `Fx`, as a whole number for comparisons with a little slack.
fn near(a: Fx, metres: i32) -> bool {
    (a - Fx::from_int(metres)).abs() < Fx::from_raw(Fx::ONE.raw() / 50)
}

/// Pins a Broken regiment high, as a regiment that has recovered its nerve.
fn pinned_broken(gap: Option<i32>) -> battle_rules::BattleState {
    let mut b = duel(7, 400, gap);
    set(&mut b, 0, 900_000, MoraleKind::Broken);
    b
}

/// Runs pinned ticks until regiment 0 leaves Broken or `limit` ticks pass; returns the
/// `state_ticks` seen just before the call that rallied it.
fn ticks_to_rally(b: &mut battle_rules::BattleState, limit: u32) -> Option<u32> {
    for _ in 0..limit {
        let before = rm(b, 0).state_ticks;
        rm_mut(b, 0).c = 900_000;
        tick(b, Some(false));
        if kind(b, 0) != MoraleKind::Broken {
            return Some(before);
        }
    }
    None
}

#[test]
fn broken_stays_broken_under_pressure() {
    let mut b = pinned_broken(Some(100));
    assert!(ticks_to_rally(&mut b, 5000).is_none());
    assert_eq!(kind(&b, 0), MoraleKind::Broken);
}

#[test]
fn unpressed_broken_rallies_not_before_min_broken() {
    let mut b = pinned_broken(None);
    let min = b.tables.morale.globals.min_broken;
    let before = ticks_to_rally(&mut b, 2000).expect("it rallies");
    assert!(before + 1 >= min, "rallied after {before} ticks");
    assert!(before + 1 < min + 21, "rallied late, after {before} ticks");
    assert_eq!(kind(&b, 0), MoraleKind::Rallied);
}

#[test]
fn officerless_leaderless_regiment_waits_for_the_long_delay() {
    let mut b = pinned_broken(None);
    rm_mut(&mut b, 0).officers.clear();
    let long = b.tables.morale.globals.officerless_rally;
    let before = ticks_to_rally(&mut b, 8000).expect("it rallies in the end");
    assert!(before + 1 >= long, "rallied after {before} ticks");
    assert!(before + 1 < long + 21);
}

#[test]
fn a_leader_in_reach_replaces_the_officers() {
    let mut b = pinned_broken(None);
    rm_mut(&mut b, 0).officers.clear();
    let anchor = b.regiments[0].anchor;
    assert!(b.set_leader(Leader {
        id: 1,
        team: 0,
        pos: anchor,
        prestige_pm: 800,
        alive: true
    }));
    let min = b.tables.morale.globals.min_broken;
    let before = ticks_to_rally(&mut b, 3000).expect("it rallies");
    assert!(before + 1 >= min && before + 1 < min + 21);
}

/// A Wavering regiment pinned below the fall threshold falls back at the first period that
/// allows it, then breaks when pinned below the break threshold.
fn fall_and_break(b: &mut battle_rules::BattleState) {
    set(b, 0, 0, MoraleKind::Wavering);
    period(b, Some(true));
    assert_eq!(kind(b, 0), MoraleKind::FallingBack);
}

#[test]
fn falling_back_orders_a_quick_retreat_from_the_nearest_enemy() {
    let mut b = duel(3, 400, Some(300));
    let from = b.regiments[0].anchor;
    fall_and_break(&mut b);
    let Order::Move { to, gait } = b.regiments[0].order else {
        panic!("no flight order: {:?}", b.regiments[0].order)
    };
    assert_eq!(gait, Gait::Quick);
    let d = b.tables.morale.globals.flight_distance;
    assert!(
        near(to.x, from.x.to_int_floor() - (d / 1000) as i32),
        "{to:?}"
    );
    assert!(near(to.y, from.y.to_int_floor()), "{to:?}");
    set(&mut b, 0, 0, MoraleKind::FallingBack);
    period(&mut b, Some(true));
    assert_eq!(kind(&b, 0), MoraleKind::Broken);
    assert!(matches!(
        b.regiments[0].order,
        Order::Move {
            gait: Gait::Run,
            ..
        }
    ));
}

#[test]
fn without_an_enemy_the_retreat_is_straight_back() {
    let mut b = duel(3, 400, None);
    fall_and_break(&mut b);
    let Order::Move { to, .. } = b.regiments[0].order else {
        panic!("no flight order")
    };
    assert!(near(to.x, 100 - 300) && near(to.y, 100), "{to:?}");
}

#[test]
fn the_team_rally_point_is_the_retreat_when_set() {
    let mut b = duel(3, 400, Some(300));
    assert!(b.set_rally_point(0, Some(m(60, 500))));
    fall_and_break(&mut b);
    let Order::Move { to, .. } = b.regiments[0].order else {
        panic!("no flight order")
    };
    assert_eq!(to, m(60, 500));
}

#[test]
fn a_rally_point_of_another_team_is_ignored() {
    let mut b = duel(3, 400, Some(300));
    assert!(b.set_rally_point(1, Some(m(60, 500))));
    fall_and_break(&mut b);
    let Order::Move { to, .. } = b.regiments[0].order else {
        panic!("no flight order")
    };
    assert_ne!(to, m(60, 500));
}

#[test]
fn flight_is_recomputed_every_hundred_ticks() {
    let mut b = duel(3, 400, Some(300));
    fall_and_break(&mut b);
    let first = b.regiments[0].order;
    assert!(b.set_rally_point(0, Some(m(60, 500))));
    // Keep it falling back, below the exit and break lines.
    // The regiment has been in the state for 19 ticks; slow ticks fall on multiples of 20 and
    // the order is refreshed on the one at 100.
    for _ in 0..80 {
        set(&mut b, 0, 400_000, MoraleKind::FallingBack);
        tick(&mut b, Some(false));
    }
    assert_eq!(rm(&b, 0).state_ticks, 99);
    assert_eq!(b.regiments[0].order, first, "not yet");
    for _ in 0..2 {
        set(&mut b, 0, 400_000, MoraleKind::FallingBack);
        tick(&mut b, Some(false));
    }
    assert_eq!(
        b.regiments[0].order,
        Order::Move {
            to: m(60, 500),
            gait: Gait::Quick
        }
    );
}

#[test]
fn falling_back_cancels_pending_and_caps_the_forming_countdown() {
    let mut b = duel(3, 400, Some(300));
    assert!(b.issue_order(battle_rules::RegimentId(0), Order::Form(Formation::Column)));
    assert!(b.issue_order(
        battle_rules::RegimentId(0),
        Order::Move {
            to: m(900, 100),
            gait: Gait::Walk
        }
    ));
    let RegimentState::Forming { ticks_left, .. } = b.regiments[0].state else {
        panic!("not forming")
    };
    assert!(ticks_left > 20, "the drill table gives {ticks_left}");
    assert!(b.regiments[0].pending.is_some());
    fall_and_break(&mut b);
    let RegimentState::Forming { ticks_left, .. } = b.regiments[0].state else {
        panic!("left forming")
    };
    assert_eq!(ticks_left, 20);
    assert!(b.regiments[0].pending.is_none());
    assert!(matches!(
        b.regiments[0].order,
        Order::Move {
            gait: Gait::Quick,
            ..
        }
    ));
}

#[test]
fn a_countdown_below_the_cap_is_left_alone() {
    let mut b = duel(3, 400, Some(300));
    assert!(b.issue_order(battle_rules::RegimentId(0), Order::Form(Formation::Column)));
    if let RegimentState::Forming { ticks_left, .. } = &mut b.regiments[0].state {
        *ticks_left = 5;
    }
    fall_and_break(&mut b);
    let RegimentState::Forming { ticks_left, .. } = b.regiments[0].state else {
        panic!("left forming")
    };
    assert_eq!(ticks_left, 5);
}

#[test]
fn rallying_holds_the_regiment() {
    let mut b = duel(3, 400, None);
    fall_and_break(&mut b);
    assert!(matches!(b.regiments[0].order, Order::Move { .. }));
    let min = b.tables.morale.globals.min_fall;
    for _ in 0..min + 40 {
        rm_mut(&mut b, 0).c = 900_000;
        tick(&mut b, Some(false));
        if kind(&b, 0) == MoraleKind::Rallied {
            break;
        }
    }
    assert_eq!(kind(&b, 0), MoraleKind::Rallied);
    assert_eq!(b.regiments[0].order, Order::Hold);
}

#[test]
fn a_broken_line_actually_runs_away() {
    let mut b = duel(5, 100, Some(300));
    set(&mut b, 0, 0, MoraleKind::FallingBack);
    let mut broke_at = None;
    for t in 0..3000u32 {
        b.step();
        if broke_at.is_none() && kind(&b, 0) == MoraleKind::Broken {
            broke_at = Some((t, b.regiments[0].anchor));
        }
        if broke_at.is_some() && t >= 3000 - 1 {
            break;
        }
    }
    let (t0, a0) = broke_at.expect("it broke");
    let a1 = b.regiments[0].anchor;
    let moved = (a0 - a1).length();
    let run = i64::from(b.tables.locomotion.gait(Gait::Run).speed_mm_s);
    let secs = i64::from(3000 - t0) / 20;
    // At least a tenth of the nominal speed over the time it was running, or the whole
    // flight distance (it may have arrived and stopped).
    let flight = Fx::from_int(300);
    let want = battle_rules::fx_ratio(run * secs / 10, 1000).min(flight / Fx::from_int(2));
    assert!(moved >= want, "moved {moved:?} of {want:?}");
    assert!(a1.x < a0.x, "it ran toward the enemy");
}
