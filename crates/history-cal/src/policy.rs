//! The calibration policy table `qc/calibration/policy.toml` (SPEC-011 section 9).

use crate::ScriptError;
use battle_rules::{parse_table, DataError};
use std::collections::BTreeMap;

/// Every threshold id and the number `n` of the `CT-011-n` target that owns it.
const ROWS: [(&str, u32); 23] = [
    ("widen_exact_pm", 1101),
    ("widen_approx_pm", 1101),
    ("approx_time_s", 1101),
    ("weak_width_pm", 1101),
    ("spread_limit_pm", 1102),
    ("seeds_smoke_lo", 1102),
    ("seeds_smoke_hi", 1102),
    ("seeds_full_lo", 1102),
    ("seeds_full_hi", 1102),
    ("seeds_fit_lo", 1102),
    ("seeds_fit_hi", 1102),
    ("seeds_holdout_lo", 1102),
    ("seeds_holdout_hi", 1102),
    ("trend_alert_pm", 1103),
    ("holdout_pass_pm", 1104),
    ("holdout_loss_factor", 1104),
    ("holdout_looks_per_cycle", 1104),
    ("fit_rounds", 1105),
    ("fit_grid_max_points", 1105),
    ("fit_excess_cap_pm", 1105),
    ("fit_accept_factor_pm", 1105),
    ("fit_accept_slack_per_band", 1105),
    ("fit_free_param_divisor", 1105),
];

/// The thresholds of section 9, one typed field each; seed lists are inclusive `(first, last)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Policy {
    /// Widening of an exact stated count, permille either side (CT-011-1101).
    pub widen_exact_pm: i64,
    /// Widening of an "about" figure, permille either side.
    pub widen_approx_pm: i64,
    /// Half-width of a "~T" time window, seconds.
    pub approx_time_s: i64,
    /// Width above which a gated band is flagged weak, permille.
    pub weak_width_pm: i64,
    /// Seed spread above which a band is unstable, permille (CT-011-1102).
    pub spread_limit_pm: i64,
    /// Seeds of the pull-request suite.
    pub seeds_smoke: (u64, u64),
    /// Seeds of the nightly suite.
    pub seeds_full: (u64, u64),
    /// Seeds of a fit.
    pub seeds_fit: (u64, u64),
    /// Seeds of the hold-out set.
    pub seeds_holdout: (u64, u64),
    /// Drift against the baseline that is reported, permille (CT-011-1103).
    pub trend_alert_pm: i64,
    /// Share of gated hold-out bands that must pass, permille (CT-011-1104).
    pub holdout_pass_pm: i64,
    /// Hold-out loss limit as a multiple of the fit loss.
    pub holdout_loss_factor: i64,
    /// Hold-out looks allowed per cycle.
    pub holdout_looks_per_cycle: i64,
    /// Coordinate-descent rounds (CT-011-1105).
    pub fit_rounds: i64,
    /// Most grid points tried for one tunable.
    pub fit_grid_max_points: i64,
    /// Cap on one band's excess inside the loss, permille.
    pub fit_excess_cap_pm: i64,
    /// Accepted-range loss limit as a multiple of the minimum loss, permille.
    pub fit_accept_factor_pm: i64,
    /// Accepted-range slack added per gated fit band.
    pub fit_accept_slack_per_band: i64,
    /// Free parameters allowed are the gated fit bands divided by this.
    pub fit_free_param_divisor: i64,
}

impl Policy {
    /// Reads `policy.toml` (`schema = "calibration-policy/1"`, rows `[[threshold]]` with `id`,
    /// `value` and provenance).
    ///
    /// Errors: [`ScriptError::Data`] for TOML, float, provenance or range faults, an unknown id
    /// (`UnknownKey`), a missing id, a row that is not a placeholder or whose target is not the
    /// `CT-011-n` assigned to its id; [`ScriptError::Duplicate`] for a repeated id.
    pub fn from_toml_str(text: &str) -> Result<Policy, ScriptError> {
        let mut root = parse_table(text)?;
        if root.take_str("schema")? != "calibration-policy/1" {
            return Err(DataError::Parse("schema must be calibration-policy/1".into()).into());
        }
        let mut values: BTreeMap<String, i64> = BTreeMap::new();
        for mut t in root.take_rows("threshold")? {
            let id = t.take_str("id")?;
            let value = t.take_int_in("value", 0, 1 << 40)?;
            let prov = t.take_provenance()?;
            t.finish()?;
            let want = ROWS
                .iter()
                .find(|(r, _)| *r == id)
                .ok_or_else(|| DataError::UnknownKey(id.clone()))?
                .1;
            let ours = prov.calibration_spec == Some(11) && prov.calibration_target == Some(want);
            if !prov.placeholder || !ours {
                return Err(DataError::Provenance(format!(
                    "{id}: must be a placeholder of CT-011-{want}"
                ))
                .into());
            }
            if values.insert(id.clone(), value).is_some() {
                return Err(ScriptError::Duplicate(id));
            }
        }
        root.finish()?;
        if let Some((id, _)) = ROWS.iter().find(|(id, _)| !values.contains_key(*id)) {
            return Err(DataError::Parse(format!("missing threshold {id}")).into());
        }
        let v = |id: &str| values[id];
        let seeds = |lo: &str, hi: &str| -> Result<(u64, u64), ScriptError> {
            let (a, b) = (v(lo), v(hi));
            if a < 1 || a > b {
                return Err(DataError::Range(format!("seed list {lo}..{hi} is {a}..{b}")).into());
            }
            Ok((a as u64, b as u64))
        };
        Ok(Policy {
            widen_exact_pm: v("widen_exact_pm"),
            widen_approx_pm: v("widen_approx_pm"),
            approx_time_s: v("approx_time_s"),
            weak_width_pm: v("weak_width_pm"),
            spread_limit_pm: v("spread_limit_pm"),
            seeds_smoke: seeds("seeds_smoke_lo", "seeds_smoke_hi")?,
            seeds_full: seeds("seeds_full_lo", "seeds_full_hi")?,
            seeds_fit: seeds("seeds_fit_lo", "seeds_fit_hi")?,
            seeds_holdout: seeds("seeds_holdout_lo", "seeds_holdout_hi")?,
            trend_alert_pm: v("trend_alert_pm"),
            holdout_pass_pm: v("holdout_pass_pm"),
            holdout_loss_factor: v("holdout_loss_factor"),
            holdout_looks_per_cycle: v("holdout_looks_per_cycle"),
            fit_rounds: v("fit_rounds"),
            fit_grid_max_points: v("fit_grid_max_points"),
            fit_excess_cap_pm: v("fit_excess_cap_pm"),
            fit_accept_factor_pm: v("fit_accept_factor_pm"),
            fit_accept_slack_per_band: v("fit_accept_slack_per_band"),
            fit_free_param_divisor: v("fit_free_param_divisor"),
        })
    }
}
