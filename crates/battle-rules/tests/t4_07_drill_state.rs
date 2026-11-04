//! T4-07, state timing half: the number of ticks in `Forming` equals `transition_ticks`;
//! `can_fire` is false while forming and in column; a pending order starts when the transition
//! ends (SPEC-004 sections 6 and 7).

mod t018_support;

use battle_rules::{
    can_fire, slot_layout, transition_ticks, Formation, Gait, Order, RegimentId, RegimentState,
    Training,
};
use core_math::{Angle, Fx, Vec2Fx};
use t018_support::{battle_with, step};
use Formation::{Column, Line, Skirmish};

const PAIRS: [(Formation, Formation); 6] = [
    (Column, Line),
    (Line, Column),
    (Line, Skirmish),
    (Skirmish, Line),
    (Column, Skirmish),
    (Skirmish, Column),
];

fn march() -> Order {
    Order::Move {
        to: Vec2Fx::new(Fx::from_int(300), Fx::from_int(200)),
        gait: Gait::Quick,
    }
}

#[test]
fn ticks_in_forming_equal_transition_ticks_for_every_pair_and_level() {
    for (from, to) in PAIRS {
        for training in Training::ALL {
            let (mut b, id) = battle_with(40, training, from);
            let want = transition_ticks(from, to, training, &b.tables.drill);
            assert!(want > 0);
            assert!(b.issue_order(id, Order::Form(to)));
            let mut in_forming = 0;
            while let RegimentState::Forming { ticks_left, .. } = b.regiments[0].state {
                assert_eq!(
                    ticks_left,
                    want - in_forming,
                    "{from:?}->{to:?} {training:?}"
                );
                assert!(!b.can_fire(id), "no fire while forming");
                step(&mut b);
                in_forming += 1;
                assert!(in_forming <= want, "forming overran");
            }
            assert_eq!(in_forming, want, "{from:?}->{to:?} {training:?}");
            assert_eq!(b.regiments[0].state, RegimentState::Steady(to));
            assert_eq!(b.can_fire(id), to != Column);
        }
    }
}

#[test]
fn forming_records_both_formations() {
    let (mut b, id) = battle_with(40, Training::Trained, Column);
    b.issue_order(id, Order::Form(Line));
    assert_eq!(
        b.regiments[0].state,
        RegimentState::Forming {
            from: Column,
            to: Line,
            ticks_left: 1200
        }
    );
    // The target layout is in place at once and the anchor has not moved.
    assert_eq!(b.regiments[0].slots, slot_layout(Line, 40, &b.tables.drill));
    assert_eq!(b.regiments[0].anchor.x, Fx::from_int(100));
}

#[test]
fn can_fire_only_when_steady_in_line_or_skirmish() {
    for f in Formation::ALL {
        let (b, id) = battle_with(8, Training::Raw, f);
        assert_eq!(can_fire(&b.regiments[0]), f != Column);
        assert_eq!(b.can_fire(id), f != Column);
    }
    let (b, _) = battle_with(8, Training::Raw, Line);
    assert!(!b.can_fire(RegimentId(7)), "unknown regiment cannot fire");
}

#[test]
fn pending_order_starts_at_the_end_of_the_transition() {
    let (mut b, id) = battle_with(40, Training::Veteran, Column);
    let t = transition_ticks(Column, Line, Training::Veteran, &b.tables.drill);
    b.issue_order(id, Order::Form(Line));
    assert!(b.issue_order(id, march()));
    assert_eq!(b.regiments[0].pending, Some(march()));
    assert_eq!(b.regiments[0].order, Order::Hold, "not started yet");
    for _ in 0..t - 1 {
        step(&mut b);
        assert_eq!(b.regiments[0].order, Order::Hold);
        assert_eq!(b.regiments[0].pending, Some(march()));
    }
    step(&mut b);
    assert_eq!(b.regiments[0].state, RegimentState::Steady(Line));
    assert_eq!(b.regiments[0].order, march());
    assert_eq!(b.regiments[0].pending, None);
}

#[test]
fn a_later_pending_order_replaces_an_earlier_one() {
    let (mut b, id) = battle_with(40, Training::Trained, Column);
    b.issue_order(id, Order::Form(Line));
    b.issue_order(id, march());
    b.issue_order(id, Order::Face(Angle::QUARTER));
    assert_eq!(b.regiments[0].pending, Some(Order::Face(Angle::QUARTER)));
}

#[test]
fn hold_applies_immediately_while_forming_and_cancels_the_pending_order() {
    let (mut b, id) = battle_with(40, Training::Trained, Column);
    b.issue_order(id, Order::Form(Line));
    b.issue_order(id, march());
    b.issue_order(id, Order::Hold);
    assert_eq!(b.regiments[0].order, Order::Hold);
    assert_eq!(b.regiments[0].pending, None);
    assert!(matches!(
        b.regiments[0].state,
        RegimentState::Forming { .. }
    ));
}

#[test]
fn pending_form_starts_a_second_transition_at_once() {
    let (mut b, id) = battle_with(40, Training::Trained, Column);
    b.issue_order(id, Order::Form(Line));
    b.issue_order(id, Order::Form(Skirmish));
    let first = transition_ticks(Column, Line, Training::Trained, &b.tables.drill);
    let second = transition_ticks(Line, Skirmish, Training::Trained, &b.tables.drill);
    for _ in 0..first {
        step(&mut b);
    }
    assert_eq!(
        b.regiments[0].state,
        RegimentState::Forming {
            from: Line,
            to: Skirmish,
            ticks_left: second
        }
    );
    assert_eq!(b.regiments[0].pending, None);
    assert_eq!(
        b.regiments[0].slots,
        slot_layout(Skirmish, 40, &b.tables.drill)
    );
}

#[test]
fn form_of_the_current_formation_is_a_no_op() {
    let (mut b, id) = battle_with(40, Training::Trained, Line);
    b.issue_order(id, march());
    let before = b.regiments[0].clone();
    b.issue_order(id, Order::Form(Line));
    assert_eq!(b.regiments[0], before, "the standing move is kept");
}

#[test]
fn steady_regiment_takes_move_and_face_orders_and_form_replaces_a_move() {
    let (mut b, id) = battle_with(40, Training::Trained, Column);
    b.issue_order(id, march());
    assert_eq!(b.regiments[0].order, march());
    b.issue_order(id, Order::Face(Angle::HALF));
    assert_eq!(b.regiments[0].order, Order::Face(Angle::HALF));
    b.issue_order(id, march());
    b.issue_order(id, Order::Form(Line));
    assert_eq!(b.regiments[0].order, Order::Hold);
    assert!(
        !b.issue_order(RegimentId(3), Order::Hold),
        "unknown regiment"
    );
}

#[test]
fn forming_gives_every_soldier_a_distinct_slot_of_the_target_layout() {
    let (mut b, id) = battle_with(400, Training::Raw, Column);
    b.issue_order(id, Order::Form(Line));
    let mut got: Vec<(i64, i64)> = b.regiments[0]
        .roster
        .iter()
        .map(|a| {
            let s = b.loco[a.index as usize].slot;
            (s.u.raw(), s.v.raw())
        })
        .collect();
    got.sort_unstable();
    let mut want: Vec<(i64, i64)> = b.regiments[0]
        .slots
        .iter()
        .map(|s| (s.u.raw(), s.v.raw()))
        .collect();
    want.sort_unstable();
    assert_eq!(got, want);
    want.dedup();
    assert_eq!(want.len(), 400);
}
