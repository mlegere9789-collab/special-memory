//! Morale feedback (SPEC-015 section 6): the level with its symptoms, the early warning and the
//! three largest causes, each banded. Values are permille of the SPEC-006 ceiling (ADR 0010, D-04).

use crate::confidence::{check_schema, take_u32};
use crate::source::MoraleReport;
use battle_rules::{parse_table, DataError, Provenance};
use std::collections::BTreeSet;

/// What one morale level looks like, from `data/ui/symptoms.toml`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LevelInfo {
    /// String-table key of the level's word.
    pub word: String,
    /// Icon shape id.
    pub icon: u8,
    /// Fire is slowed (the figure is the simulation's loading multiplier).
    pub fire_slowed: bool,
    /// Men are looking back.
    pub looking_back: bool,
    /// The ranks are thinning.
    pub ranks_thinning: bool,
}

/// Thresholds and per-level symptoms, from `data/ui/morale.toml` and `data/ui/symptoms.toml`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MoraleViewParams {
    /// A cause at or above this is Large.
    pub large_pm: u32,
    /// A cause at or above this is Moderate.
    pub moderate_pm: u32,
    /// A wavering regiment closer than this to the next worse level raises the warning.
    pub warn_margin_pm: u32,
    levels: [LevelInfo; 5],
    /// Provenance of the thresholds.
    pub provenance: Provenance,
}

impl MoraleViewParams {
    /// Parses both files. Errors: the shared reader's [`DataError`] kinds, and
    /// [`DataError::Range`] when `moderate_pm` exceeds `large_pm`, a level is missing, repeated
    /// or above 4, or two levels share an icon or a symptom set.
    pub fn from_toml_str(morale: &str, symptoms: &str) -> Result<MoraleViewParams, DataError> {
        let mut root = parse_table(morale)?;
        check_schema(&mut root, "ui-morale/1")?;
        let mut t = root.take_table("params")?;
        let large_pm = take_u32(&mut t, "large_pm", 1, 1000)?;
        let moderate_pm = take_u32(&mut t, "moderate_pm", 1, 1000)?;
        let warn_margin_pm = take_u32(&mut t, "warn_margin_pm", 0, 1000)?;
        let provenance = t.take_provenance()?;
        t.finish()?;
        root.finish()?;
        if moderate_pm > large_pm {
            return Err(DataError::Range("moderate_pm exceeds large_pm".to_owned()));
        }
        Ok(MoraleViewParams {
            large_pm,
            moderate_pm,
            warn_margin_pm,
            levels: load_levels(symptoms)?,
            provenance,
        })
    }

    /// The word, icon and symptoms of `level`, `None` above 4.
    pub fn level_info(&self, level: u8) -> Option<&LevelInfo> {
        self.levels.get(usize::from(level))
    }
}

fn load_levels(text: &str) -> Result<[LevelInfo; 5], DataError> {
    let mut root = parse_table(text)?;
    check_schema(&mut root, "ui-symptoms/1")?;
    let mut slots: [Option<LevelInfo>; 5] = [None, None, None, None, None];
    for mut row in root.take_rows("level")? {
        let id = take_u32(&mut row, "id", 0, 4)?;
        let info = LevelInfo {
            word: row.take_str("word")?,
            icon: u8::try_from(take_u32(&mut row, "icon", 0, 255)?).unwrap_or(0),
            fire_slowed: row.take_bool("fire_slowed")?,
            looking_back: row.take_bool("looking_back")?,
            ranks_thinning: row.take_bool("ranks_thinning")?,
        };
        row.take_provenance()?;
        let at = row.path().to_owned();
        row.finish()?;
        let slot = &mut slots[id as usize];
        if slot.replace(info).is_some() {
            return Err(DataError::Range(format!(
                "{at}: level {id} is listed twice"
            )));
        }
    }
    root.finish()?;
    let [Some(a), Some(b), Some(c), Some(d), Some(e)] = slots else {
        return Err(DataError::Range(
            "levels 0 to 4 must each be listed".to_owned(),
        ));
    };
    let levels = [a, b, c, d, e];
    let icons: BTreeSet<u8> = levels.iter().map(|l| l.icon).collect();
    let sets: BTreeSet<(bool, bool, bool)> = levels
        .iter()
        .map(|l| (l.fire_slowed, l.looking_back, l.ranks_thinning))
        .collect();
    if icons.len() != 5 || sets.len() != 5 {
        return Err(DataError::Range(
            "levels must differ in icon and in symptom set".to_owned(),
        ));
    }
    Ok(levels)
}

/// How much a cause weighs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FactorBand {
    /// At least `large_pm`.
    Large,
    /// At least `moderate_pm`.
    Moderate,
    /// Below `moderate_pm`.
    Slight,
}

/// A regiment's morale as shown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MoraleView {
    /// 0 steady to 4 broken.
    pub level: u8,
    /// A wavering regiment is close to breaking.
    pub warn: bool,
    /// Direction of change.
    pub trend: i8,
    /// Up to three causes with the largest absolute contribution, largest first.
    pub top: Vec<(u8, FactorBand)>,
}

/// The view of `r`: the three largest `|contribution|` (ties to the lower factor id), banded, and
/// `warn` when `level == 2` and `margin < warn_margin_pm`.
pub fn morale_view(r: &MoraleReport, p: &MoraleViewParams) -> MoraleView {
    let mut ranked: Vec<(u8, u32)> = r
        .factors
        .iter()
        .map(|&(id, c)| (id, c.unsigned_abs()))
        .collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let top = ranked
        .into_iter()
        .take(3)
        .map(|(id, m)| {
            let band = if m >= p.large_pm {
                FactorBand::Large
            } else if m >= p.moderate_pm {
                FactorBand::Moderate
            } else {
                FactorBand::Slight
            };
            (id, band)
        })
        .collect();
    let warn_margin = i64::from(p.warn_margin_pm);
    MoraleView {
        level: r.level,
        warn: r.level == 2 && i64::from(r.margin) < warn_margin,
        trend: r.trend,
        top,
    }
}
