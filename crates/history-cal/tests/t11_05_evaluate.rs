//! T11-05: band evaluation (SPEC-011 section 4.4): verdicts, lower median, spread, excess,
//! `upper_only`, `weak`, and the band reader's provenance rules.

use battle_rules::DataError;
use history_cal::{Band, BandSet, Capabilities, Policy, ScriptError, Verdict};

const POLICY: &str = include_str!("../../../qc/calibration/policy.toml");
const ALL: Capabilities = Capabilities {
    capture: true,
    morale: true,
    melee: true,
};

fn policy() -> Policy {
    Policy::from_toml_str(POLICY).unwrap()
}

/// One band over the toy brigade; `extra` lines are appended to the row.
fn band(low: i64, high: Option<i64>, conf: &str, extra: &str) -> Band {
    let high = high.map_or(String::new(), |h| format!("high = {h}\n"));
    let sources = if conf == "unverified" {
        ""
    } else {
        "sources = [{ document = \"CWV\", page = 69, tag = \"V\" }]\n"
    };
    let text = format!(
        "schema = \"history-bands/1\"\n[[band]]\nid = \"TB-01\"\nscenario = \"toy_replay\"\nunit = \"toy_bde\"\nmetric = \"count\"\nbasis = \"total\"\nfrom_s = 0\nto_s = 100\nlow = {low}\n{high}phrase = \"exact\"\nsource_value = 150\nconfidence = \"{conf}\"\n{sources}placeholder = {}\n{}{extra}",
        conf == "unverified",
        if conf == "unverified" {
            "calibration_target = \"CT-011-1101\"\n"
        } else {
            ""
        }
    );
    BandSet::from_toml_str(&text).unwrap().bands()[0].clone()
}

fn eval(b: &Band, samples: &[i64]) -> history_cal::BandResult {
    history_cal::evaluate(b, samples, &ALL, &policy())
}

#[test]
fn medians_inside_and_outside_a_closed_band() {
    let b = band(100, Some(200), "partial", "");
    for (median, verdict, excess) in [
        (150, Verdict::Pass, 0),
        (50, Verdict::Fail, 500),
        (250, Verdict::Fail, 500),
    ] {
        let r = eval(&b, &[median]);
        assert_eq!(
            (r.verdict, r.excess_pm, r.median),
            (verdict, excess, median)
        );
    }
}

#[test]
fn an_open_band_divides_by_its_low_edge() {
    let b = band(100, None, "partial", "");
    let r = eval(&b, &[50]);
    assert_eq!((r.verdict, r.excess_pm), (Verdict::Fail, 500));
    assert_eq!(eval(&b, &[5000]).verdict, Verdict::Pass);
    // A zero width band divides by its low edge too; low 0 divides by 1.
    let point = band(100, Some(100), "partial", "");
    assert_eq!(eval(&point, &[150]).excess_pm, 500);
    let zero = band(0, Some(0), "partial", "");
    assert_eq!(eval(&zero, &[3]).excess_pm, 3000);
}

#[test]
fn a_wide_spread_is_unstable() {
    let b = band(100, Some(200), "partial", "");
    let r = eval(&b, &[100, 100, 200]);
    assert_eq!((r.verdict, r.spread_pm), (Verdict::Unstable, 1000));
    assert_eq!((r.min, r.median, r.max), (100, 100, 200));
    // The sample order never matters and a spread inside the limit is judged on its median.
    assert_eq!(eval(&b, &[200, 100, 100]).verdict, Verdict::Unstable);
    let tight = eval(&b, &[140, 150, 160, 150]);
    assert_eq!((tight.spread_pm, tight.verdict), (133, Verdict::Pass));
    // One or two seeds have no meaningful spread.
    assert_eq!(eval(&b, &[100, 200]).verdict, Verdict::Pass);
}

#[test]
fn an_even_count_takes_the_lower_median() {
    let b = band(100, Some(200), "partial", "");
    let r = eval(&b, &[300, 150, 120, 250]);
    assert_eq!(r.median, 150);
    assert_eq!(eval(&b, &[10, 20]).median, 10);
}

#[test]
fn unverified_bands_report_only() {
    let b = band(100, Some(200), "unverified", "");
    let r = eval(&b, &[999]);
    assert_eq!(r.verdict, Verdict::Report);
    assert_eq!(r.excess_pm, 7990);
    assert!(!r.weak);
}

#[test]
fn absent_capture_makes_a_total_band_upper_only() {
    let b = band(100, Some(200), "partial", "needs = \"capture\"\n");
    let none = Capabilities::default();
    let p = policy();
    let below = history_cal::evaluate(&b, &[10], &none, &p);
    assert!(below.upper_only);
    assert_eq!((below.verdict, below.excess_pm), (Verdict::Pass, 0));
    let above = history_cal::evaluate(&b, &[300], &none, &p);
    assert_eq!((above.verdict, above.excess_pm), (Verdict::Fail, 1000));
    // With capture the same band gates on both edges.
    assert!(!eval(&b, &[10]).upper_only);
    assert_eq!(eval(&b, &[10]).verdict, Verdict::Fail);
    // Open above and upper only: nothing to exceed.
    let open = band(100, None, "partial", "needs = \"capture\"\n");
    assert_eq!(
        history_cal::evaluate(&open, &[1], &none, &p).verdict,
        Verdict::Pass
    );
}

#[test]
fn an_absent_capability_is_not_measurable() {
    let b = band(250, Some(300), "partial", "needs = \"morale\"\n");
    let none = Capabilities {
        capture: true,
        morale: false,
        melee: true,
    };
    let r = history_cal::evaluate(&b, &[275], &none, &policy());
    assert_eq!(r.verdict, Verdict::NotMeasurable);
    assert_eq!(eval(&b, &[275]).verdict, Verdict::Pass);
    // `NotMeasurable` comes before `Report`.
    let u = band(250, Some(300), "unverified", "needs = \"melee\"\n");
    let nm = Capabilities {
        melee: false,
        ..ALL
    };
    assert_eq!(
        history_cal::evaluate(&u, &[275], &nm, &policy()).verdict,
        Verdict::NotMeasurable
    );
}

#[test]
fn a_gated_band_wider_than_the_limit_is_weak() {
    let p = policy();
    assert_eq!(p.weak_width_pm, 1500);
    // Width 1000 * 150 / 100 = 1500 is not above the limit; 1501 is.
    assert!(!eval(&band(100, Some(250), "partial", ""), &[120]).weak);
    assert!(eval(&band(100, Some(251), "partial", ""), &[120]).weak);
    assert!(!eval(&band(100, None, "partial", ""), &[120]).weak);
    // Weakness is a flag on gated bands only.
    assert!(!eval(&band(100, Some(900), "unverified", ""), &[120]).weak);
}

#[test]
fn the_reader_enforces_provenance_and_shape() {
    let head = "schema = \"history-bands/1\"\n[[band]]\nid = \"TB-01\"\nscenario = \"toy_replay\"\nunit = \"toy_bde\"\nmetric = \"count\"\nbasis = \"kw\"\nfrom_s = 0\nto_s = 100\nlow = 1\nhigh = 2\nphrase = \"exact\"\n";
    let ok = format!("{head}confidence = \"unverified\"\nplaceholder = true\ncalibration_target = \"CT-011-1101\"\n");
    let set = BandSet::from_toml_str(&ok).unwrap();
    assert_eq!(set.bands().len(), 1);
    let load = |tail: &str| BandSet::from_toml_str(&format!("{head}{tail}"));
    let prov = |r: Result<BandSet, ScriptError>| {
        matches!(r, Err(ScriptError::Data(DataError::Provenance(_))))
    };
    assert!(prov(load(
        "confidence = \"unverified\"\nplaceholder = true\n"
    )));
    assert!(prov(load("confidence = \"partial\"\n")));
    assert!(prov(load(
        "confidence = \"verified\"\nsources = [{ document = \"CWV\", page = 1 }]\n"
    )));
    assert!(load("confidence = \"verified\"\nsources = [{ document = \"CWV\", page = 1 }]\nsecond_source = \"OR p1\"\n").is_ok());
    let data = |r: Result<BandSet, ScriptError>| matches!(r, Err(ScriptError::Data(_)));
    let p =
        "confidence = \"unverified\"\nplaceholder = true\ncalibration_target = \"CT-011-1101\"\n";
    assert!(data(load(&format!("{p}wat = 1\n"))));
    assert!(data(BandSet::from_toml_str(
        &ok.replace("low = 1", "low = 1.5")
    )));
    assert!(data(BandSet::from_toml_str(
        &ok.replace("metric = \"count\"", "metric = \"mean\"")
    )));
    assert!(data(BandSet::from_toml_str(
        &ok.replace("basis = \"kw\"", "basis = \"all\"")
    )));
    assert!(data(BandSet::from_toml_str(&format!(
        "{ok}needs = \"luck\"\n"
    ))));
    assert!(data(BandSet::from_toml_str(
        &ok.replace("history-bands/1", "bands/1")
    )));
    // high below low, an empty window and a repeated id are refused.
    assert!(BandSet::from_toml_str(&ok.replace("high = 2", "high = 0")).is_err());
    assert!(matches!(
        BandSet::from_toml_str(&ok.replace("to_s = 100", "to_s = 0")),
        Err(ScriptError::Time(_))
    ));
    let twice = format!("{ok}{}", &ok["schema = \"history-bands/1\"\n".len()..]);
    assert_eq!(
        BandSet::from_toml_str(&twice),
        Err(ScriptError::Duplicate("TB-01".into()))
    );
}

#[test]
fn the_set_is_ordered_by_id_and_selects_a_scenario_with_any() {
    let row = |id: &str, scenario: &str| {
        format!(
            "[[band]]\nid = \"{id}\"\nscenario = \"{scenario}\"\nunit = \"u\"\nmetric = \"rate_pm\"\nbasis = \"kw\"\nfrom_s = 0\nto_s = 9\nlow = 1\nphrase = \"over\"\nsource_value = 1\nconfidence = \"unverified\"\nplaceholder = true\ncalibration_target = \"CT-011-1101\"\n"
        )
    };
    let text = format!(
        "schema = \"history-bands/1\"\n{}{}{}",
        row("HB-02", "gb_day1"),
        row("HB-01", "any"),
        row("HB-03", "gb_day2")
    );
    let set = BandSet::from_toml_str(&text).unwrap();
    let ids: Vec<&str> = set.bands().iter().map(|b| b.id.as_str()).collect();
    assert_eq!(ids, ["HB-01", "HB-02", "HB-03"]);
    let day1: Vec<&str> = set.for_scenario("gb_day1").map(|b| b.id.as_str()).collect();
    assert_eq!(day1, ["HB-01", "HB-02"]);
    assert_eq!(set.get("HB-03").unwrap().high, None);
    assert!(set.get("HB-09").is_none());
}
