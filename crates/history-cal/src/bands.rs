//! The band file `qc/calibration/<battle>-bands.toml` (SPEC-011 section 3): evidenced ranges for
//! measured quantities. Whether a band gates is derived from its confidence, never stored.

use crate::ScriptError;
use battle_rules::{parse_table, Confidence, DataError, Provenance, Table};
use std::collections::BTreeSet;

/// Largest band edge, keeping every product of the evaluator inside `i64`.
const EDGE_LIMIT: i64 = 1 << 40;

/// What a band measures over the unit's regiments (SPEC-011 section 4.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Metric {
    /// Casualty records in the window.
    Count,
    /// Permille of the unit's strength.
    RatePm,
    /// Lower median over the unit's regiments of each regiment's permille rate.
    RatePmMedian,
}

/// Which casualty kinds a band counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Basis {
    /// Killed and wounded.
    Kw,
    /// Killed, wounded and captured.
    Total,
}

/// A rule the simulation must implement before the band can be measured.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Need {
    /// Captured soldiers are recorded.
    Capture,
    /// Morale and rout are modelled.
    Morale,
    /// Melee is modelled.
    Melee,
}

/// One band row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Band {
    /// Row id (`HB-01`).
    pub id: String,
    /// Scenario id, or `any`.
    pub scenario: String,
    /// Unit id the measure is taken over.
    pub unit: String,
    /// Measured quantity.
    pub metric: Metric,
    /// Casualty kinds counted.
    pub basis: Basis,
    /// Window start, clock seconds.
    pub from_s: i64,
    /// Window end (exclusive), clock seconds.
    pub to_s: i64,
    /// Lower edge, a count or permille by the metric.
    pub low: i64,
    /// Upper edge; `None` is open above.
    pub high: Option<i64>,
    /// The source's wording, from which `low` and `high` are widened.
    pub phrase: String,
    /// The figure the source states, when it states one.
    pub source_value: Option<i64>,
    /// The rule the band waits for.
    pub needs: Option<Need>,
    /// Source and confidence.
    pub provenance: Provenance,
}

impl Band {
    /// True when the band can fail a run: confidence `partial` or `verified`.
    pub fn gates(&self) -> bool {
        self.provenance.confidence != Confidence::Unverified
    }
}

/// The rows of one band file in ascending id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BandSet {
    bands: Vec<Band>,
}

fn bad(t: &Table, key: &str, v: &str) -> ScriptError {
    DataError::Parse(format!("{}.{key}: unknown value {v:?}", t.path())).into()
}

fn ident(t: &Table, key: &str, v: &str, extra: &[u8], upper: bool) -> Result<(), ScriptError> {
    let ok = !v.is_empty()
        && v.bytes().all(|b| {
            matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'_')
                || extra.contains(&b)
                || (upper && b.is_ascii_uppercase())
        });
    if ok {
        Ok(())
    } else {
        Err(DataError::Parse(format!("{}.{key} {v:?} is not a valid id", t.path())).into())
    }
}

fn read_band(mut t: Table) -> Result<Band, ScriptError> {
    let id = t.take_str("id")?;
    ident(&t, "id", &id, b"-", true)?;
    let scenario = t.take_str("scenario")?;
    ident(&t, "scenario", &scenario, b"", false)?;
    let unit = t.take_str("unit")?;
    ident(&t, "unit", &unit, b"", false)?;
    let metric = match t.take_str("metric")?.as_str() {
        "count" => Metric::Count,
        "rate_pm" => Metric::RatePm,
        "rate_pm_median" => Metric::RatePmMedian,
        other => return Err(bad(&t, "metric", other)),
    };
    let basis = match t.take_str("basis")?.as_str() {
        "kw" => Basis::Kw,
        "total" => Basis::Total,
        other => return Err(bad(&t, "basis", other)),
    };
    let from_s = t.take_int_in("from_s", 0, EDGE_LIMIT)?;
    let to_s = t.take_int_in("to_s", 0, EDGE_LIMIT)?;
    let low = t.take_int_in("low", 0, EDGE_LIMIT)?;
    let high = t.take_int_opt("high")?;
    let phrase = t.take_str("phrase")?;
    let source_value = t.take_int_opt("source_value")?;
    let needs = match t.take_str_opt("needs")?.as_deref() {
        None => None,
        Some("capture") => Some(Need::Capture),
        Some("morale") => Some(Need::Morale),
        Some("melee") => Some(Need::Melee),
        Some(other) => return Err(bad(&t, "needs", other)),
    };
    let provenance = t.take_provenance()?;
    t.finish()?;
    if from_s >= to_s {
        return Err(ScriptError::Time(format!(
            "band {id}: window {from_s}..{to_s}"
        )));
    }
    if let Some(h) = high {
        if !(low..=EDGE_LIMIT).contains(&h) {
            return Err(DataError::Range(format!("band {id}: high {h} below low {low}")).into());
        }
    }
    Ok(Band {
        id,
        scenario,
        unit,
        metric,
        basis,
        from_s,
        to_s,
        low,
        high,
        phrase,
        source_value,
        needs,
        provenance,
    })
}

impl BandSet {
    /// Reads a band file (`schema = "history-bands/1"`, rows `[[band]]`).
    ///
    /// Errors: [`ScriptError::Data`] for TOML, float, unknown key or value, provenance or range
    /// faults (a placeholder needs a `CT-<spec>-<n>`, any other row a source, `verified` a
    /// `second_source`); [`ScriptError::Time`] for an empty window; [`ScriptError::Duplicate`] for
    /// a repeated id.
    pub fn from_toml_str(text: &str) -> Result<BandSet, ScriptError> {
        let mut root = parse_table(text)?;
        if root.take_str("schema")? != "history-bands/1" {
            return Err(DataError::Parse("schema must be history-bands/1".into()).into());
        }
        let mut bands = Vec::new();
        for t in root.take_rows("band")? {
            bands.push(read_band(t)?);
        }
        root.finish()?;
        bands.sort_by(|a, b| a.id.cmp(&b.id));
        let mut seen = BTreeSet::new();
        for b in &bands {
            if !seen.insert(b.id.as_str()) {
                return Err(ScriptError::Duplicate(b.id.clone()));
            }
        }
        Ok(BandSet { bands })
    }

    /// Every band, ascending id.
    pub fn bands(&self) -> &[Band] {
        &self.bands
    }

    /// The band called `id`.
    pub fn get(&self, id: &str) -> Option<&Band> {
        self.bands
            .binary_search_by(|b| b.id.as_str().cmp(id))
            .ok()
            .map(|i| &self.bands[i])
    }

    /// The bands that run in `scenario`: its own and the `any` rows, ascending id.
    pub fn for_scenario<'a>(&'a self, scenario: &'a str) -> impl Iterator<Item = &'a Band> + 'a {
        self.bands
            .iter()
            .filter(move |b| b.scenario == scenario || b.scenario == "any")
    }
}
