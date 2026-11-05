//! Confidence shading of enemy markers (SPEC-015 section 4): a score in permille from the age of
//! the last sighting and the position error, and the four bands it falls into.

use battle_rules::{parse_table, DataError, Provenance, Table, TICK_HZ};

/// Weights of the score, from `data/ui/confidence.toml`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfidenceParams {
    /// Permille lost per second of age.
    pub age_pm_per_s: u32,
    /// Permille lost per 100 m of position error.
    pub err_pm_per_100m: u32,
    /// Provenance of the weights.
    pub provenance: Provenance,
}

impl ConfidenceParams {
    /// Parses `confidence.toml` text.
    ///
    /// Errors: the shared reader's [`DataError`] kinds for floats, unknown or missing keys, a
    /// wrong `schema` or broken provenance, and [`DataError::Range`] for a weight above 1000.
    pub fn from_toml_str(text: &str) -> Result<ConfidenceParams, DataError> {
        let mut root = parse_table(text)?;
        check_schema(&mut root, "ui-confidence/1")?;
        let mut t = root.take_table("params")?;
        let age_pm_per_s = take_u32(&mut t, "age_pm_per_s", 0, 1000)?;
        let err_pm_per_100m = take_u32(&mut t, "err_pm_per_100m", 0, 1000)?;
        let provenance = t.take_provenance()?;
        t.finish()?;
        root.finish()?;
        Ok(ConfidenceParams {
            age_pm_per_s,
            err_pm_per_100m,
            provenance,
        })
    }
}

/// How firmly headquarters holds a sighting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfBand {
    /// At least 750: solid outline, opaque.
    Firm,
    /// At least 500: dashed, 75 percent.
    Probable,
    /// At least 250: dotted, 50 percent, with the uncertainty circle.
    Possible,
    /// Below 250: a hollow ghost labelled with the last-seen clock time.
    Stale,
}

const FIRM_PM: u16 = 750;
const PROBABLE_PM: u16 = 500;
const POSSIBLE_PM: u16 = 250;

/// `max(0, 1000 - age_s * age_pm_per_s - err_mm * err_pm_per_100m / 100000)` with `age_s` the
/// age in whole seconds. Never rises as the age or the error grows.
pub fn confidence_pm(age_ticks: u64, err_mm: u32, p: &ConfidenceParams) -> u16 {
    let age_s = age_ticks / u64::from(TICK_HZ);
    let by_age = age_s.saturating_mul(u64::from(p.age_pm_per_s));
    let by_err = u64::from(err_mm) * u64::from(p.err_pm_per_100m) / 100_000;
    let lost = by_age.saturating_add(by_err);
    u16::try_from(1000u64.saturating_sub(lost)).unwrap_or(0)
}

/// The band of a score.
pub fn confidence_band(pm: u16) -> ConfBand {
    if pm >= FIRM_PM {
        ConfBand::Firm
    } else if pm >= PROBABLE_PM {
        ConfBand::Probable
    } else if pm >= POSSIBLE_PM {
        ConfBand::Possible
    } else {
        ConfBand::Stale
    }
}

// Helpers shared by the parameter loaders of `data/ui` (schema key, bounded integers).

/// Consumes the `schema` key and requires it to equal `want`.
pub(crate) fn check_schema(root: &mut Table, want: &str) -> Result<(), DataError> {
    let got = root.take_str("schema")?;
    if got == want {
        Ok(())
    } else {
        Err(DataError::Parse(format!(
            "schema must be \"{want}\", found \"{got}\""
        )))
    }
}

/// An integer in `lo..=hi` as `u32`.
pub(crate) fn take_u32(t: &mut Table, key: &str, lo: u32, hi: u32) -> Result<u32, DataError> {
    let v = t.take_int_in(key, i64::from(lo), i64::from(hi))?;
    u32::try_from(v).map_err(|_| DataError::Range(format!("{}.{key} out of range", t.path())))
}
