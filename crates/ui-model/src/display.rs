//! The display-field registry (SPEC-015 section 14, T15-02): every number the interface shows is
//! listed in `data/ui/display-fields.toml` with its simulation source and how it is shown.

use battle_rules::{parse_table, DataError};
use std::collections::BTreeSet;

/// How a displayed value relates to the simulation value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldKind {
    /// The simulation value itself.
    Exact,
    /// A quantisation printed in the tooltip.
    Band,
    /// A range that brackets the truth.
    Range,
}

/// One row of the registry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DisplayField {
    /// `Struct.field` in the snapshot or a view model.
    pub name: String,
    /// How it is shown.
    pub kind: FieldKind,
    /// The simulation value it comes from.
    pub source: String,
}

/// Parses the registry. A row without a name or source, an unknown kind or a repeated name is
/// refused.
pub fn display_fields(text: &str) -> Result<Vec<DisplayField>, DataError> {
    let mut root = parse_table(text)?;
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for mut row in root.take_rows("field")? {
        let name = row.take_str("name")?;
        let source = row.take_str("source")?;
        let kind = match row.take_str("kind")?.as_str() {
            "exact" => FieldKind::Exact,
            "band" => FieldKind::Band,
            "range" => FieldKind::Range,
            other => return Err(DataError::Parse(format!("unknown kind {other} for {name}"))),
        };
        row.finish()?;
        if name.is_empty() || source.trim().is_empty() {
            return Err(DataError::Parse(format!(
                "field {name:?} needs a name and a source"
            )));
        }
        if !seen.insert(name.clone()) {
            return Err(DataError::Parse(format!("field {name} is listed twice")));
        }
        out.push(DisplayField { name, kind, source });
    }
    root.finish()?;
    Ok(out)
}
