//! T4-03: `march_single` arrival tick against the exact reference model (`ref_march.py`) within
//! 1 percent for each gait, never earlier than `ceil(distance * 20 / speed)`, and the largest
//! slot error never above `max_lag_mm` (SPEC-004 section 13 and Amendment A2).

mod t019_support;

use battle_rules::{scenarios, Gait, Order};
use core_math::Fx;
use t019_support::{max_slot_error, mm, ref_rows, tables, within_percent};

fn run(gait: Gait) -> (u64, Fx) {
    let t = tables();
    let mut b = scenarios::march_single_at(&t, 1, gait);
    let mut worst = Fx::ZERO;
    while b.regiments[0].order != Order::Hold {
        assert!(b.world.tick < 20_000, "{gait:?} never arrives");
        b.step_movement();
        worst = worst.max(max_slot_error(&b, 0));
    }
    assert_eq!(b.regiments[0].anchor.x, Fx::from_int(scenarios::MARCH_M));
    (b.world.tick, worst)
}

#[test]
fn arrival_matches_the_reference_for_every_gait() {
    let lag = mm(i64::from(tables().locomotion.globals().max_lag_mm));
    let rows = ref_rows();
    for gait in Gait::ALL {
        let row = rows
            .iter()
            .find(|r| r.0 == "march" && r.1 == gait.id())
            .unwrap();
        let (arrival, worst) = run(gait);
        println!(
            "{gait:?}: arrival {arrival} model {} ideal {} max slot error {worst:?}",
            row.4, row.3
        );
        assert!(
            within_percent(arrival, row.4, 1),
            "{gait:?}: {arrival} vs model {}",
            row.4
        );
        assert!(
            arrival >= row.3,
            "{gait:?} arrived before the ideal tick {}",
            row.3
        );
        assert!(worst <= lag, "{gait:?}: slot error {worst:?} above max_lag");
    }
}

#[test]
fn reference_rows_follow_the_ideal_formula() {
    let t = tables();
    for r in ref_rows().iter().filter(|r| r.0 == "march") {
        let gait = Gait::ALL.into_iter().find(|g| g.id() == r.1).unwrap();
        let speed = u64::from(t.locomotion.gait(gait).speed_mm_s);
        assert_eq!(r.3, (r.2 as u64 * 20).div_ceil(speed));
        assert!(r.4 >= r.3);
    }
}
