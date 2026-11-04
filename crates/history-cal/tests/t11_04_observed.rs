//! T11-04, observed half: the four observation kinds, windows (absolute and after a trigger),
//! gating by confidence, the median over seeds, and `reached` against the SPEC-004 T4-03 march.

mod common;

use battle_rules::{scenarios, Casualty, CasualtyKind, Gait, Order};
use common::{script, tables, trigger, units, PROV};
use history_cal::{
    compile_script, evaluate_observed, stage_battle, units_from_toml_str, variants_to_run,
    Observation, ReplayDriver, ReplayScript, ScriptError, Variant, Verdict,
};

fn observe(id: &str, kind_fields: &str, lo_s: i64, hi_s: i64, extra: &str, conf: &str) -> String {
    let prov = if conf == "unverified" {
        PROV.to_owned()
    } else {
        format!("confidence = \"{conf}\"\nsources = [{{ document = \"CWV\", page = 80, tag = \"V\" }}]\n")
    };
    format!("[[observe]]\nid = \"{id}\"\n{kind_fields}lo_s = {lo_s}\nhi_s = {hi_s}\n{extra}{prov}")
}

fn hit(tick: u64, unit: u32, shooter: u32, kind: CasualtyKind) -> Casualty {
    Casualty {
        tick,
        target_index: u32::MAX,
        unit,
        team: 0,
        kind,
        shooter_unit: shooter,
        ammo: 0,
    }
}

// Regiment ids: toy_b1 0, toy_r1 1, toy_r2 2 (toy_bde = r1 and r2).
fn run(s: &ReplayScript, log: Vec<Casualty>, until: u64) -> ReplayDriver {
    let mut b = stage_battle(&units(), s, &tables(), 1).unwrap();
    b.casualties = log;
    let mut d = ReplayDriver::new(b, compile_script(s, &units()).unwrap(), s).unwrap();
    d.observe(&units()).unwrap();
    d.run_to(until).unwrap();
    d
}

fn by_id(d: &ReplayDriver, id: &str) -> Observation {
    d.observations().into_iter().find(|o| o.id == id).unwrap()
}

#[test]
fn first_and_last_casualty_by_a_unit_on_another() {
    let rows = [
        observe(
            "first",
            "kind = \"first_casualty\"\na = \"toy_b1\"\nb = \"toy_bde\"\n",
            0,
            100,
            "",
            "partial",
        ),
        observe(
            "last",
            "kind = \"last_casualty_by\"\na = \"toy_b1\"\nb = \"toy_bde\"\n",
            0,
            100,
            "",
            "partial",
        ),
    ]
    .concat();
    let s = script(0, &rows);
    let log = vec![
        hit(5, 1, 1, CasualtyKind::Killed), // r1 shot by r1: wrong shooter
        hit(30, 2, 0, CasualtyKind::Disabled), // disabled does not count
        hit(40, 2, 0, CasualtyKind::Wounded),
        hit(50, 0, 0, CasualtyKind::Killed), // shot b1 itself: wrong target
        hit(70, 1, 0, CasualtyKind::Killed),
    ];
    let d = run(&s, log, 100);
    assert_eq!(by_id(&d, "first").tick, Some(40));
    assert_eq!(by_id(&d, "last").tick, Some(70));
}

#[test]
fn a_casualty_event_that_never_happens_has_no_tick() {
    let rows = observe(
        "none",
        "kind = \"first_casualty\"\na = \"toy_b1\"\nb = \"toy_bde\"\n",
        0,
        10,
        "",
        "partial",
    );
    let d = run(&script(0, &rows), vec![], 20);
    let o = by_id(&d, "none");
    assert_eq!(o.tick, None);
    assert!(!o.inside());
    assert_eq!(evaluate_observed(&[o]).verdict, Verdict::Fail);
}

#[test]
fn loss_rate_fires_at_the_casualty_that_reaches_it() {
    // toy_r1 has 100 men: 25 pm needs 3 records (30 pm; 2 records is 20 pm).
    let rows = observe(
        "rate",
        "kind = \"loss_rate_at_least\"\nunit = \"toy_r1\"\nrate_pm = 25\n",
        0,
        100,
        "",
        "partial",
    );
    let log = (0..4)
        .map(|i| hit(10 + i * 10, 1, 0, CasualtyKind::Killed))
        .collect();
    let d = run(&script(0, &rows), log, 100);
    assert_eq!(by_id(&d, "rate").tick, Some(30));
}

#[test]
fn windows_are_half_open_in_ticks_from_the_clock_start() {
    let rows = observe(
        "w",
        "kind = \"first_casualty\"\na = \"toy_b1\"\nb = \"toy_r1\"\n",
        3601,
        3603,
        "",
        "partial",
    );
    let s = script(3600, &rows);
    for (tick, inside) in [(19, false), (20, true), (59, true), (60, false)] {
        let d = run(&s, vec![hit(tick, 1, 0, CasualtyKind::Killed)], 70);
        let o = by_id(&d, "w");
        assert_eq!(o.window, Some((20, 60)));
        assert_eq!(o.inside(), inside, "tick {tick}");
    }
}

#[test]
fn an_after_window_is_an_offset_from_the_logged_trigger_tick() {
    let rows = [
        trigger("t1", 2, ""),
        observe(
            "a",
            "kind = \"first_casualty\"\na = \"toy_b1\"\nb = \"toy_r1\"\n",
            1,
            3,
            "after = \"t1\"\n",
            "partial",
        ),
    ]
    .concat();
    let s = script(0, &rows);
    // The trigger logs tick 40, so the window is ticks 60..100.
    let d = run(&s, vec![hit(70, 1, 0, CasualtyKind::Killed)], 120);
    let o = by_id(&d, "a");
    assert_eq!((o.window, o.inside()), (Some((60, 100)), true));
    // Before its trigger has fired the window is unknown.
    let d = run(&s, vec![], 10);
    assert_eq!(by_id(&d, "a").window, None);
    // A unit that is not in the list is a reference error.
    let ghost = script(
        0,
        &observe(
            "g",
            "kind = \"reached\"\nunit = \"ghost\"\nx_m = 0\ny_m = 0\nradius_m = 5\n",
            1,
            3,
            "",
            "partial",
        ),
    );
    let mut d = ReplayDriver::new(
        stage_battle(&units(), &ghost, &tables(), 1).unwrap(),
        vec![],
        &ghost,
    )
    .unwrap();
    assert!(matches!(
        d.observe(&units()),
        Err(ScriptError::Reference(_))
    ));
}

#[test]
fn the_median_over_seeds_decides_and_confidence_gates() {
    let ob = |tick: Option<u64>, gated: bool| Observation {
        id: "o".into(),
        tick,
        window: Some((100, 200)),
        gated,
    };
    let seeds = |ticks: &[Option<u64>], gated| -> Vec<Observation> {
        ticks.iter().map(|t| ob(*t, gated)).collect()
    };
    // Lower median of [50, 150, 150, 900] is 150; a None counts as later than every tick.
    let r = evaluate_observed(&seeds(&[Some(900), Some(50), Some(150), Some(150)], true));
    assert_eq!((r.median, r.verdict), (Some(150), Verdict::Pass));
    let r = evaluate_observed(&seeds(&[None, Some(50), Some(900)], true));
    assert_eq!((r.median, r.verdict), (Some(900), Verdict::Fail));
    let r = evaluate_observed(&seeds(&[None, None, Some(150)], true));
    assert_eq!((r.median, r.verdict), (None, Verdict::Fail));
    // The window edges: lo is inside, hi is outside.
    assert_eq!(
        evaluate_observed(&seeds(&[Some(100)], true)).verdict,
        Verdict::Pass
    );
    assert_eq!(
        evaluate_observed(&seeds(&[Some(200)], true)).verdict,
        Verdict::Fail
    );
    // Not gated: reports whatever happened.
    assert_eq!(
        evaluate_observed(&seeds(&[Some(900)], false)).verdict,
        Verdict::Report
    );
}

#[test]
fn gating_follows_the_row_confidence() {
    let kind = "kind = \"first_casualty\"\na = \"toy_b1\"\nb = \"toy_r1\"\n";
    let rows = [
        observe("p", kind, 0, 10, "", "partial"),
        observe("u", kind, 0, 10, "", "unverified"),
    ]
    .concat();
    let d = run(&script(0, &rows), vec![], 5);
    assert!(by_id(&d, "p").gated);
    assert!(!by_id(&d, "u").gated);
}

/// A 400-man trained column on open ground, the `march_single` regiment.
const MARCH_UNITS: &str = "schema = \"replay-units/1\"\n[[unit]]\nid = \"march_r1\"\nlevel = \"regiment\"\nparent = \"\"\nside = 0\nstrength = 400\ntraining = \"trained\"\nformation = \"column\"\nx_m = 0\ny_m = 0\nconfidence = \"unverified\"\nplaceholder = true\ncalibration_target = \"CT-011-1101\"\n";

/// The arrival tick of `march_single` at quick time, measured as SPEC-004 T4-03 does.
fn march_single_arrival() -> u64 {
    let t = tables();
    let mut b = scenarios::march_single_at(&t, 1, Gait::Quick);
    while b.regiments[0].order != Order::Hold {
        assert!(b.world.tick < 20_000);
        b.step_movement();
    }
    b.world.tick
}

#[test]
fn reached_fires_within_one_percent_of_the_march_single_arrival() {
    let rows = format!(
        "[[order]]\nid = \"go\"\nunit = \"march_r1\"\nat_s = 0\nkind = \"move\"\nx_m = 200\ny_m = 0\ngait = \"quick\"\n{PROV}{}",
        observe("arrive", "kind = \"reached\"\nunit = \"march_r1\"\nx_m = 200\ny_m = 0\nradius_m = 0\n", 1, 400, "", "partial")
    );
    let head = "schema = \"replay-script/1\"\n[meta]\nscenario = \"march\"\nfidelity_class = 0\nend_s = 400\n";
    let s = ReplayScript::from_toml_str(&format!("{head}{rows}")).unwrap();
    let u = units_from_toml_str(MARCH_UNITS).unwrap();
    let b = stage_battle(&u, &s, &tables(), 1).unwrap();
    let mut d = ReplayDriver::new(b, compile_script(&s, &u).unwrap(), &s).unwrap();
    d.observe(&u).unwrap();
    d.run_to(4000).unwrap();
    let o = by_id(&d, "arrive");
    let reference = march_single_arrival();
    let tick = o.tick.expect("the regiment arrives");
    assert!(
        tick.abs_diff(reference) * 100 <= reference,
        "reached at {tick}, march_single arrives at {reference}"
    );
    assert!(o.inside() && o.gated);
    assert_eq!(evaluate_observed(&[o]).verdict, Verdict::Pass);
}

#[test]
fn reached_uses_the_centroid_of_the_units_regiments() {
    // The brigade's regiments start at (0, 0) and (0, 100): the centroid is (0, 50).
    let rows = observe(
        "c",
        "kind = \"reached\"\nunit = \"toy_bde\"\nx_m = 0\ny_m = 50\nradius_m = 1\n",
        1,
        100,
        "",
        "partial",
    );
    let d = run(&script(0, &rows), vec![], 2);
    assert_eq!(by_id(&d, "c").tick, Some(1));
    let rows = observe(
        "c",
        "kind = \"reached\"\nunit = \"toy_bde\"\nx_m = 0\ny_m = 0\nradius_m = 49\n",
        1,
        100,
        "",
        "partial",
    );
    let d = run(&script(0, &rows), vec![], 2);
    assert_eq!(by_id(&d, "c").tick, None);
}

#[test]
fn variants_follow_the_trigger_rows() {
    let plain = script(0, &trigger("t1", 5, ""));
    assert_eq!(variants_to_run(&plain), vec![Variant::Nominal]);
    let early = script(0, &trigger("t1", 5, "early_s = 4\n"));
    // A row with either conflicting time makes the three runs of section 4.5.
    assert_eq!(
        variants_to_run(&early),
        vec![Variant::Nominal, Variant::Early, Variant::Late]
    );
    let both = script(0, &trigger("t1", 5, "early_s = 4\nlate_s = 6\n"));
    assert_eq!(
        variants_to_run(&both),
        vec![Variant::Nominal, Variant::Early, Variant::Late]
    );
}
