//! T10-31 (message half, unit level on a scripted port): one in-flight order per recipient, an
//! overdue order resent unchanged at `ack_timeout_ticks` at most `max_resends` times and then
//! escalated as `NoReply`, think delay, and no use of a report before its delivery.

mod support;

use battle_ai::{
    Contact, Decision, Difficulty, ObjectiveBody, ObjectiveKind, OrderBody, OwnReport, PlanCx,
    PlanKind, Planner, ReasonCode, SeatLevel, UnitRef,
};
use battle_rules::{Formation, Order};
use core_math::{Angle, Fx, Rng, Vec2Fx};
use support::{ai, run, seat, tables, Draws, Once, ScriptPort};

const TIMEOUT: u64 = 1200;
const RESENDS: usize = 2;

fn hold(reason: ReasonCode) -> Decision {
    Decision {
        to: UnitRef(5),
        body: OrderBody::Reg(Order::Hold),
        plan: PlanKind::Place,
        reason,
        v0: 0,
        v1: 0,
    }
}

fn tac_ai(d: Difficulty, commander: &str, planner: Box<dyn Planner>) -> battle_ai::Ai {
    let mut a = ai(
        0,
        d,
        &[
            seat(0, SeatLevel::Operational, None, ""),
            seat(1, SeatLevel::Tactical, Some(0), commander),
        ],
        7,
    );
    a.set_planners(Box::new(Once(None)), planner);
    a
}

#[test]
fn think_delay_by_level_and_reaction() {
    // Seat 1 first fires when (tick + 7) % interval == 0: tick 93 on standard (interval 100).
    for (d, think, fires) in [
        (Difficulty::Easy, 100, 193),
        (Difficulty::Standard, 40, 93),
        (Difficulty::Hard, 20, 53),
    ] {
        let mut a = tac_ai(d, "", Box::new(Once(Some(hold(ReasonCode::Assigned)))));
        let mut p = ScriptPort::new(10);
        run(&mut a, &mut p, 400);
        assert_eq!(p.log.len(), 1, "{d:?}");
        assert_eq!(p.log[0].tick, fires + think, "{d:?}");
        assert_eq!(
            a.notes()[0].tick,
            fires,
            "the note is made at decision time"
        );
    }
}

#[test]
fn reaction_ticks_of_the_commander_add_to_the_think_delay() {
    let reaction =
        u64::from(battle_ai::personality_for("george_g_meade", &tables()).reaction_ticks);
    assert_eq!(reaction, 8);
    let mut a = tac_ai(
        Difficulty::Standard,
        "george_g_meade",
        Box::new(Once(Some(hold(ReasonCode::Assigned)))),
    );
    let mut p = ScriptPort::new(10);
    run(&mut a, &mut p, 400);
    assert_eq!(p.log[0].tick, 93 + 40 + reaction);
}

#[test]
fn a_lost_order_is_resent_at_the_timeout_then_escalated() {
    let mut a = tac_ai(
        Difficulty::Standard,
        "",
        Box::new(Once(Some(hold(ReasonCode::Assigned)))),
    );
    let mut p = ScriptPort::new(10).losing_all();
    run(&mut a, &mut p, 10_000);
    let ticks: Vec<u64> = p.log.iter().map(|s| s.tick).collect();
    let first = 93 + 40;
    assert_eq!(ticks, vec![first, first + TIMEOUT, first + 2 * TIMEOUT]);
    assert_eq!(ticks.len(), 1 + RESENDS);
    // Resent unchanged.
    assert!(p.log.iter().all(|s| s.msg == p.log[0].msg));
    // Escalated to the parent seat once, at the next timeout.
    let esc: Vec<_> = a
        .notes()
        .iter()
        .filter(|n| n.reason == ReasonCode::NoReply)
        .collect();
    assert_eq!(esc.len(), 1);
    assert_eq!(esc[0].tick, first + 3 * TIMEOUT);
    assert_eq!(esc[0].seat.0, 0, "the parent seat");
    assert_eq!(esc[0].unit, UnitRef(5));
    assert_eq!(esc[0].plan, PlanKind::Escalate);
    // Two resend notes.
    let n_resend = a
        .notes()
        .iter()
        .filter(|n| n.reason == ReasonCode::Resent)
        .count();
    assert_eq!(n_resend, RESENDS);
}

#[test]
fn an_acknowledged_order_is_not_resent() {
    let mut a = tac_ai(
        Difficulty::Standard,
        "",
        Box::new(Once(Some(hold(ReasonCode::Assigned)))),
    );
    let mut p = ScriptPort::new(300);
    run(&mut a, &mut p, 10_000);
    assert_eq!(p.log.len(), 1);
    assert!(a.notes().iter().all(|n| n.reason != ReasonCode::NoReply));
}

/// For every recipient: a later submission follows the earlier one only after its
/// acknowledgement or after the timeout; an identical body is a resend (at most `max_resends`).
fn check_discipline(p: &ScriptPort) {
    for unit in 0..8u16 {
        let sent: Vec<_> = p.log.iter().filter(|s| s.msg.to == UnitRef(unit)).collect();
        let mut run_len = 0;
        for (i, s) in sent.iter().enumerate() {
            if i == 0 {
                run_len = 0;
                continue;
            }
            let prev = sent[i - 1];
            let acked = !prev.lost && s.tick >= prev.tick + p.ack_delay;
            let overdue = s.tick >= prev.tick + TIMEOUT;
            assert!(
                acked || overdue,
                "unit {unit}: second order in flight at {}",
                s.tick
            );
            if s.msg == prev.msg {
                assert!(overdue, "a resend comes only at the timeout");
                run_len += 1;
                assert!(run_len <= RESENDS, "more than max_resends resends");
            } else {
                run_len = 0;
            }
        }
    }
}

#[test]
fn one_in_flight_order_per_recipient_under_loss() {
    for seed in 1..=4u64 {
        let mut a = tac_ai(Difficulty::Hard, "", Box::new(Draws { unit: 5, n: 0 }));
        let mut rng = Rng::from_seed(seed);
        let mut p = ScriptPort::new(35);
        p.lose = Box::new(move |_| rng.coin(3, 10));
        run(&mut a, &mut p, 12_000);
        assert!(p.log.len() > 20, "the scenario must send orders");
        assert!(p.log.iter().any(|s| s.lost));
        check_discipline(&p);
    }
}

#[test]
fn a_newer_order_waits_for_the_ack_or_the_timeout() {
    // Seat 1 decides every 60 ticks on hard; the first order is lost, so the replacement may go
    // only once the timeout has run (as a replacement, not a resend).
    let mut a = tac_ai(Difficulty::Hard, "", Box::new(Draws { unit: 5, n: 0 }));
    let mut p = ScriptPort::new(10);
    p.lose = Box::new(|i| i == 0);
    run(&mut a, &mut p, 2000);
    assert_eq!(p.log[0].tick, 53 + 20);
    assert_eq!(p.log[1].tick, 53 + 20 + TIMEOUT);
    assert_ne!(
        p.log[1].msg, p.log[0].msg,
        "the newest decision replaces it"
    );
}

#[test]
fn escalation_frees_the_recipient() {
    let mut a = tac_ai(Difficulty::Standard, "", Box::new(Draws { unit: 5, n: 0 }));
    let mut p = ScriptPort::new(10).losing_all();
    run(&mut a, &mut p, 6000);
    // Replacements follow each timeout; nothing is ever sent twice within one timeout.
    let t: Vec<u64> = p.log.iter().map(|s| s.tick).collect();
    assert!(t.windows(2).all(|w| w[1] - w[0] >= TIMEOUT));
}

#[test]
fn an_unchanged_objective_is_not_resent() {
    struct Same;
    impl Planner for Same {
        fn plan(&mut self, _cx: &mut PlanCx<'_>) -> Vec<Decision> {
            vec![Decision {
                to: UnitRef(3),
                body: OrderBody::Objective(ObjectiveBody {
                    kind: ObjectiveKind::Hold,
                    center: Vec2Fx::new(Fx::from_int(10), Fx::from_int(20)),
                    radius_mm: 50_000,
                    deadline_tick: 6000,
                    priority: 3,
                    discretionary: false,
                }),
                plan: PlanKind::Stay,
                reason: ReasonCode::Assigned,
                v0: 1,
                v1: 2,
            }]
        }
    }
    let mut a = tac_ai(Difficulty::Hard, "", Box::new(Same));
    let mut p = ScriptPort::new(5);
    run(&mut a, &mut p, 3000);
    assert_eq!(p.log.len(), 1);
    assert_eq!(
        a.notes().len(),
        1,
        "no note for a decision that sends nothing new"
    );
}

#[test]
fn no_decision_uses_a_report_before_its_delivery() {
    /// Records what each pass could see.
    struct Peek(std::rc::Rc<std::cell::RefCell<Vec<(u64, usize, usize)>>>);
    impl Planner for Peek {
        fn plan(&mut self, cx: &mut PlanCx<'_>) -> Vec<Decision> {
            self.0
                .borrow_mut()
                .push((cx.tick, cx.reports.len(), cx.contacts.len()));
            Vec::new()
        }
    }
    let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let mut a = tac_ai(Difficulty::Hard, "", Box::new(Peek(seen.clone())));
    let mut p = ScriptPort::new(5);
    for t in 0..=1000u64 {
        p.now = t;
        if t == 500 {
            p.reports.push(OwnReport {
                unit: UnitRef(5),
                pos: Vec2Fx::ZERO,
                heading: Angle::default(),
                formation: Formation::Line,
                strength: 400,
                strength_ref: 400,
                rounds_pm: 1000,
                fatigue_pm: 0,
                cohesion_pm: None,
                age_ticks: 30,
            });
            p.contacts.push(Contact {
                id: battle_ai::ContactId(1),
                pos: Vec2Fx::ZERO,
                strength_est: 300,
                formation_est: None,
                weapon_est: None,
                last_seen: 400,
                quality_pm: 600,
            });
        }
        a.step(&mut p);
    }
    let seen = seen.borrow();
    assert!(seen.iter().any(|&(t, _, _)| t < 500));
    for &(t, reports, contacts) in seen.iter() {
        let expect = usize::from(t >= 500);
        assert_eq!((reports, contacts), (expect, expect), "tick {t}");
    }
}
