//! The score of a sample and the aggregate over samples (SPEC-010 section 7), by hand.

use battle_ai::{aggregate_scores, score_sample, ForecastResult};

fn r(
    own_lost: u64,
    enemy_lost: u64,
    ctrl_pm: u16,
    own_start: u64,
    enemy_start: u64,
) -> ForecastResult {
    ForecastResult {
        own_lost,
        enemy_lost,
        ctrl_pm,
        final_hash: 0,
        own_start,
        enemy_start,
    }
}

#[test]
fn a_sample_scores_control_plus_kills_minus_risk_weighted_losses() {
    // 600 + 250 - 100 * (2000 - 500) / 1000 = 700.
    assert_eq!(score_sample(&r(40, 100, 600, 400, 400), 500), 700);
    // Risk tolerance 1000 weighs losses once: 600 + 250 - 100 = 750.
    assert_eq!(score_sample(&r(40, 100, 600, 400, 400), 1000), 750);
    // Risk tolerance 0 doubles them: 600 + 250 - 200 = 650.
    assert_eq!(score_sample(&r(40, 100, 600, 400, 400), 0), 650);
    // Each division truncates: 7 * 1000 / 300 = 23, 23 * 1500 / 1000 = 34.
    assert_eq!(score_sample(&r(7, 0, 0, 300, 300), 500), -34);
    assert_eq!(score_sample(&r(0, 0, 0, 0, 0), 500), 0);
}

#[test]
fn the_aggregate_leans_on_the_worst_sample_as_caution_rises() {
    let s = [100, 400, 700];
    assert_eq!(
        aggregate_scores(&s, 1000),
        400,
        "full tolerance is the mean"
    );
    assert_eq!(aggregate_scores(&s, 0), 100, "no tolerance is the minimum");
    // mean 400, min 100: 400 - 300 * 500 / 1000 = 250.
    assert_eq!(aggregate_scores(&s, 500), 250);
    // Truncation toward zero: mean -1, min -2, -1 - (1 * 999) / 1000 = -1.
    assert_eq!(aggregate_scores(&[-2, 0], 1), -1);
    assert_eq!(aggregate_scores(&[], 500), 0);
}
