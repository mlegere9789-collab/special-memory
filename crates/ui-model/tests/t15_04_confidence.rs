//! T15-04 confidence: the score of an enemy marker from the age of the sighting and its position
//! error, the four bands, and the parameter file with its provenance. The HQ-view hash half of
//! T15-04 is `t15_04_hq_view.rs`.

use proptest::prelude::*;
use ui_model::{confidence_band, confidence_pm, ConfBand, ConfidenceParams};

const TOML: &str = include_str!("../../../data/ui/confidence.toml");

fn params() -> ConfidenceParams {
    ConfidenceParams::from_toml_str(TOML).expect("confidence.toml loads")
}

/// Ticks in `s` seconds at 20 ticks per second.
fn secs(s: u64) -> u64 {
    s * 20
}

#[test]
fn the_spec_examples_hold_with_the_defaults() {
    let p = params();
    for (age_s, err_mm, pm, band) in [
        (0, 0, 1000, ConfBand::Firm),
        (60, 100_000, 600, ConfBand::Probable),
        (100, 0, 500, ConfBand::Probable),
        (150, 0, 250, ConfBand::Possible),
        (120, 300_000, 100, ConfBand::Stale),
    ] {
        let got = confidence_pm(secs(age_s), err_mm, &p);
        assert_eq!(got, pm, "age {age_s} s, error {err_mm} mm");
        assert_eq!(confidence_band(got), band);
    }
}

#[test]
fn the_defaults_are_the_documented_weights() {
    let p = params();
    assert_eq!((p.age_pm_per_s, p.err_pm_per_100m), (5, 100));
}

#[test]
fn band_edges_are_inclusive_below() {
    for (pm, band) in [
        (1000, ConfBand::Firm),
        (750, ConfBand::Firm),
        (749, ConfBand::Probable),
        (500, ConfBand::Probable),
        (499, ConfBand::Possible),
        (250, ConfBand::Possible),
        (249, ConfBand::Stale),
        (0, ConfBand::Stale),
    ] {
        assert_eq!(confidence_band(pm), band, "{pm}");
    }
}

#[test]
fn age_is_whole_seconds_and_the_score_never_goes_below_zero() {
    let p = params();
    assert_eq!(confidence_pm(19, 0, &p), 1000);
    assert_eq!(confidence_pm(20, 0, &p), 995);
    assert_eq!(confidence_pm(u64::MAX, u32::MAX, &p), 0);
    // 99 m of error costs 99 permille, flooring the fraction.
    assert_eq!(confidence_pm(0, 99_999, &p), 901);
}

proptest! {
    #[test]
    fn non_increasing_in_age_and_error(
        age in 0u64..100_000,
        more_age in 0u64..100_000,
        err in 0u32..2_000_000,
        more_err in 0u32..2_000_000,
        age_w in 0u32..=20,
        err_w in 0u32..=400,
    ) {
        let text = TOML
            .replace("age_pm_per_s = 5", &format!("age_pm_per_s = {age_w}"))
            .replace("err_pm_per_100m = 100", &format!("err_pm_per_100m = {err_w}"));
        let p = ConfidenceParams::from_toml_str(&text).unwrap();
        let base = confidence_pm(age, err, &p);
        prop_assert!(confidence_pm(age + more_age, err, &p) <= base);
        prop_assert!(confidence_pm(age, err + more_err, &p) <= base);
        prop_assert!(base <= 1000);
    }
}

#[test]
fn loader_refuses_bad_files() {
    let float = TOML.replace("age_pm_per_s = 5", "age_pm_per_s = 5.5");
    assert!(ConfidenceParams::from_toml_str(&float).is_err());
    let unknown = format!("{TOML}\nextra = 1\n");
    assert!(ConfidenceParams::from_toml_str(&unknown).is_err());
    let no_target = TOML.replace("calibration_target = \"CT-015-1\"\n", "");
    assert!(ConfidenceParams::from_toml_str(&no_target).is_err());
    let wrong_schema = TOML.replace("ui-confidence/1", "ui-confidence/2");
    assert!(ConfidenceParams::from_toml_str(&wrong_schema).is_err());
    let verified_alone = TOML
        .replace("\"unverified\"", "\"verified\"")
        .replace("placeholder = true\n", "placeholder = false\n");
    assert!(ConfidenceParams::from_toml_str(&verified_alone).is_err());
    let missing = TOML.replace("err_pm_per_100m = 100\n", "");
    assert!(ConfidenceParams::from_toml_str(&missing).is_err());
}

#[test]
fn the_weights_are_placeholders_with_their_calibration_target() {
    let p = params();
    assert!(p.provenance.placeholder);
    assert_eq!(
        (
            p.provenance.calibration_spec,
            p.provenance.calibration_target
        ),
        (Some(15), Some(1))
    );
}
