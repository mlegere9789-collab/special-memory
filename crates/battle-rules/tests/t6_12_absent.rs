//! T6-12, absentees, symptoms and officers (SPEC-006 sections 5.1, 5.5 and 5.6).

mod t64_support;
mod t6_support;

use battle_rules::morale::{MoraleKind, F_LOOKBACK, F_SKULK};
use battle_rules::{BattleState, CasualtyKind, Order};
use core_math::Rng;
use t64_support::{cas, duel, period, rm, rm_mut, set, tick};

fn flagged(b: &BattleState, flag: u16) -> usize {
    b.regiments[0]
        .roster
        .iter()
        .filter(|a| b.world.agents.flags(**a).unwrap() & flag != 0)
        .count()
}

/// Pins regiment 0 Broken and runs `periods` whole periods; the regiment keeps its nerve low so
/// that the ranks thin.
fn rout(b: &mut BattleState, periods: u32) {
    for _ in 0..periods {
        set(b, 0, 100_000, MoraleKind::Broken);
        period(b, Some(false));
    }
}

#[test]
fn skulkers_leave_the_roster_and_mark_the_regiment_dirty() {
    let mut b = duel(11, 400, None);
    set(&mut b, 0, 100_000, MoraleKind::Broken);
    let before = b.regiments[0].roster.len();
    period(&mut b, Some(false));
    rout(&mut b, 19);
    let r = rm(&b, 0);
    assert!(!r.absent.is_empty(), "nobody left in 20 s of rout");
    assert!(r.absent.windows(2).all(|w| w[0] < w[1]), "ascending");
    assert_eq!(b.regiments[0].roster.len() + r.absent.len(), before);
    assert!(b.regiments[0].dirty);
    let skulk_mm = b.tables.morale.globals.skulk_rear;
    for a in &r.absent {
        assert_ne!(b.world.agents.flags(*a).unwrap() & F_SKULK, 0);
        assert!(!b.regiments[0].roster.contains(a));
        let slot = b.loco[a.index as usize].slot;
        assert_eq!(slot.v, battle_rules::fx_ratio(-skulk_mm, 1000));
    }
}

#[test]
fn the_departure_rate_follows_skulk_ppm() {
    // Broken: 6000 ppm per second per soldier; over 40 s of 400 men the expected number is
    // 400 * (1 - (1 - 0.006)^40).
    let mut total = 0.0f64;
    let mut seen = 0usize;
    for seed in 1..=6u64 {
        let mut b = duel(seed, 400, None);
        rout(&mut b, 40);
        seen += rm(&b, 0).absent.len();
        total += 400.0 * (1.0 - 0.994f64.powi(40));
    }
    let sigma = (total * 0.8).sqrt();
    assert!(
        (seen as f64 - total).abs() < 3.0 * sigma,
        "{seen} absent, expected {total:.0} +- {sigma:.0}"
    );
}

#[test]
fn officers_never_skulk() {
    let mut b = duel(5, 400, None);
    let officers = rm(&b, 0).officers.clone();
    assert!(officers.len() >= 10);
    rout(&mut b, 150);
    assert!(rm(&b, 0).absent.len() > 100);
    assert_eq!(rm(&b, 0).officers, officers);
    for o in &officers {
        assert!(!rm(&b, 0).absent.contains(o));
        assert_eq!(b.world.agents.flags(*o).unwrap() & F_SKULK, 0);
    }
}

#[test]
fn nobody_skulks_while_steady() {
    let mut b = duel(5, 400, None);
    for _ in 0..30 {
        set(&mut b, 0, 1_000_000, MoraleKind::Steady);
        period(&mut b, Some(false));
    }
    assert!(rm(&b, 0).absent.is_empty());
}

#[test]
fn lookback_is_cleared_and_redrawn_each_period() {
    let mut b = duel(8, 400, None);
    // Everybody looks back, then a steady period clears most of it.
    for a in b.regiments[0].roster.clone() {
        let f = b.world.agents.flags(a).unwrap();
        b.world.agents.set_flags(a, f | F_LOOKBACK);
    }
    period(&mut b, Some(false));
    let steady = flagged(&b, F_LOOKBACK);
    assert!(steady < 40, "{steady} of 400 look back when steady");
    // Broken: 800 permille.
    set(&mut b, 0, 100_000, MoraleKind::Broken);
    tick(&mut b, Some(false));
    let broken = flagged(&b, F_LOOKBACK);
    let n = b.regiments[0].roster.len();
    assert!(n > 350);
    let sigma = (n as f64 * 0.16).sqrt();
    assert!(
        (broken as f64 - 0.8 * n as f64).abs() < 3.0 * sigma + 2.0,
        "{broken} of {n}"
    );
}

/// Thins a regiment, then returns it to Rallied and counts who comes back.
fn rally_return(seed: u64) -> (usize, usize, BattleState) {
    let mut b = duel(seed, 400, None);
    rout(&mut b, 60);
    let absent = rm(&b, 0).absent.len();
    assert!(absent > 60, "{absent}");
    // Recovered: FallingBack with the exit conditions met enters Rallied on the next period.
    let min = b.tables.morale.globals.min_fall;
    {
        let r = rm_mut(&mut b, 0);
        r.kind = MoraleKind::FallingBack;
        r.c = 900_000;
        r.state_ticks = min + 100;
    }
    b.regiments[0].dirty = false;
    period(&mut b, Some(false));
    assert_eq!(rm(&b, 0).kind, MoraleKind::Rallied);
    let back = absent - rm(&b, 0).absent.len();
    (absent, back, b)
}

#[test]
fn returners_on_rally_are_near_return_pm() {
    let (mut n, mut k) = (0usize, 0usize);
    for seed in 1..=6 {
        let (a, back, _) = rally_return(seed);
        n += a;
        k += back;
    }
    let p = 650.0 / 1000.0;
    let mean = n as f64 * p;
    let sigma = (n as f64 * p * (1.0 - p)).sqrt();
    assert!(
        (k as f64 - mean).abs() < 3.0 * sigma,
        "{k} of {n} returned, expected {mean:.0} +- {sigma:.0}"
    );
}

#[test]
fn a_returner_is_back_in_the_roster_in_order() {
    let (absent, back, b) = rally_return(3);
    assert!(back > 0 && back < absent);
    assert_eq!(b.regiments[0].order, Order::Hold);
    assert!(b.regiments[0].dirty);
    let roster = &b.regiments[0].roster;
    assert!(roster.windows(2).all(|w| w[0] < w[1]));
    for a in roster {
        assert_eq!(b.world.agents.flags(*a).unwrap() & F_SKULK, 0);
        assert!(!rm(&b, 0).absent.contains(a));
    }
    // Everyone who is still away still carries the flag.
    for a in &rm(&b, 0).absent {
        assert_ne!(b.world.agents.flags(*a).unwrap() & F_SKULK, 0);
    }
}

#[test]
fn the_period_draws_one_roll_one_word_per_soldier_and_one_per_absentee_on_rally() {
    let mut b = duel(4, 400, None);
    rout(&mut b, 40);
    let min = b.tables.morale.globals.min_fall;
    {
        let r = rm_mut(&mut b, 0);
        r.kind = MoraleKind::FallingBack;
        r.c = 900_000;
        r.state_ticks = min + 100;
    }
    let roster = b.regiments[0].roster.len();
    let absent = rm(&b, 0).absent.len();
    let mut want = Rng::from_state(rm(&b, 0).rng.state());
    want.next_u32();
    for _ in 0..roster {
        want.next_u64();
    }
    for _ in 0..absent {
        want.next_u32();
    }
    tick(&mut b, Some(false));
    assert_eq!(rm(&b, 0).kind, MoraleKind::Rallied);
    assert_eq!(rm(&b, 0).rng.state(), want.state());
}

#[test]
fn a_killed_absentee_is_dropped_and_costs_no_shock() {
    let mut b = duel(6, 400, None);
    rout(&mut b, 30);
    let gone = rm(&b, 0).absent[0];
    // Off a slow tick, so only the casualty ingestion could change cohesion.
    b.world.advance_tick();
    let c = rm(&b, 0).c;
    let rec = cas(&b, gone.index, CasualtyKind::Killed);
    b.casualties.push(rec);
    b.step_morale_hooked(false, Some(false));
    assert_eq!(rm(&b, 0).c, c);
    b.world.advance_tick();
    b.world.agents.despawn(gone);
    while !b.world.tick.is_multiple_of(20) {
        tick(&mut b, Some(false));
    }
    assert!(
        rm(&b, 0).absent.contains(&gone),
        "dropped only in a slow period"
    );
    tick(&mut b, Some(false));
    assert!(!rm(&b, 0).absent.contains(&gone));
}

#[test]
fn losing_the_colonel_shocks_like_ten_kills() {
    let mut a = duel(2, 400, None);
    let mut b = duel(2, 400, None);
    let colonel = rm(&a, 0).colonel;
    let plain: Vec<u32> = a.regiments[0]
        .roster
        .iter()
        .filter(|x| !rm(&a, 0).officers.contains(x))
        .take(10)
        .map(|x| x.index)
        .collect();
    let c0 = rm(&a, 0).c;
    let rec = cas(&a, colonel.index, CasualtyKind::Killed);
    a.casualties.push(rec);
    for i in plain {
        let rec = cas(&b, i, CasualtyKind::Killed);
        b.casualties.push(rec);
    }
    // Off a slow tick, so only the casualty ingestion changes cohesion.
    for s in [&mut a, &mut b] {
        s.world.advance_tick();
        s.step_morale_hooked(false, Some(false));
    }
    assert_eq!(rm(&a, 0).c, rm(&b, 0).c);
    assert!(rm(&a, 0).c < c0);
    assert!(!rm(&a, 0).officers.contains(&colonel));
}
