//! T11-01, policy half: `qc/calibration/policy.toml` loads, every row is a placeholder naming its
//! own calibration target (SPEC-011 section 9).

use battle_rules::DataError;
use history_cal::{Policy, ScriptError};

const POLICY: &str = include_str!("../../../qc/calibration/policy.toml");

#[test]
fn policy_file_loads_with_section_9_values() {
    let p = Policy::from_toml_str(POLICY).unwrap();
    assert_eq!(
        (
            p.widen_exact_pm,
            p.widen_approx_pm,
            p.approx_time_s,
            p.weak_width_pm
        ),
        (150, 200, 900, 1500)
    );
    assert_eq!(
        (p.spread_limit_pm, p.trend_alert_pm, p.holdout_pass_pm),
        (400, 150, 700)
    );
    assert_eq!(
        (
            p.holdout_loss_factor,
            p.holdout_looks_per_cycle,
            p.fit_rounds
        ),
        (2, 3, 3)
    );
    assert_eq!(
        (p.seeds_smoke, p.seeds_full, p.seeds_fit, p.seeds_holdout),
        ((1, 4), (1, 16), (1, 8), (101, 116))
    );
    assert_eq!((p.fit_grid_max_points, p.fit_excess_cap_pm), (33, 2000));
    assert_eq!(
        (
            p.fit_accept_factor_pm,
            p.fit_accept_slack_per_band,
            p.fit_free_param_divisor
        ),
        (1100, 100, 3)
    );
}

fn data_err(text: &str) -> DataError {
    match Policy::from_toml_str(text) {
        Err(ScriptError::Data(e)) => e,
        other => panic!("expected a data error, got {other:?}"),
    }
}

#[test]
fn bad_policies_are_refused() {
    assert!(matches!(
        data_err(&POLICY.replace("value = 900", "value = 900.5")),
        DataError::Float(_)
    ));
    let unknown = POLICY.replace("id = \"fit_rounds\"", "id = \"fit_roundz\"");
    assert!(matches!(data_err(&unknown), DataError::UnknownKey(_)));
    let wrong_ct = POLICY.replacen("CT-011-1101", "CT-011-1105", 1);
    assert!(matches!(data_err(&wrong_ct), DataError::Provenance(_)));
    let dup = POLICY.replace("id = \"fit_rounds\"", "id = \"trend_alert_pm\"");
    assert!(Policy::from_toml_str(&dup).is_err());
    let missing = POLICY.replace("id = \"fit_rounds\"", "id = \"\"");
    assert!(Policy::from_toml_str(&missing).is_err());
}
