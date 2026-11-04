//! Band widening and the conflict fixtures of SPEC-011 section 7.
//!
//! [`widen`] turns the wording of a source (`phrase`) and its stated figure into band edges by the
//! `CT-011-1101` rules; [`check_bands`] recomputes every edge of a band file. A conflict file
//! (`qc/fixtures/history-conflicts.toml`, rows CF-nn) names a disagreement in the record and the
//! rule the committed tables must obey; [`check_conflicts`] runs the rules and returns one message
//! per violation. Nothing here reads a file: callers pass text and the evidence tables.

use crate::{BandSet, Policy, ReplayScript, ScriptError};
use battle_rules::{parse_table, Confidence, DataError, Provenance, Table};
use std::collections::BTreeSet;

/// Largest figure accepted in a fixture row, keeping every product inside `i64`.
const VALUE_LIMIT: i64 = 1 << 40;

fn parse_err(msg: String) -> ScriptError {
    DataError::Parse(msg).into()
}

fn floor_div(a: i64, b: i64) -> i64 {
    a.div_euclid(b)
}

fn ceil_div(a: i64, b: i64) -> i64 {
    -((-a).div_euclid(b))
}

/// Widens a stated figure into `(low, high)` (`high` `None` is open above).
///
/// `phrase` is `exact`, `about`, `about_open`, `nearly` or `over` (each needs `source_value`), or
/// `range A B` or `range_open A` (which carry their figures and take no `source_value`). Exact,
/// `nearly` and `over` use `widen_exact_pm`, `about` forms `widen_approx_pm`:
/// exact is `[v*(1000-e)/1000 floor, v*(1000+e)/1000 ceil]`, about is the same with the approximate
/// width, `nearly` is `[v-e, v]`, `over` is `[v, v+e]`, `about_open` keeps the lower edge of
/// `about` and leaves the top open.
///
/// Errors: [`ScriptError::Data`] for an unknown phrase, a missing or surplus figure, or a range
/// whose edges are reversed.
pub fn widen(
    phrase: &str,
    source_value: Option<i64>,
    policy: &Policy,
) -> Result<(i64, Option<i64>), ScriptError> {
    let words: Vec<&str> = phrase.split_whitespace().collect();
    let kind = *words
        .first()
        .ok_or_else(|| parse_err("empty phrase".into()))?;
    let args = |n: usize| -> Result<Vec<i64>, ScriptError> {
        if words.len() != n + 1 {
            return Err(parse_err(format!("phrase {phrase:?} needs {n} figures")));
        }
        words[1..]
            .iter()
            .map(|w| {
                w.parse::<i64>()
                    .ok()
                    .filter(|v| (0..=VALUE_LIMIT).contains(v))
                    .ok_or_else(|| parse_err(format!("phrase {phrase:?}: bad figure {w:?}")))
            })
            .collect()
    };
    let need_value = || -> Result<i64, ScriptError> {
        args(0)?;
        source_value
            .filter(|v| (0..=VALUE_LIMIT).contains(v))
            .ok_or_else(|| parse_err(format!("phrase {phrase:?} needs a source_value")))
    };
    let (e, a) = (policy.widen_exact_pm, policy.widen_approx_pm);
    let down = |v: i64, pm: i64| floor_div(v * (1000 - pm), 1000);
    let up = |v: i64, pm: i64| ceil_div(v * (1000 + pm), 1000);
    match kind {
        "exact" => need_value().map(|v| (down(v, e), Some(up(v, e)))),
        "about" => need_value().map(|v| (down(v, a), Some(up(v, a)))),
        "about_open" => need_value().map(|v| (down(v, a), None)),
        "nearly" => need_value().map(|v| (down(v, e), Some(v))),
        "over" => need_value().map(|v| (v, Some(up(v, e)))),
        "range" | "range_open" => {
            if source_value.is_some() {
                return Err(parse_err(format!(
                    "phrase {phrase:?} takes no source_value"
                )));
            }
            if kind == "range" {
                let v = args(2)?;
                if v[0] > v[1] {
                    return Err(parse_err(format!("phrase {phrase:?}: edges reversed")));
                }
                Ok((v[0], Some(v[1])))
            } else {
                Ok((args(1)?[0], None))
            }
        }
        other => Err(parse_err(format!("unknown phrase {other:?}"))),
    }
}

/// Recomputes every band edge from its phrase and refuses a `verified` row; returns one message per
/// fault (empty when the file is sound).
pub fn check_bands(set: &BandSet, policy: &Policy) -> Vec<String> {
    let mut out = Vec::new();
    for b in set.bands() {
        if b.provenance.confidence == Confidence::Verified {
            out.push(format!("{}: verified without a second source", b.id));
        }
        match widen(&b.phrase, b.source_value, policy) {
            Ok(edges) if edges == (b.low, b.high) => {}
            Ok((low, high)) => out.push(format!(
                "{}: edges {}..{:?} but phrase {:?} gives {low}..{high:?}",
                b.id, b.low, b.high, b.phrase
            )),
            Err(e) => out.push(format!("{}: {e}", b.id)),
        }
    }
    out
}

/// The rule a conflict row enforces.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FixtureRule {
    /// The band covers every value.
    Envelope,
    /// The trigger's window covers every value.
    TimeWindow,
    /// No band edge is the difference of two values.
    NoSubtract,
    /// Known-bad values appear only in unverified rows.
    BadValue,
    /// The named history range holds both values.
    RecordRange,
    /// The two values bound a window, never a point.
    WindowOnly,
}

/// One conflict row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conflict {
    /// Row id (`CF-01`).
    pub id: String,
    /// What disagrees.
    pub subject: String,
    /// The rule the tables must obey.
    pub rule: FixtureRule,
    /// Band id, trigger id or range name the rule applies to.
    pub reference: Option<String>,
    /// A second band whose rate must contain the rate of `value_b` over `rate_base`.
    pub rate_ref: Option<String>,
    /// The strength the rate is taken over.
    pub rate_base: Option<i64>,
    /// The disagreeing figures, `value_a` first.
    pub values: Vec<i64>,
    /// Source and confidence.
    pub provenance: Provenance,
}

/// The rows of a conflict file in ascending id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConflictSet {
    conflicts: Vec<Conflict>,
}

fn read_conflict(mut t: Table) -> Result<Conflict, ScriptError> {
    let id = t.take_str("id")?;
    let subject = t.take_str("subject")?;
    let rule = match t.take_str("rule")?.as_str() {
        "envelope" => FixtureRule::Envelope,
        "time_window" => FixtureRule::TimeWindow,
        "no_subtract" => FixtureRule::NoSubtract,
        "bad_value" => FixtureRule::BadValue,
        "record_range" => FixtureRule::RecordRange,
        "window_only" => FixtureRule::WindowOnly,
        other => return Err(parse_err(format!("{id}: unknown rule {other:?}"))),
    };
    let reference = t.take_str_opt("ref")?;
    let rate_ref = t.take_str_opt("rate_ref")?;
    let rate_base = t.take_int_opt("rate_base")?;
    let mut values = vec![
        t.take_int_in("value_a", 0, VALUE_LIMIT)?,
        t.take_int_in("value_b", 0, VALUE_LIMIT)?,
    ];
    for key in ["value_c", "value_d"] {
        if let Some(v) = t.take_int_opt(key)? {
            values.push(v);
        }
    }
    let provenance = t.take_provenance()?;
    t.finish()?;
    if rate_ref.is_some() != rate_base.is_some() || rate_base.is_some_and(|b| b < 1) {
        return Err(parse_err(format!(
            "{id}: rate_ref and rate_base go together"
        )));
    }
    Ok(Conflict {
        id,
        subject,
        rule,
        reference,
        rate_ref,
        rate_base,
        values,
        provenance,
    })
}

impl ConflictSet {
    /// Reads a conflict file (`schema = "history-conflicts/1"`, rows `[[conflict]]`).
    ///
    /// Errors: [`ScriptError::Data`] for TOML, float, unknown key or rule, or provenance faults;
    /// [`ScriptError::Duplicate`] for a repeated id.
    pub fn from_toml_str(text: &str) -> Result<ConflictSet, ScriptError> {
        let mut root = parse_table(text)?;
        if root.take_str("schema")? != "history-conflicts/1" {
            return Err(parse_err("schema must be history-conflicts/1".into()));
        }
        let mut conflicts = Vec::new();
        for t in root.take_rows("conflict")? {
            conflicts.push(read_conflict(t)?);
        }
        root.finish()?;
        conflicts.sort_by(|a, b| a.id.cmp(&b.id));
        let mut seen = BTreeSet::new();
        for c in &conflicts {
            if !seen.insert(c.id.as_str()) {
                return Err(ScriptError::Duplicate(c.id.clone()));
            }
        }
        Ok(ConflictSet { conflicts })
    }

    /// Every row, ascending id.
    pub fn conflicts(&self) -> &[Conflict] {
        &self.conflicts
    }
}

/// The evidence tables a conflict row is checked against.
pub struct FixtureContext<'a> {
    /// The band file.
    pub bands: &'a BandSet,
    /// The replay script holding the timeline triggers.
    pub script: &'a ReplayScript,
    /// Integer values listed in `qc/fixtures/audit-known-errors.toml`.
    pub audit_values: &'a [i64],
    /// Named history ranges `(name, low, high)` for `record_range` rows.
    pub ranges: &'a [(String, i64, i64)],
}

fn bounds(values: &[i64]) -> (i64, i64) {
    let lo = values.iter().copied().min().unwrap_or(0);
    let hi = values.iter().copied().max().unwrap_or(0);
    (lo, hi)
}

fn check_one(c: &Conflict, ctx: &FixtureContext) -> Result<(), String> {
    let (lo, hi) = bounds(&c.values);
    let reference = || {
        c.reference
            .as_deref()
            .ok_or_else(|| "rule needs a ref".to_string())
    };
    let band = |id: &str| ctx.bands.get(id).ok_or_else(|| format!("no band {id}"));
    match c.rule {
        FixtureRule::Envelope => {
            let b = band(reference()?)?;
            if b.low > lo || b.high.is_some_and(|h| h < hi) {
                return Err(format!(
                    "{} {}..{:?} does not cover {lo} and {hi}",
                    b.id, b.low, b.high
                ));
            }
            if let (Some(r), Some(base)) = (&c.rate_ref, c.rate_base) {
                let rate = 1000 * c.values[1] / base;
                let rb = band(r)?;
                if rb.low > rate || rb.high.is_some_and(|h| h < rate) {
                    return Err(format!(
                        "{r} {}..{:?} excludes rate {rate}",
                        rb.low, rb.high
                    ));
                }
            }
        }
        FixtureRule::TimeWindow => {
            let id = reference()?;
            let t = ctx
                .script
                .triggers
                .iter()
                .find(|t| t.id == id)
                .ok_or_else(|| format!("no trigger {id}"))?;
            let from = t.early_s.map_or(t.at_s, |e| e.min(t.at_s));
            let to = t.late_s.map_or(t.at_s, |l| l.max(t.at_s));
            if from > lo || to < hi {
                return Err(format!(
                    "{id} window {from}..{to} does not cover {lo} and {hi}"
                ));
            }
        }
        FixtureRule::NoSubtract => {
            let b = band(reference()?)?;
            for (i, x) in c.values.iter().enumerate() {
                for y in &c.values[i + 1..] {
                    let d = (x - y).abs();
                    if b.low == d || b.high == Some(d) {
                        return Err(format!("{} has an edge equal to {x} - {y}", b.id));
                    }
                }
            }
        }
        FixtureRule::BadValue => {
            if let Some(v) = c.values.iter().find(|v| !ctx.audit_values.contains(v)) {
                return Err(format!("{v} is not in the known-error list"));
            }
            for b in ctx.bands.bands() {
                let touches = [Some(b.low), b.high, b.source_value]
                    .into_iter()
                    .flatten()
                    .any(|n| c.values.contains(&n));
                if touches && b.gates() {
                    return Err(format!("{} carries a known-bad value in a gated row", b.id));
                }
            }
        }
        FixtureRule::RecordRange => {
            let name = reference()?;
            let (_, rlo, rhi) = ctx
                .ranges
                .iter()
                .find(|(n, _, _)| n == name)
                .ok_or_else(|| format!("no range {name}"))?;
            if (*rlo, *rhi) != (lo, hi) {
                return Err(format!("{name} holds {rlo}..{rhi}, not {lo}..{hi}"));
            }
        }
        FixtureRule::WindowOnly => {
            if c.values[0] >= c.values[1] {
                return Err(format!(
                    "{} to {} is a point, not a window",
                    c.values[0], c.values[1]
                ));
            }
        }
    }
    Ok(())
}

/// Runs every conflict row; each violation is `"<id>: <reason>"`. Empty means every rule holds.
pub fn check_conflicts(set: &ConflictSet, ctx: &FixtureContext) -> Vec<String> {
    set.conflicts()
        .iter()
        .filter_map(|c| check_one(c, ctx).err().map(|e| format!("{}: {e}", c.id)))
        .collect()
}
