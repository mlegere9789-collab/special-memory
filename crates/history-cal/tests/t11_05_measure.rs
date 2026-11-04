//! T11-05, measuring half: `count`, `rate_pm` and `rate_pm_median` over the casualty log
//! (SPEC-011 section 4.3). Casualty records are placed in the log by hand.

mod common;

use battle_rules::{Casualty, CasualtyKind};
use common::{script, tables, units};
use history_cal::{measure, stage_battle, try_measure, BandSet, ReplayDriver};

// Regiment ids by ascending unit id: toy_b1 0, toy_r1 1, toy_r2 2; toy_bde holds r1 and r2.
fn hit(tick: u64, unit: u32, kind: CasualtyKind) -> Casualty {
    Casualty {
        tick,
        target_index: u32::MAX,
        unit,
        team: 0,
        kind,
        shooter_unit: 0,
        ammo: 0,
    }
}

fn driver(log: Vec<Casualty>, clock_start_s: i64) -> ReplayDriver {
    let s = script(clock_start_s, "");
    let mut b = stage_battle(&units(), &s, &tables(), 1).unwrap();
    b.casualties = log;
    ReplayDriver::new(b, vec![], &s).unwrap()
}

fn band(unit: &str, metric: &str, basis: &str, from_s: i64, to_s: i64) -> history_cal::Band {
    let text = format!(
        "schema = \"history-bands/1\"\n[[band]]\nid = \"TB-01\"\nscenario = \"toy_replay\"\nunit = \"{unit}\"\nmetric = \"{metric}\"\nbasis = \"{basis}\"\nfrom_s = {from_s}\nto_s = {to_s}\nlow = 1\nphrase = \"over\"\nsource_value = 1\nconfidence = \"unverified\"\nplaceholder = true\ncalibration_target = \"CT-011-1101\"\n"
    );
    BandSet::from_toml_str(&text).unwrap().bands()[0].clone()
}

use CasualtyKind::{Disabled, Killed, Wounded};

#[test]
fn count_takes_unit_window_and_basis() {
    let log = vec![
        hit(0, 1, Killed),
        hit(19, 1, Wounded),
        hit(20, 2, Killed),
        hit(39, 0, Killed),
        hit(40, 1, Killed),
        hit(30, 1, Disabled),
    ];
    let d = driver(log, 0);
    let u = units();
    // The brigade is regiments 1 and 2; ticks 0..40 hold four of its records but one is Disabled.
    assert_eq!(measure(&band("toy_bde", "count", "kw", 0, 2), &d, &u), 3);
    assert_eq!(measure(&band("toy_bde", "count", "total", 0, 2), &d, &u), 3);
    // Half-open: tick 20 is in [1 s, 2 s) and tick 40 is not.
    assert_eq!(measure(&band("toy_bde", "count", "kw", 1, 2), &d, &u), 1);
    assert_eq!(measure(&band("toy_r1", "count", "kw", 0, 3), &d, &u), 3);
    assert_eq!(measure(&band("toy_b1", "count", "kw", 0, 3), &d, &u), 1);
}

#[test]
fn windows_are_clock_seconds_from_the_clock_start() {
    let d = driver(vec![hit(0, 1, Killed), hit(20, 1, Killed)], 3600);
    let u = units();
    assert_eq!(
        measure(&band("toy_r1", "count", "kw", 3600, 3601), &d, &u),
        1
    );
    assert_eq!(
        measure(&band("toy_r1", "count", "kw", 3601, 3602), &d, &u),
        1
    );
    assert_eq!(
        measure(&band("toy_r1", "count", "kw", 3600, 3602), &d, &u),
        2
    );
}

#[test]
fn rate_is_permille_of_the_strength_floored() {
    let log = (0..7)
        .map(|t| hit(t, 1, Killed))
        .chain([hit(1, 2, Wounded)])
        .collect();
    let d = driver(log, 0);
    let u = units();
    // r1 has 100 men and 7 records: 70 pm. The brigade has 200 men and 8 records: 40 pm.
    assert_eq!(measure(&band("toy_r1", "rate_pm", "kw", 0, 9), &d, &u), 70);
    assert_eq!(measure(&band("toy_bde", "rate_pm", "kw", 0, 9), &d, &u), 40);
    // 1000 * 1 / 200 = 5 exactly; 1000 * 1 / 100 = 10.
    let one = driver(vec![hit(0, 1, Killed)], 0);
    assert_eq!(
        measure(&band("toy_bde", "rate_pm", "kw", 0, 9), &one, &u),
        5
    );
}

#[test]
fn the_median_rate_is_taken_over_regiments() {
    // r1 loses 10 of 100 (100 pm), r2 loses 30 of 100 (300 pm); the lower median of two is r1's.
    let log = (0..10)
        .map(|t| hit(t, 1, Killed))
        .chain((0..30).map(|t| hit(t, 2, Killed)))
        .collect();
    let d = driver(log, 0);
    let u = units();
    assert_eq!(
        measure(&band("toy_bde", "rate_pm_median", "kw", 0, 9), &d, &u),
        100
    );
    assert_eq!(
        measure(&band("toy_bde", "rate_pm", "kw", 0, 9), &d, &u),
        200
    );
    assert_eq!(
        measure(&band("toy_r2", "rate_pm_median", "kw", 0, 9), &d, &u),
        300
    );
}

#[test]
fn an_unknown_unit_is_an_error_for_try_measure_and_zero_for_measure() {
    let d = driver(vec![hit(0, 1, Killed)], 0);
    let u = units();
    let b = band("nobody", "count", "kw", 0, 9);
    assert!(try_measure(&b, &d, &u).is_err());
    assert_eq!(measure(&b, &d, &u), 0);
}
