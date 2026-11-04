//! T9-11: the small-feature rule (SPEC-009 section 6, SPEC-004 amendment A6): men that cannot
//! see a target walk forward a step at a time, never beyond `peek_max_mm`; a `Move` order resets
//! the offset; a crest too high to see over sets `view_blocked`.

mod t022_support;
mod t9_support;

use battle_rules::{BattleState, ElevationGrid, FireMode, Gait, Order, RegimentId, Training};
use core_math::Angle;
use t022_support::{field, line, metres};
use t9_support::{env_table_with, weather};

const PEEK_MAX: u16 = 15_000;
/// One gate query per blind soldier per tick keeps the long runs short.
const FEW: [(&str, &str); 1] = [("retries = 4", "retries = 0")];

/// Open ground that rises from column 8 (68 m) to `crest_dm` decimetres at column 16 (132 m)
/// and falls away by column 19; a line of `men` stands 60 m behind the crest facing east, and
/// a line of 20 stands 128 m beyond it. `edits` change `environment.toml`. The men fire at will; the others hold.
fn crest(crest_dm: i32, men: usize, edits: &[(&str, &str)]) -> (BattleState, RegimentId) {
    let mut b = field(7);
    let profile = |col: i32| -> i32 {
        match col {
            ..=8 => 0,
            9..=16 => crest_dm * (col - 8) / 8,
            17..=19 => crest_dm * (19 - col) / 3,
            _ => 0,
        }
    };
    let row: Vec<i32> = (0..256).map(profile).collect();
    let rows = vec![row; 64];
    let s = b.spawn_regiment(line(
        0,
        men,
        Training::Trained,
        metres(72, 256),
        Angle::ZERO,
    ));
    let t = b.spawn_regiment(line(
        1,
        20,
        Training::Trained,
        metres(260, 256),
        Angle::HALF,
    ));
    b.equip_regiment(s, "rifle_58", 0).unwrap();
    assert!(b.set_fire_mode(s, FireMode::AtWill { target: t }));
    let table = env_table_with(edits);
    b.set_environment(table, weather(&[(0, 700, 800, 0, 0, 0)]));
    b.env
        .as_mut()
        .unwrap()
        .set_relief(ElevationGrid::from_rows(8000, 0, 0, &rows).unwrap());
    (b, s)
}

fn peeks(b: &BattleState, r: RegimentId) -> Vec<u16> {
    let reg = &b.regiments[usize::from(r.0)];
    reg.roster
        .iter()
        .map(|a| b.loco[a.index as usize].peek_mm)
        .collect()
}

#[test]
fn men_peek_forward_but_never_beyond_the_limit() {
    let (mut b, s) = crest(30, 10, &FEW);
    let mut top = 0;
    for tick in 0..1400 {
        b.step();
        if tick % 20 == 19 {
            for p in peeks(&b, s) {
                assert!(p <= PEEK_MAX, "peek {p} at tick {tick}");
                top = top.max(p);
            }
            assert!(b.regiments[0].peek_mm <= PEEK_MAX);
        }
    }
    assert_eq!(top, PEEK_MAX, "a blind soldier walks on to the limit");
}

/// Soldiers of the first regiment that have fired, and the regiment's size, after `ticks`.
fn fired_after(b: &mut BattleState, ticks: u32) -> (usize, usize) {
    let start: Vec<u16> = b.regiments[0]
        .roster
        .iter()
        .map(|a| b.fire[a.index as usize].rounds)
        .collect();
    for _ in 0..ticks {
        b.step();
    }
    let roster = &b.regiments[0].roster;
    let n = roster
        .iter()
        .zip(&start)
        .filter(|(a, r0)| b.fire[a.index as usize].rounds < **r0)
        .count();
    (n, roster.len())
}

#[test]
fn most_men_get_a_shot_over_a_low_crest_after_a_minute() {
    let (mut b, s) = crest(15, 30, &[]);
    let (fired, n) = fired_after(&mut b, 1200);
    assert!(fired * 1000 >= 800 * n, "{fired} of {n} fired");
    assert!(!b.view_blocked(s));
    assert!(
        b.counters().shots_withheld > 0,
        "some men had to peek first"
    );
    assert!(b.loco.iter().any(|l| l.peek_mm > 0));
}

#[test]
fn with_a_negligible_peek_step_the_crest_holds_men_back() {
    let (mut b, _) = crest(15, 30, &[("peek_step_mm = 500", "peek_step_mm = 1")]);
    let (fired, n) = fired_after(&mut b, 1200);
    assert!(fired * 1000 < 800 * n, "{fired} of {n} fired");
    assert!(b.loco.iter().all(|l| l.peek_mm < 100));
}

#[test]
fn a_move_order_zeroes_every_peek() {
    let (mut b, s) = crest(30, 10, &FEW);
    for _ in 0..900 {
        b.step();
    }
    assert!(peeks(&b, s).iter().any(|&p| p > 0));
    let to = metres(80, 256);
    assert!(b.issue_order(
        s,
        Order::Move {
            to,
            gait: Gait::Quick
        }
    ));
    assert!(peeks(&b, s).iter().all(|&p| p == 0));
    assert_eq!(b.regiments[usize::from(s.0)].peek_mm, 0);
}

#[test]
fn a_move_that_waits_for_a_transition_zeroes_the_peeks_when_it_starts() {
    let (mut b, s) = crest(30, 10, &FEW);
    for _ in 0..900 {
        b.step();
    }
    assert!(b.issue_order(s, Order::Form(battle_rules::Formation::Column)));
    let to = metres(80, 256);
    assert!(b.issue_order(
        s,
        Order::Move {
            to,
            gait: Gait::Quick
        }
    ));
    assert!(peeks(&b, s).iter().any(|&p| p > 0), "still pending");
    for _ in 0..3000 {
        b.step();
        if matches!(b.regiments[usize::from(s.0)].order, Order::Move { .. }) {
            break;
        }
    }
    assert!(peeks(&b, s).iter().all(|&p| p == 0));
}

#[test]
fn a_twelve_metre_crest_blocks_the_view() {
    let (mut b, s) = crest(120, 10, &FEW);
    for tick in 0..2400 {
        b.step();
        if b.view_blocked(s) {
            let i = b.env_inputs(s).unwrap();
            assert!(i.view_blocked && i.blind_pm >= 600, "{i:?} at tick {tick}");
            assert_eq!(b.counters().shots_fired, 0);
            return;
        }
    }
    panic!("the view was never reported blocked");
}

#[test]
fn quiet_men_step_back_and_the_flags_clear_each_period() {
    let (mut b, s) = crest(15, 30, &[]);
    for _ in 0..1200 {
        b.step();
    }
    // At the end of a period every soldier's blind and fired flags are clear.
    while b.world.tick % 20 != 0 {
        b.step();
    }
    let reg = &b.regiments[usize::from(s.0)];
    assert!(reg.roster.iter().all(|a| {
        let l = &b.loco[a.index as usize];
        !l.blind && !l.fired
    }));
    // A regiment whose only target is out of reach neither fires nor is blind: each period
    // the offsets fall by `peek_back_mm` (250), never below zero.
    let far = b.spawn_regiment(line(
        1,
        4,
        Training::Trained,
        metres(2000, 256),
        Angle::HALF,
    ));
    assert!(b.set_fire_mode(s, FireMode::AtWill { target: far }));
    while b.world.tick % 20 != 19 {
        b.step();
    }
    let before = peeks(&b, s);
    assert!(before.iter().any(|&p| p > 250));
    b.step();
    let after = peeks(&b, s);
    for (p0, p1) in before.iter().zip(&after) {
        assert_eq!(*p1, p0 - (*p0).min(250));
    }
}
