//! T6-11, fire interplay (SPEC-006 section 5.8, SPEC-004 amendment A7, SPEC-005 amendment A11):
//! the regiment gate, the refusal of orders, loading scaled by `fire_pm`, skulkers that never
//! fire and the exact `shake_pm` factor of `sigma_s`.

mod t64_support;
mod t6_support;

use battle_rules::aim::{sigma_urad, sigma_urad_shaken};
use battle_rules::morale::{symptom_values, MoraleKind, F_SKULK};
use battle_rules::scenarios::fire::cal_line;
use battle_rules::{BattleState, Formation, Gait, Order, RegimentId, Tables, Training};
use t64_support::{duel, rm_mut, set};
use t6_support::{m, RULES_DIR};

const R0: RegimentId = RegimentId(0);

fn all_kinds() -> [MoraleKind; 5] {
    MoraleKind::ALL
}

fn allowed(k: MoraleKind) -> bool {
    matches!(
        k,
        MoraleKind::Steady | MoraleKind::Wavering | MoraleKind::Rallied
    )
}

#[test]
fn can_fire_needs_a_steady_enough_state() {
    let mut b = duel(1, 100, None);
    for k in all_kinds() {
        set(&mut b, 0, 500_000, k);
        assert_eq!(b.can_fire(R0), allowed(k), "{k:?}");
    }
    // The drill gate still applies.
    set(&mut b, 0, 1_000_000, MoraleKind::Steady);
    assert!(b.issue_order(R0, Order::Form(Formation::Column)));
    assert!(!b.can_fire(R0));
    assert!(!b.can_fire(RegimentId(9)));
}

#[test]
fn issue_order_is_refused_while_falling_back_or_broken() {
    for k in all_kinds() {
        let mut b = duel(1, 100, None);
        set(&mut b, 0, 500_000, k);
        let order = Order::Move {
            to: m(300, 100),
            gait: Gait::Walk,
        };
        let took = b.issue_order(R0, order);
        assert_eq!(took, allowed(k), "{k:?}");
        assert_eq!(b.regiments[0].order == order, allowed(k), "{k:?}");
        let held = b.issue_order(R0, Order::Hold);
        assert_eq!(held, allowed(k), "hold in {k:?}");
    }
}

#[test]
fn morale_off_changes_neither_gate() {
    let mut b = duel(1, 100, None);
    b.morale = None;
    assert!(b.can_fire(R0));
    assert!(b.issue_order(R0, Order::Hold));
    assert_eq!(b.fire_pm(R0), 1000);
    assert_eq!(b.shake_pm(R0), 1000);
}

#[test]
fn symptom_rows_and_the_wavering_ramp() {
    let t = Tables::load_dir(RULES_DIR).unwrap();
    let g = &t.morale.globals;
    let at = |k, c| symptom_values(&t.morale, k, c);
    let v = at(MoraleKind::Steady, 900_000);
    assert_eq!(
        (v.fire_pm, v.shake_pm, v.look_pm, v.skulk_ppm),
        (1000, 1000, 10, 0)
    );
    let v = at(MoraleKind::Wavering, g.steady);
    assert_eq!(
        (v.fire_pm, v.shake_pm, v.look_pm, v.skulk_ppm),
        (1000, 1000, 80, 0)
    );
    let v = at(MoraleKind::Wavering, g.fall);
    assert_eq!(
        (v.fire_pm, v.shake_pm, v.look_pm, v.skulk_ppm),
        (550, 1500, 300, 400)
    );
    let mid = (g.steady + g.fall) / 2;
    let v = at(MoraleKind::Wavering, mid);
    assert_eq!(v.fire_pm, 775);
    assert_eq!(v.shake_pm, 1250);
    assert_eq!(v.look_pm, 190);
    assert_eq!(at(MoraleKind::Wavering, 500_000).skulk_ppm, 0);
    assert_eq!(at(MoraleKind::Wavering, 600_000).skulk_ppm, 0);
    let low = at(MoraleKind::Wavering, (500_000 + g.fall) / 2).skulk_ppm;
    assert_eq!(low, 200);
    let v = at(MoraleKind::FallingBack, 0);
    assert_eq!(
        (v.fire_pm, v.shake_pm, v.look_pm, v.skulk_ppm),
        (0, 1500, 400, 1500)
    );
    let v = at(MoraleKind::Broken, 0);
    assert_eq!(
        (v.fire_pm, v.shake_pm, v.look_pm, v.skulk_ppm),
        (0, 1500, 800, 6000)
    );
    let v = at(MoraleKind::Rallied, 500_000);
    assert_eq!(
        (v.fire_pm, v.shake_pm, v.look_pm, v.skulk_ppm),
        (800, 1250, 120, 0)
    );
}

#[test]
fn battle_fire_pm_and_shake_pm_follow_the_state() {
    let mut b = duel(1, 100, None);
    for k in all_kinds() {
        set(&mut b, 0, 500_000, k);
        let v = symptom_values(&b.tables.morale, k, 500_000);
        assert_eq!(
            (u32::from(b.fire_pm(R0)), u32::from(b.shake_pm(R0))),
            (v.fire_pm, v.shake_pm)
        );
    }
    assert_eq!(b.fire_pm(RegimentId(9)), 1000);
}

#[test]
fn sigma_s_changes_by_exactly_shake_pm() {
    let t = Tables::load_dir(RULES_DIR).unwrap();
    let mut w = t.weapons.rows()[t.weapons.index_of("rifle_58").unwrap()].clone();
    // With no ballistic term the total is the shooter's term alone.
    w.sigma0_urad = 0;
    w.sigma_slope_urad_per_100m = 0;
    // Keep the shooter's term clear of the saturation at 46,340 micro-radians.
    w.aim_scale_pm = 1000;
    for training in [Training::Raw, Training::Trained, Training::Veteran] {
        for (fatigue, smoke) in [(0u32, 0u32), (500_000, 300)] {
            let plain = sigma_urad(&w, training, fatigue, smoke, 100_000);
            assert_eq!(
                sigma_urad_shaken(&w, training, fatigue, smoke, 1000, 100_000),
                plain
            );
            for shake in [550u32, 1000, 1250, 1500] {
                let ss = i64::from(battle_rules::aim::aim_sigma_urad(training));
                let ss = ss * i64::from(w.aim_scale_pm) / 1000;
                let ss = ss * (1000 + 600 * i64::from(fatigue) / 1_000_000) / 1000;
                let ss = ss * (1000 + 4 * i64::from(smoke)) / 1000;
                let ss = ss * i64::from(shake) / 1000;
                let sum = (ss * ss).min(i64::from(i32::MAX));
                let want = core_math::Fx::from_int(sum as i32).sqrt();
                let got = sigma_urad_shaken(&w, training, fatigue, smoke, shake, 100_000);
                assert_eq!(got, want, "{training:?} {fatigue} {smoke} {shake}");
            }
        }
    }
}

fn pin(b: &mut BattleState, kinds: [(MoraleKind, u32); 2]) {
    for (i, (k, c)) in kinds.into_iter().enumerate() {
        let r = rm_mut(b, i);
        r.kind = k;
        r.c = c;
        r.state_ticks = 0;
    }
}

/// Runs `ticks` of a pinned `cal_line`; returns shots fired and soldier-ticks (roster sizes
/// summed over the run).
fn pinned_run(
    seed: u64,
    ticks: u32,
    kinds: Option<[(MoraleKind, u32); 2]>,
) -> (u64, u64, BattleState) {
    let t = Tables::load_dir(RULES_DIR).unwrap();
    let mut b = cal_line(&t, seed, "rifle_58", Training::Trained, 91_440, false);
    if let Some(k) = kinds {
        b.enable_morale();
        pin(&mut b, k);
    }
    let mut roster_ticks = 0u64;
    for _ in 0..ticks {
        if let Some(k) = kinds {
            pin(&mut b, k);
        }
        b.step();
        roster_ticks += b
            .regiments
            .iter()
            .map(|r| r.roster.len() as u64)
            .sum::<u64>();
    }
    (b.counters().shots_fired, roster_ticks, b)
}

#[test]
fn steady_morale_changes_no_shot() {
    let steady = [(MoraleKind::Steady, 1_000_000); 2];
    let (_, _, off) = pinned_run(1, 600, None);
    let (_, _, on) = pinned_run(1, 600, Some(steady));
    assert_eq!(off.counters(), on.counters());
    assert_eq!(off.pool.len(), on.pool.len());
    assert_eq!(off.casualties, on.casualties);
    for (a, b) in off.fire.iter().zip(on.fire.iter()) {
        assert_eq!(a, b);
    }
}

#[test]
fn shots_per_soldier_scale_with_fire_pm() {
    let ticks = 6000;
    let (s1, r1, _) = pinned_run(2, ticks, Some([(MoraleKind::Steady, 1_000_000); 2]));
    let (s2, r2, _) = pinned_run(2, ticks, Some([(MoraleKind::Rallied, 700_000); 2]));
    assert!(s1 > 500, "{s1}");
    let rate1 = (s1 * 1_000_000 / r1) as i64;
    let rate2 = (s2 * 1_000_000 / r2) as i64;
    let want = rate1 * 800 / 1000;
    assert!(
        (rate2 - want).abs() * 100 <= want * 3,
        "rates {rate1} and {rate2}, wanted about {want}"
    );
}

#[test]
fn a_regiment_that_cannot_fire_loads_nothing_and_shoots_nothing() {
    for dead in [MoraleKind::FallingBack, MoraleKind::Broken] {
        let t = Tables::load_dir(RULES_DIR).unwrap();
        let mut b = cal_line(&t, 3, "rifle_58", Training::Trained, 91_440, false);
        b.enable_morale();
        let before: Vec<_> = b.regiments[0]
            .roster
            .iter()
            .map(|a| {
                (
                    *a,
                    b.fire[a.index as usize].step,
                    b.fire[a.index as usize].left,
                )
            })
            .collect();
        for _ in 0..400 {
            pin(&mut b, [(dead, 0), (MoraleKind::Steady, 1_000_000)]);
            b.step();
        }
        assert_eq!(b.regiments[0].acct.spent, 0, "{dead:?}");
        assert!(b.regiments[1].acct.spent > 0);
        for (a, step, left) in before {
            let f = &b.fire[a.index as usize];
            assert_eq!((f.step, f.left), (step, left), "{dead:?}");
        }
    }
}

#[test]
fn skulkers_never_fire() {
    let t = Tables::load_dir(RULES_DIR).unwrap();
    let mut b = cal_line(&t, 4, "rifle_58", Training::Trained, 91_440, false);
    let marked: Vec<_> = b.regiments[0].roster.iter().take(60).copied().collect();
    for a in &marked {
        let f = b.world.agents.flags(*a).unwrap();
        b.world.agents.set_flags(*a, f | F_SKULK);
    }
    let rounds: Vec<u16> = marked
        .iter()
        .map(|a| b.fire[a.index as usize].rounds)
        .collect();
    // The fire phase alone, so the roster still lists them.
    for _ in 0..1500 {
        b.step_fire();
        b.world.advance_tick();
    }
    assert!(b.counters().shots_fired > 50);
    for (a, r) in marked.iter().zip(rounds) {
        assert_eq!(b.fire[a.index as usize].rounds, r);
    }
    let others = b.regiments[0]
        .roster
        .iter()
        .filter(|a| !marked.contains(a))
        .filter(|a| b.fire[a.index as usize].rounds < 60)
        .count();
    assert!(others > 0, "the others did fire");
}
