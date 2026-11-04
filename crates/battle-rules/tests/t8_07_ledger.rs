//! T8-07 (soldiers) and T8-08 (ammunition): the ledger identities hold at every tick of a
//! hand-built six-regiment battle stepped by `BattleState::step` while regiments are forced
//! through class changes at ticks 100, 300, 500 and 900 (SPEC-008 section 5).

mod t083_support;

use battle_rules::{
    condense, hand_over, materialise, BattleState, FidelityClass, HitOutcome, RegimentId,
    RegimentStatus, F0, F1, F2,
};
use t083_support::{hit, member, six};

const MEN: u64 = 180;

fn check(b: &BattleState, what: &str) {
    for team in 0..2u8 {
        let l = b.ledger(team);
        assert_eq!(l.present + l.killed, MEN, "{what}: soldiers of team {team}");
        assert_eq!(
            l.held + l.spent + l.lost,
            l.initial + l.resupplied,
            "{what}: rounds of team {team}: {l:?}"
        );
    }
}

fn r(i: u16) -> RegimentId {
    RegimentId(i)
}

/// The forced class changes of one tick, preceded by a kill, a wound and a disabling hit.
fn handovers(b: &mut BattleState, tick: u64) {
    let plan: &[(u16, u8)] = match tick {
        100 => &[(1, 2), (4, 2)],
        300 => &[(0, 2), (1, 0), (5, 2)],
        500 => &[(0, 1), (2, 2), (4, 1)],
        900 => &[(1, 2), (2, 0), (3, 2), (5, 1)],
        _ => return,
    };
    // Casualties land on the tick of the hand-over, in the regiments about to change.
    for &(reg, _) in plan {
        let roster = b.regiments[usize::from(reg)].roster.clone();
        if let Some(&a) = roster.first() {
            hit(b, a, HitOutcome::Killed);
        }
        if let Some(&a) = roster.get(1) {
            hit(b, a, HitOutcome::Wounded);
        }
        if let Some(&a) = roster.get(2) {
            hit(b, a, HitOutcome::Disabled);
        }
    }
    b.step_casualties();
    check(b, "after the casualties of a hand-over tick");
    for &(reg, class) in plan {
        assert!(hand_over(b, r(reg), FidelityClass(class)));
        check(b, "after a hand-over");
    }
}

#[test]
fn soldiers_and_rounds_are_conserved_through_class_changes() {
    let mut b = six(3);
    check(&b, "at the start");
    for _ in 0..1000 {
        let tick = b.world.tick;
        handovers(&mut b, tick);
        if tick == 200 {
            assert!(b.resupply(r(3), 7));
        }
        b.step();
        check(&b, &format!("tick {tick}"));
    }
    assert!(b.soldier_serial > 0, "soldiers were materialised");
    assert!(
        b.casualties().len() >= 12,
        "the hand-over ticks alone kill, wound and disable"
    );
    let spent: u64 = (0..2).map(|t| b.ledger(t).spent).sum();
    assert!(spent > 0);
    for reg in &b.regiments {
        assert_eq!(reg.status, RegimentStatus::Present);
    }
}

#[test]
fn a_kill_on_a_hand_over_tick_loses_its_rounds_once() {
    let mut b = six(5);
    for _ in 0..30 {
        b.step();
    }
    let victim = member(&b, 0, 4);
    let carried = u64::from(b.fire[victim.index as usize].rounds);
    let before = b.ledger(0);
    hit(&mut b, victim, HitOutcome::Killed);
    b.step_casualties();
    let after = b.ledger(0);
    assert_eq!(after.lost, before.lost + carried);
    assert_eq!(after.killed, before.killed + 1);
    assert_eq!(after.present, before.present - 1);
    assert!(condense(&mut b, r(0)));
    let c = b.ledger(0);
    assert_eq!(
        (c.present, c.held, c.lost),
        (after.present, after.held, after.lost)
    );
    assert!(materialise(&mut b, r(0), F0));
    let d = b.ledger(0);
    assert_eq!(
        (d.present, d.held, d.lost),
        (after.present, after.held, after.lost)
    );
}

#[test]
fn re_equipping_and_resupplying_keep_the_round_account_balanced() {
    let mut b = six(9);
    for _ in 0..40 {
        b.step();
    }
    assert!(b.resupply(r(1), 5));
    check(&b, "after resupply");
    b.equip_regiment(r(1), "smoothbore_69", 0).unwrap();
    check(&b, "after re-equipping");
    assert!(hand_over(&mut b, r(1), F2));
    assert!(hand_over(&mut b, r(1), F1));
    check(&b, "after a round trip");
}

#[test]
fn classes_and_since_follow_the_hand_overs() {
    let mut b = six(1);
    assert_eq!((b.regiments[2].class, b.regiments[2].since), (F0, 0));
    for _ in 0..25 {
        b.step();
    }
    assert!(hand_over(&mut b, r(2), F1));
    assert_eq!((b.regiments[2].class, b.regiments[2].since), (F1, 25));
    assert!(!hand_over(&mut b, r(2), F1), "no change of class");
    assert!(hand_over(&mut b, r(2), F2));
    assert_eq!(b.regiments[2].class, F2);
    assert!(b.regiments[2].roster.is_empty() && b.regiments[2].slots.is_empty());
    assert!(!hand_over(&mut b, r(99), F0), "unknown regiment");
}
