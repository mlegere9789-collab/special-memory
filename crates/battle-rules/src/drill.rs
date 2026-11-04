//! The drill table (SPEC-004 section 6): formations, training levels, transition durations and
//! block geometry, loaded from `data/rules/drill.toml`.

use crate::{parse_table, DataError, Provenance, Table};

/// Battle formation of a regiment.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Formation {
    /// Marching column.
    Column,
    /// Line of battle.
    Line,
    /// Skirmish order: a thin deployed rank plus a reserve column.
    Skirmish,
}

impl Formation {
    /// Every formation in tag order.
    pub const ALL: [Formation; 3] = [Formation::Column, Formation::Line, Formation::Skirmish];

    /// Stable numeric tag (hash and wire formats): column 0, line 1, skirmish 2.
    pub const fn tag(self) -> u8 {
        self as u8
    }

    /// Inverse of [`Formation::tag`].
    pub const fn from_tag(tag: u8) -> Option<Formation> {
        match tag {
            0 => Some(Formation::Column),
            1 => Some(Formation::Line),
            2 => Some(Formation::Skirmish),
            _ => None,
        }
    }

    fn from_id(id: &str) -> Option<Formation> {
        match id {
            "column" => Some(Formation::Column),
            "line" => Some(Formation::Line),
            "skirmish" => Some(Formation::Skirmish),
            _ => None,
        }
    }
}

/// Training level of a regiment.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Training {
    /// Wildly uneven, slowest to change formation.
    Raw,
    /// Reference level.
    Trained,
    /// Fastest to change formation.
    Veteran,
}

impl Training {
    /// Every level in tag order.
    pub const ALL: [Training; 3] = [Training::Raw, Training::Trained, Training::Veteran];

    /// Stable numeric tag (hash and wire formats): raw 0, trained 1, veteran 2.
    pub const fn tag(self) -> u8 {
        self as u8
    }

    /// Inverse of [`Training::tag`].
    pub const fn from_tag(tag: u8) -> Option<Training> {
        match tag {
            0 => Some(Training::Raw),
            1 => Some(Training::Trained),
            2 => Some(Training::Veteran),
            _ => None,
        }
    }

    fn from_id(id: &str) -> Option<Training> {
        match id {
            "raw" => Some(Training::Raw),
            "trained" => Some(Training::Trained),
            "veteran" => Some(Training::Veteran),
            _ => None,
        }
    }
}

/// Table-wide drill constants.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DrillGlobals {
    /// A dirty regiment closes up when `tick % closeup_interval_ticks == 0`.
    pub closeup_interval_ticks: u32,
    /// A soldier is settled when within this distance of its slot, in millimetres.
    pub settle_tol_mm: i64,
    /// Ticks allowed for settling after a transition ends.
    pub settle_budget_ticks: u32,
    /// Provenance of the row.
    pub provenance: Provenance,
}

/// Transition speed of one training level.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrainingRow {
    /// The level.
    pub id: Training,
    /// Multiplier on `base_ticks`, permille (1000 = 1.0).
    pub transition_pm: u32,
    /// Provenance of the row.
    pub provenance: Provenance,
}

/// Duration of one formation change at the reference training level.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransitionRow {
    /// Formation before.
    pub from: Formation,
    /// Formation after.
    pub to: Formation,
    /// Ticks at `transition_pm == 1000`.
    pub base_ticks: u32,
    /// Provenance of the row.
    pub provenance: Provenance,
}

/// Block geometry of one formation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormationRow {
    /// The formation.
    pub id: Formation,
    /// Rank count (line only).
    pub ranks: Option<u32>,
    /// Column count (column, and the skirmish reserve).
    pub cols: Option<u32>,
    /// Lateral spacing of files, millimetres.
    pub file_width_mm: i64,
    /// Distance between ranks, millimetres.
    pub rank_gap_mm: i64,
    /// Share of the men in the deployed skirmish rank, permille (skirmish only; 1000 otherwise).
    pub deployed_pm: u32,
    /// Lateral spacing of the deployed skirmish rank, millimetres (skirmish only).
    pub interval_mm: Option<i64>,
    /// Distance from the skirmish rank to the first reserve rank, millimetres (skirmish only).
    pub reserve_offset_mm: Option<i64>,
    /// Provenance of the row.
    pub provenance: Provenance,
}

/// The loaded drill table. Every formation, every training level and every ordered pair of
/// distinct formations is present exactly once (the loader refuses anything else).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DrillTable {
    /// Table-wide constants.
    pub globals: DrillGlobals,
    /// Training rows indexed by [`Training::tag`].
    pub training: [TrainingRow; 3],
    /// Formation rows indexed by [`Formation::tag`].
    pub formations: [FormationRow; 3],
    /// Transition rows in file order.
    pub transitions: Vec<TransitionRow>,
    /// `base_ticks[from][to]`, zero on the diagonal.
    base_ticks: [[u32; 3]; 3],
}

fn to_u32(v: i64) -> u32 {
    // Every caller has range-checked `v` into `u32` already.
    u32::try_from(v).unwrap_or(u32::MAX)
}

fn id_of<T>(row: &mut Table, key: &str, parse: fn(&str) -> Option<T>) -> Result<T, DataError> {
    let text = row.take_str(key)?;
    parse(&text).ok_or_else(|| {
        DataError::Range(format!("{}.{key} = {text:?} is not a known id", row.path()))
    })
}

fn sized<T>(slots: [Option<T>; 3], what: &str) -> Result<[T; 3], DataError> {
    let [a, b, c] = slots;
    match (a, b, c) {
        (Some(a), Some(b), Some(c)) => Ok([a, b, c]),
        _ => Err(DataError::Range(format!("{what}: every id needs one row"))),
    }
}

impl DrillTable {
    /// Parses and validates `drill.toml` text.
    ///
    /// Errors: the shared reader's [`DataError::Float`], [`DataError::UnknownKey`],
    /// [`DataError::Provenance`] and [`DataError::Parse`]; [`DataError::Range`] for a value out of
    /// range, an unknown id, a repeated or missing training level, formation or transition pair,
    /// a transition with `from == to`, or a formation lacking the fields its kind needs.
    pub fn from_toml_str(text: &str) -> Result<DrillTable, DataError> {
        let mut root = parse_table(text)?;

        let mut g = root.take_table("globals")?;
        let globals = DrillGlobals {
            closeup_interval_ticks: to_u32(g.take_int_in(
                "closeup_interval_ticks",
                1,
                i64::from(u32::MAX),
            )?),
            settle_tol_mm: g.take_int_in("settle_tol_mm", 1, 1_000_000)?,
            settle_budget_ticks: to_u32(g.take_int_in(
                "settle_budget_ticks",
                1,
                i64::from(u32::MAX),
            )?),
            provenance: g.take_provenance()?,
        };
        g.finish()?;

        let mut training: [Option<TrainingRow>; 3] = [None, None, None];
        for mut row in root.take_rows("training")? {
            let id = id_of(&mut row, "id", Training::from_id)?;
            let transition_pm = to_u32(row.take_int_in("transition_pm", 1, 10_000)?);
            let provenance = row.take_provenance()?;
            let at = row.path().to_owned();
            row.finish()?;
            let slot = &mut training[usize::from(id.tag())];
            if slot.is_some() {
                return Err(DataError::Range(format!("{at}: training {id:?} repeated")));
            }
            *slot = Some(TrainingRow {
                id,
                transition_pm,
                provenance,
            });
        }
        let training = sized(training, "training")?;

        let mut transitions = Vec::new();
        let mut base_ticks = [[0u32; 3]; 3];
        for mut row in root.take_rows("transition")? {
            let from = id_of(&mut row, "from", Formation::from_id)?;
            let to = id_of(&mut row, "to", Formation::from_id)?;
            let ticks = to_u32(row.take_int_in("base_ticks", 1, 1_000_000)?);
            let provenance = row.take_provenance()?;
            let at = row.path().to_owned();
            row.finish()?;
            if from == to {
                return Err(DataError::Range(format!("{at}: from equals to")));
            }
            let cell = &mut base_ticks[usize::from(from.tag())][usize::from(to.tag())];
            if *cell != 0 {
                return Err(DataError::Range(format!(
                    "{at}: transition {from:?} to {to:?} repeated"
                )));
            }
            *cell = ticks;
            transitions.push(TransitionRow {
                from,
                to,
                base_ticks: ticks,
                provenance,
            });
        }
        for f in Formation::ALL {
            for t in Formation::ALL {
                if f != t && base_ticks[usize::from(f.tag())][usize::from(t.tag())] == 0 {
                    return Err(DataError::Range(format!(
                        "transition {f:?} to {t:?} is missing"
                    )));
                }
            }
        }

        let mut formations: [Option<FormationRow>; 3] = [None, None, None];
        for mut row in root.take_rows("formation")? {
            let at = row.path().to_owned();
            let id = id_of(&mut row, "id", Formation::from_id)?;
            let ranks = opt_int(&mut row, "ranks", 1, 64)?.map(to_u32);
            let cols = opt_int(&mut row, "cols", 1, 64)?.map(to_u32);
            let file_width_mm = row.take_int_in("file_width_mm", 1, 100_000)?;
            let rank_gap_mm = row.take_int_in("rank_gap_mm", 1, 100_000)?;
            let deployed_pm = opt_int(&mut row, "deployed_pm", 1, 1000)?.map(to_u32);
            let interval_mm = opt_int(&mut row, "interval_mm", 1, 100_000)?;
            let reserve_offset_mm = opt_int(&mut row, "reserve_offset_mm", 1, 10_000_000)?;
            let provenance = row.take_provenance()?;
            row.finish()?;
            let needs = |ok: bool, what: &str| {
                if ok {
                    Ok(())
                } else {
                    Err(DataError::Range(format!("{at}: {id:?} {what}")))
                }
            };
            match id {
                Formation::Line => {
                    needs(ranks.is_some(), "needs ranks")?;
                    needs(cols.is_none(), "takes no cols")?;
                    needs(
                        deployed_pm.is_none()
                            && interval_mm.is_none()
                            && reserve_offset_mm.is_none(),
                        "takes no skirmish fields",
                    )?;
                }
                Formation::Column => {
                    needs(cols.is_some(), "needs cols")?;
                    needs(ranks.is_none(), "takes no ranks")?;
                    needs(
                        deployed_pm.is_none()
                            && interval_mm.is_none()
                            && reserve_offset_mm.is_none(),
                        "takes no skirmish fields",
                    )?;
                }
                Formation::Skirmish => {
                    needs(cols.is_some(), "needs cols")?;
                    needs(ranks.is_none(), "takes no ranks")?;
                    needs(
                        deployed_pm.is_some()
                            && interval_mm.is_some()
                            && reserve_offset_mm.is_some(),
                        "needs deployed_pm, interval_mm and reserve_offset_mm",
                    )?;
                }
            }
            let slot = &mut formations[usize::from(id.tag())];
            if slot.is_some() {
                return Err(DataError::Range(format!("{at}: formation {id:?} repeated")));
            }
            *slot = Some(FormationRow {
                id,
                ranks,
                cols,
                file_width_mm,
                rank_gap_mm,
                deployed_pm: deployed_pm.unwrap_or(1000),
                interval_mm,
                reserve_offset_mm,
                provenance,
            });
        }
        let formations = sized(formations, "formation")?;
        root.finish()?;

        Ok(DrillTable {
            globals,
            training,
            formations,
            transitions,
            base_ticks,
        })
    }

    /// Geometry row of a formation.
    pub fn formation(&self, f: Formation) -> &FormationRow {
        &self.formations[usize::from(f.tag())]
    }

    /// Transition speed of a training level, permille.
    pub fn transition_pm(&self, t: Training) -> u32 {
        self.training[usize::from(t.tag())].transition_pm
    }

    /// Reference duration of a formation change in ticks; zero when `from == to`.
    pub fn base_ticks(&self, from: Formation, to: Formation) -> u32 {
        self.base_ticks[usize::from(from.tag())][usize::from(to.tag())]
    }
}

/// An optional integer key that, when present, must lie in `lo..=hi`.
fn opt_int(row: &mut Table, key: &str, lo: i64, hi: i64) -> Result<Option<i64>, DataError> {
    match row.take_int_opt(key)? {
        Some(v) if v < lo || v > hi => Err(DataError::Range(format!(
            "{}.{key} = {v} outside {lo}..={hi}",
            row.path()
        ))),
        other => Ok(other),
    }
}

/// Ticks a regiment of `training` spends in `Forming` when changing from `from` to `to`:
/// `ceil(base_ticks * transition_pm / 1000)`, integer arithmetic. Zero when `from == to`.
///
/// ```
/// use battle_rules::{transition_ticks, DrillTable, Formation, Training};
/// let text = include_str!("../../../data/rules/drill.toml");
/// let table = DrillTable::from_toml_str(text).unwrap();
/// assert_eq!(transition_ticks(Formation::Column, Formation::Line, Training::Raw, &table), 1560);
/// ```
pub fn transition_ticks(
    from: Formation,
    to: Formation,
    training: Training,
    table: &DrillTable,
) -> u32 {
    let scaled = u64::from(table.base_ticks(from, to)) * u64::from(table.transition_pm(training));
    to_u32(i64::try_from(scaled.div_ceil(1000)).unwrap_or(i64::MAX))
}
