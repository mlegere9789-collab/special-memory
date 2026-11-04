//! T11-04, scripted half: triggers log exactly `(at_s - clock_start_s) * 20`, and the early,
//! late and nominal variants each log their own row time (SPEC-011 sections 4.5 and 8).

mod common;

use common::{compile, script, tables, trigger, units};
use history_cal::{apply_variant, stage_battle, ReplayDriver, ReplayScript, Variant};

fn log_of(s: &ReplayScript, until: u64) -> Vec<(u64, u16)> {
    let b = stage_battle(&units(), s, &tables(), 1).unwrap();
    let mut d = ReplayDriver::new(b, compile(s), s).unwrap();
    d.run_to(until).unwrap();
    d.timeline_log().to_vec()
}

#[test]
fn nominal_triggers_log_the_exact_tick() {
    let rows = [trigger("t1", 3601, ""), trigger("t2", 3604, "")].concat();
    assert_eq!(log_of(&script(3600, &rows), 100), vec![(20, 0), (80, 1)]);
}

#[test]
fn trigger_index_follows_ascending_id_not_file_order() {
    let rows = [trigger("t_b", 2, ""), trigger("t_a", 2, "")].concat();
    assert_eq!(log_of(&script(0, &rows), 60), vec![(40, 0), (40, 1)]);
}

#[test]
fn the_three_variants_each_log_their_own_time() {
    let rows = [
        trigger("t1", 5, "early_s = 4\nlate_s = 6\n"),
        trigger("t2", 3, ""),
    ]
    .concat();
    let s = script(0, &rows);
    let nominal = log_of(&apply_variant(&s, Variant::Nominal), 140);
    let early = log_of(&apply_variant(&s, Variant::Early), 140);
    let late = log_of(&apply_variant(&s, Variant::Late), 140);
    assert_eq!(nominal, vec![(60, 1), (100, 0)]);
    assert_eq!(early, vec![(60, 1), (80, 0)]);
    assert_eq!(late, vec![(60, 1), (120, 0)]);
    assert_eq!(Variant::Nominal.name(), "nominal");
    assert_eq!(Variant::Early.name(), "early");
    assert_eq!(Variant::Late.name(), "late");
}
