//! The `fid_mixed` scenario (SPEC-008 sections 5, 11; REGISTRY section 8): six regiments stepped
//! by `BattleState::step` with class changes forced at ticks 100, 300, 500 and 900. The ledger
//! identities hold at every tick, the forced classes are reached, and two runs agree.

mod t084_support;

use battle_rules::scenarios::fidelity::{fid_mixed, step_fid_mixed, FID_MIXED_PLAN};
use battle_rules::BattleState;
use t084_support::tables;

fn check(b: &BattleState) {
    for team in 0..2u8 {
        let l = b.ledger(team);
        assert_eq!(l.present + l.killed, 180, "soldiers of team {team}");
        assert_eq!(l.held + l.spent + l.lost, l.initial + l.resupplied);
    }
}

fn run(seed: u64, ticks: u64) -> BattleState {
    let mut b = fid_mixed(&tables(), seed);
    check(&b);
    for _ in 0..ticks {
        step_fid_mixed(&mut b);
        check(&b);
    }
    b
}

#[test]
fn the_plan_is_ordered_and_names_six_regiments() {
    assert!(FID_MIXED_PLAN.windows(2).all(|w| w[0].0 <= w[1].0));
    assert!(FID_MIXED_PLAN.iter().all(|p| p.1 < 6 && p.2 <= 2));
    assert_eq!(FID_MIXED_PLAN.iter().map(|p| p.0).max(), Some(900));
}

#[test]
fn forced_classes_are_reached_and_the_ledger_holds() {
    let b = run(3, 1400);
    let classes: Vec<u8> = b.regiments.iter().map(|r| r.class.0).collect();
    assert_eq!(classes, [1, 2, 0, 2, 1, 1]);
    assert!(b.soldier_serial > 0);
    assert!(b.casualties().len() < 360);
}

#[test]
fn two_runs_hash_alike_and_the_seed_matters() {
    assert_eq!(run(3, 600).hash(), run(3, 600).hash());
    assert_ne!(run(3, 600).hash(), run(4, 600).hash());
}
