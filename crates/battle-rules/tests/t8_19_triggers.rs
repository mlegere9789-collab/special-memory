//! T8-19 (SPEC-008 sections 2.3, 4, 11): triggers act at exactly `at_s * tick_hz`, at the
//! boundary before movement: orders, fire mode, resupply and markers, and weather rows.

mod t084_support;

use battle_rules::{battle_hash, Order, F0};
use t084_support::{battle, four, scenario_text, trigger, U};

/// The boundary of tick `t` runs in the step that takes the world from `t` to `t + 1`.
fn step_to(b: &mut battle_rules::Battle, tick: u64) {
    while b.state.world.tick < tick {
        b.step();
    }
}

#[test]
fn an_order_trigger_fires_at_its_tick_and_not_before() {
    let extra = trigger(
        "go",
        3,
        "b1",
        "order",
        "order = \"move\"\nx_mm = 80000\ny_mm = 20000\ngait = \"quick\"",
    );
    let mut b = battle(&scenario_text(0, 60, &four(), &extra), 1);
    step_to(&mut b, 60);
    assert_eq!(b.state.world.tick, 60);
    assert_eq!(
        b.state.regiments[0].order,
        Order::Hold,
        "not before tick 60"
    );
    b.step();
    assert!(
        matches!(b.state.regiments[0].order, Order::Move { .. }),
        "at tick 60: {:?}",
        b.state.regiments[0].order
    );
    assert_eq!(b.fid.triggers_done, 1);
    // No other regiment received it.
    assert_eq!(b.state.regiments[1].order, Order::Hold);
}

#[test]
fn form_face_and_hold_triggers_issue_their_orders() {
    let extra = [
        trigger(
            "f",
            1,
            "b1",
            "order",
            "order = \"form\"\nformation = \"column\"",
        ),
        trigger("h", 1, "b2", "order", "order = \"face\"\nheading_deg = 90"),
    ]
    .concat();
    let mut b = battle(&scenario_text(0, 60, &four(), &extra), 1);
    step_to(&mut b, 20);
    b.step();
    let r0 = &b.state.regiments[0];
    assert!(matches!(
        r0.state,
        battle_rules::RegimentState::Forming { .. }
    ));
    assert!(matches!(b.state.regiments[1].order, Order::Face(_)));
}

#[test]
fn a_fire_mode_trigger_starts_the_shooting() {
    // g1 and g2 hold fire until the trigger names a target.
    let extra = trigger("fire", 2, "b1", "fire_mode", "target = \"g1\"");
    let mut b = battle(&scenario_text(0, 60, &four(), &extra), 1);
    step_to(&mut b, 40);
    assert_eq!(b.state.counters().shots_fired, 0);
    for _ in 0..400 {
        b.step();
    }
    assert!(b.state.counters().shots_fired > 0);
    assert_eq!(b.state.regiments[0].class, F0);
    // Only the regiment of b1 fired.
    assert_eq!(b.ledger(1).ledger.spent, 0);
    assert!(b.ledger(0).ledger.spent > 0);
}

#[test]
fn a_resupply_trigger_adds_rounds_at_its_tick() {
    let extra = trigger("ammo", 5, "g1", "resupply", "rounds = 7");
    let mut b = battle(&scenario_text(0, 60, &four(), &extra), 1);
    step_to(&mut b, 100);
    let before = b.ledger(1).ledger;
    assert_eq!(before.resupplied, 0);
    b.step();
    let after = b.ledger(1).ledger;
    assert_eq!(after.resupplied, 7 * 120);
    assert_eq!(after.held, before.held + 7 * 120);
    assert_eq!(
        after.held + after.spent + after.lost,
        after.initial + after.resupplied
    );
}

#[test]
fn a_resupply_of_an_aggregate_adds_to_its_rounds() {
    let units = [U::blue("b", 50, 0), U::grey("g", 110, 0)];
    let extra = trigger("ammo", 5, "g", "resupply", "rounds = 4");
    let mut b = battle(&scenario_text(2, 60, &units, &extra), 1);
    step_to(&mut b, 100);
    assert_eq!(b.state.regiments[1].aggregate.rounds, 120 * 40);
    b.step();
    assert_eq!(b.state.regiments[1].aggregate.rounds, 120 * 44);
    let l = b.ledger(1).ledger;
    assert_eq!(l.held + l.spent + l.lost, l.initial + l.resupplied);
}

#[test]
fn a_marker_changes_nothing_but_the_cursor() {
    let with = trigger("m", 2, "b1", "marker", "");
    let mut a = battle(&scenario_text(0, 60, &four(), &with), 4);
    let mut b = battle(&scenario_text(0, 60, &four(), ""), 4);
    for _ in 0..200 {
        a.step();
        b.step();
    }
    assert_eq!(battle_hash(&a.state), battle_hash(&b.state));
    assert_eq!(a.fid.triggers_done, 1);
}

#[test]
fn weather_rows_apply_at_their_second() {
    let extra = "[[weather]]\nfrom_s = 2\nwind_x_mm_s = 3000\nwind_y_mm_s = -1000\nwet_pm = 500\n\
        ground_pm = 400\nvisibility_m = 4000\nconfidence = \"unverified\"\nplaceholder = true\n\
        calibration_target = \"CT-008-88\"\nsources = [{ document = \"CWV\", page = 79, note = \"test\" }]\n";
    let mut b = battle(&scenario_text(0, 60, &four(), extra), 1);
    step_to(&mut b, 40);
    let w = b.state.weather;
    assert_eq!((w.wind.x.raw(), w.wind.y.raw()), (0, 0));
    b.step();
    let w = b.state.weather;
    assert_eq!(w.wind.x, battle_rules::fx_ratio(3, 1));
    assert_eq!(w.wind.y, battle_rules::fx_ratio(-1, 1));
    assert_eq!(
        w.wet_pm, 1500,
        "dry is 1000; air wetness adds to the misfire factor"
    );
    assert_eq!(
        w.ground_pm, 800,
        "ground wetness 400 permille gives restitution 800"
    );
}
