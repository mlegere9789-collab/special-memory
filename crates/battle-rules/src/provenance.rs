//! Provenance of a rules row (SPEC-004 section 2): the loader's twin of
//! `data/schemas/rules-common.schema.json`.

use crate::{DataError, Table};

/// How far a number has been confirmed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Confidence {
    /// Confirmed by a source and a second, independent source.
    Verified,
    /// Partly confirmed.
    Partial,
    /// Not confirmed.
    Unverified,
}

/// A page reference into a research document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceRef {
    /// Short code of the document.
    pub document: String,
    /// Page number, at least 1.
    pub page: u32,
    /// The tag the source itself used, verbatim.
    pub tag: Option<String>,
    /// Free note.
    pub note: Option<String>,
}

/// Flattened provenance fields of a data row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Provenance {
    /// Confidence level.
    pub confidence: Confidence,
    /// True for an invented number awaiting calibration.
    pub placeholder: bool,
    /// The `n` of `CT-n`; present for every placeholder.
    pub calibration_target: Option<u32>,
    /// Source pages; non-empty unless the row is a placeholder.
    pub sources: Vec<SourceRef>,
    /// Independent confirmation; required for `Verified`.
    pub second_source: Option<String>,
    /// Free note.
    pub note: Option<String>,
}

fn bad(row: &Table, msg: &str) -> DataError {
    let at = if row.path().is_empty() {
        "(root)"
    } else {
        row.path()
    };
    DataError::Provenance(format!("{at}: {msg}"))
}

fn parse_target(s: &str) -> Option<u32> {
    let digits = s.strip_prefix("CT-")?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

impl Table {
    /// Removes the flattened provenance keys of a data row and enforces the rules: a placeholder
    /// needs a `calibration_target` (`CT-n`); any other row needs at least one source; `verified`
    /// needs a non-empty `second_source`. Unknown keys inside a source are refused here.
    pub fn take_provenance(&mut self) -> Result<Provenance, DataError> {
        let confidence = match self.take_str_opt("confidence")?.as_deref() {
            Some("verified") => Confidence::Verified,
            Some("partial") => Confidence::Partial,
            Some("unverified") => Confidence::Unverified,
            Some(other) => return Err(bad(self, &format!("unknown confidence {other:?}"))),
            None => return Err(bad(self, "missing confidence")),
        };
        let placeholder = self.take_bool_opt("placeholder")?.unwrap_or(false);
        let target_text = self.take_str_opt("calibration_target")?;
        let calibration_target = match target_text.as_deref().map(parse_target) {
            Some(Some(n)) => Some(n),
            Some(None) => return Err(bad(self, "calibration_target must look like CT-3")),
            None => None,
        };
        let mut sources = Vec::new();
        for mut s in self.take_rows_opt("sources")?.unwrap_or_default() {
            let document = s.take_str("document")?;
            let page = u32::try_from(s.take_int_in("page", 1, i64::from(u32::MAX))?).unwrap_or(1);
            let tag = s.take_str_opt("tag")?;
            let note = s.take_str_opt("note")?;
            s.finish()?;
            sources.push(SourceRef {
                document,
                page,
                tag,
                note,
            });
        }
        let second_source = self.take_str_opt("second_source")?;
        let note = self.take_str_opt("note")?;
        if placeholder && calibration_target.is_none() {
            return Err(bad(self, "placeholder without calibration_target"));
        }
        if !placeholder && sources.is_empty() {
            return Err(bad(self, "non-placeholder without sources"));
        }
        let has_second = second_source
            .as_deref()
            .is_some_and(|s| !s.trim().is_empty());
        if confidence == Confidence::Verified && !has_second {
            return Err(bad(self, "verified without second_source"));
        }
        Ok(Provenance {
            confidence,
            placeholder,
            calibration_target,
            sources,
            second_source,
            note,
        })
    }
}
