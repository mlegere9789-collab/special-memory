//! T15-05 order status (checklist item 11): every status, the boundaries at `eta_hi` and the
//! grace after it, the misread flag after completion, `Lost` only when known, a late ack, and a
//! scripted fixture (delay, loss, misread) giving the status of every tick.

use battle_rules::{Formation, Order, RegimentId};
use std::collections::BTreeSet;
use ui_model::{derive_status, order_view, OrderRecord, OrderStatus, StatusParams};
use OrderStatus::*;

const TOML: &str = include_str!("../../../data/ui/status.toml");

fn params() -> StatusParams {
    StatusParams::from_toml_str(TOML).expect("status.toml loads")
}

/// Sent at 100, acknowledgement due by 300, nothing heard.
fn rec() -> OrderRecord {
    OrderRecord {
        id: 1,
        regiment: RegimentId(3),
        order: Order::Hold,
        sent_tick: Some(100),
        eta_lo: 150,
        eta_hi: 300,
        eta_hi0: 300,
        ack_tick: None,
        interpreted: None,
        executed_tick: None,
        completed_tick: None,
        lost_known: false,
    }
}

fn status(r: &OrderRecord, now: u64) -> OrderStatus {
    derive_status(r, now, &params())
}

#[test]
fn the_grace_is_the_command_layer_value() {
    assert_eq!(params().lost_grace_ticks, 2400);
    assert!(params().provenance.placeholder);
    assert_eq!(params().provenance.calibration_target, Some(2));
}

#[test]
fn drafted_until_sent() {
    let r = OrderRecord {
        sent_tick: None,
        ..rec()
    };
    for now in [0, 150, 301, 100_000] {
        assert_eq!(status(&r, now), Drafted);
    }
}

#[test]
fn boundaries_at_the_deadline_and_the_grace() {
    let g = params().lost_grace_ticks as u64;
    let r = rec();
    assert_eq!(status(&r, 100), InTransit);
    assert_eq!(status(&r, 299), InTransit);
    assert_eq!(status(&r, 300), InTransit);
    assert_eq!(status(&r, 301), Overdue);
    assert_eq!(status(&r, 300 + g), Overdue);
    assert_eq!(status(&r, 300 + g + 1), PresumedLost);
    assert_eq!(status(&r, u64::MAX), PresumedLost);
}

#[test]
fn a_learned_delay_shows_by_how_much() {
    let r = OrderRecord {
        eta_hi: 350,
        ..rec()
    };
    assert_eq!(status(&r, 200), Delayed { by_ticks: 50 });
    assert_eq!(status(&r, 350), Delayed { by_ticks: 50 });
    assert_eq!(status(&r, 351), Overdue);
    assert_eq!(status(&r, 350 + 2400 + 1), PresumedLost);
}

#[test]
fn acknowledgement_states() {
    let acked = OrderRecord {
        ack_tick: Some(280),
        interpreted: Some(Order::Hold),
        ..rec()
    };
    assert_eq!(status(&acked, 290), Received);
    let executing = OrderRecord {
        executed_tick: Some(285),
        ..acked.clone()
    };
    assert_eq!(status(&executing, 290), Executing);
    let done = OrderRecord {
        completed_tick: Some(400),
        ..executing.clone()
    };
    assert_eq!(status(&done, 500), Completed);
    let misread = OrderRecord {
        interpreted: Some(Order::Form(Formation::Column)),
        ..executing
    };
    assert_eq!(status(&misread, 290), Misread);
    let no_reading = OrderRecord {
        interpreted: None,
        ..acked
    };
    assert_eq!(status(&no_reading, 290), Received);
}

#[test]
fn lost_shows_only_when_known() {
    let r = rec();
    assert_ne!(status(&r, u64::MAX), Lost);
    let known = OrderRecord {
        lost_known: true,
        ..r
    };
    for now in [0, 150, 301, 5000] {
        assert_eq!(status(&known, now), Lost);
    }
    // completed outranks it; sent-never orders are not lost either way
    let done = OrderRecord {
        completed_tick: Some(1),
        ..known
    };
    assert_eq!(status(&done, 2), Completed);
}

#[test]
fn a_late_ack_replaces_overdue_and_presumed_lost() {
    let mut r = rec();
    assert_eq!(status(&r, 5000), PresumedLost);
    r.ack_tick = Some(5000);
    r.interpreted = Some(Order::Hold);
    assert_eq!(status(&r, 5000), Received);
}

#[test]
fn the_misread_flag_persists_after_completion() {
    let p = params();
    let mut r = OrderRecord {
        ack_tick: Some(280),
        interpreted: Some(Order::Form(Formation::Line)),
        ..rec()
    };
    assert!(order_view(&r, 290, &p).misread);
    r.completed_tick = Some(400);
    let v = order_view(&r, 500, &p);
    assert_eq!((v.status, v.misread), (Completed, true));
    r.interpreted = Some(Order::Hold);
    let v = order_view(&r, 500, &p);
    assert_eq!((v.status, v.misread), (Completed, false));
    r.interpreted = None;
    assert!(!order_view(&r, 500, &p).misread);
}

#[test]
fn every_status_has_its_own_icon() {
    let all = [
        Drafted,
        InTransit,
        Delayed { by_ticks: 7 },
        Overdue,
        PresumedLost,
        Received,
        Executing,
        Completed,
        Misread,
        Lost,
    ];
    let icons: BTreeSet<u8> = all.iter().map(|s| s.icon()).collect();
    assert_eq!(icons.len(), all.len());
    assert_eq!(
        Delayed { by_ticks: 1 }.icon(),
        Delayed { by_ticks: 9 }.icon()
    );
}

#[test]
fn loader_refuses_bad_files() {
    assert!(StatusParams::from_toml_str(&TOML.replace("2400", "2400.5")).is_err());
    assert!(StatusParams::from_toml_str(&TOML.replace("2400", "-1")).is_err());
    assert!(
        StatusParams::from_toml_str(&TOML.replace("calibration_target = \"CT-015-2\"\n", ""))
            .is_err()
    );
    assert!(StatusParams::from_toml_str(&TOML.replace("ui-status/1", "ui-status/9")).is_err());
    assert!(StatusParams::from_toml_str(&format!("{TOML}\nextra = 1\n")).is_err());
}

/// The fixture: the record of one order as headquarters learns of it, `(tick, change)` events.
fn timeline(kind: char, now: u64) -> OrderRecord {
    let mut r = OrderRecord {
        sent_tick: Some(0),
        eta_lo: 5,
        eta_hi: 20,
        eta_hi0: 20,
        ..rec()
    };
    match kind {
        // the delay is learned at 12 (eta_hi 30), the ack arrives at 40, work starts 45, ends 60
        'd' => {
            if now >= 12 {
                r.eta_hi = 30;
            }
            if now >= 40 {
                (r.ack_tick, r.interpreted) = (Some(40), Some(Order::Hold));
            }
            r.executed_tick = (now >= 45).then_some(45);
            r.completed_tick = (now >= 60).then_some(60);
        }
        // no word ever; the loss becomes known at 50
        'l' => r.lost_known = now >= 50,
        // ack at 25 with the wrong reading, executing from 30, completed at 40
        'm' => {
            if now >= 25 {
                (r.ack_tick, r.interpreted) = (Some(25), Some(Order::Form(Formation::Line)));
            }
            r.executed_tick = (now >= 30).then_some(30);
            r.completed_tick = (now >= 40).then_some(40);
        }
        _ => unreachable!(),
    }
    r
}

/// `(first tick, status)` for every change over ticks 0..=70.
fn runs(kind: char) -> Vec<(u64, OrderStatus)> {
    let p = StatusParams::from_toml_str(&TOML.replace("2400", "10")).unwrap();
    let mut out: Vec<(u64, OrderStatus)> = Vec::new();
    for now in 0..=70 {
        let s = derive_status(&timeline(kind, now), now, &p);
        if out.last().map(|x| x.1) != Some(s) {
            out.push((now, s));
        }
    }
    out
}

#[test]
fn scripted_fixture_delay() {
    assert_eq!(
        runs('d'),
        vec![
            (0, InTransit),
            (12, Delayed { by_ticks: 10 }),
            (31, Overdue),
            (40, Received),
            (45, Executing),
            (60, Completed),
        ]
    );
}

#[test]
fn scripted_fixture_loss() {
    assert_eq!(
        runs('l'),
        vec![
            (0, InTransit),
            (21, Overdue),
            (31, PresumedLost),
            (50, Lost)
        ]
    );
}

#[test]
fn scripted_fixture_misread() {
    assert_eq!(
        runs('m'),
        vec![
            (0, InTransit),
            (21, Overdue),
            (25, Misread),
            (40, Completed)
        ]
    );
    let p = params();
    assert!(order_view(&timeline('m', 25), 25, &p).misread);
    assert!(order_view(&timeline('m', 70), 70, &p).misread);
    assert!(!order_view(&timeline('m', 24), 24, &p).misread);
}
