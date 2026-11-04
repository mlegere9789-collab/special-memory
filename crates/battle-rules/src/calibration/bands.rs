//! The band file (`qc/calibration/bands.toml`): integer-only rows with provenance.
//!
//! A band names a metric, the scenario parameters it is measured with and one or more points
//! `{at_mm | training | ammo, low, high}`. Values are the real number times the band's `scale`.

use crate::{parse_table, DataError, Provenance};

/// One measured point of a band.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Point {
    /// Distance in millimetres, 0 when the point is not a distance.
    pub at_mm: u32,
    /// Training level name for a rate point, else empty.
    pub training: String,
    /// Ammunition row for a range point, else empty.
    pub ammo: String,
    /// Elevation in hundredths of a degree for a range point, else 0.
    pub elevation_cdeg: u32,
    /// Inclusive lower limit, scaled.
    pub low: Option<i64>,
    /// Inclusive upper limit, scaled.
    pub high: Option<i64>,
    /// Upper limit as permille of the first point's measured mean, instead of `high`.
    pub high_of_first_pm: Option<i64>,
}

/// One band row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Band {
    /// Id `B1` to `B11`.
    pub id: String,
    /// Family: `fire` for SPEC-005.
    pub family: String,
    /// Metric name the harness dispatches on.
    pub metric: String,
    /// Weapon row the metric is measured with; for `monotone`, the first band id to check.
    pub weapon: String,
    /// Second weapon (the denominator of a ratio); for `monotone`, the second band id.
    pub other: String,
    /// Multiplier from the real quantity to the integers in `points`.
    pub scale: i64,
    /// The points.
    pub points: Vec<Point>,
    /// Confidence, placeholder flag and sources.
    pub provenance: Provenance,
}

/// Parses the band file text.
///
/// Errors: [`DataError`] for malformed text, floats, unknown keys or missing provenance.
pub fn parse_bands(text: &str) -> Result<Vec<Band>, DataError> {
    let mut root = parse_table(text)?;
    if root.take_str("schema")? != "bands/1" {
        return Err(DataError::Parse("unknown bands schema".into()));
    }
    let mut out = Vec::new();
    for mut row in root.take_rows("band")? {
        let id = row.take_str("id")?;
        let family = row.take_str("family")?;
        let metric = row.take_str("metric")?;
        let weapon = row.take_str_opt("weapon")?.unwrap_or_default();
        let other = row.take_str_opt("other")?.unwrap_or_default();
        let scale = row.take_int_in("scale", 1, 1_000_000)?;
        let mut points = Vec::new();
        for mut p in row.take_rows_opt("points")?.unwrap_or_default() {
            points.push(Point {
                at_mm: u32::try_from(p.take_int_opt("at_mm")?.unwrap_or(0))
                    .map_err(|_| DataError::Range("at_mm".into()))?,
                training: p.take_str_opt("training")?.unwrap_or_default(),
                ammo: p.take_str_opt("ammo")?.unwrap_or_default(),
                elevation_cdeg: u32::try_from(p.take_int_opt("elevation_cdeg")?.unwrap_or(0))
                    .map_err(|_| DataError::Range("elevation_cdeg".into()))?,
                low: p.take_int_opt("low")?,
                high: p.take_int_opt("high")?,
                high_of_first_pm: p.take_int_opt("high_of_first_pm")?,
            });
            p.finish()?;
        }
        let provenance = row.take_provenance()?;
        row.finish()?;
        out.push(Band {
            id,
            family,
            metric,
            weapon,
            other,
            scale,
            points,
            provenance,
        });
    }
    root.finish()?;
    Ok(out)
}
