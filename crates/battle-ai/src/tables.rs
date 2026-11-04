//! The AI tables: `data/rules/ai.toml` and the generated `data/rules/ai_traits.toml`
//! (SPEC-010 section 4). Loading follows ADR 0004: integers only, unknown keys refused,
//! provenance on every row.

use crate::personality::Personality;
use battle_rules::{parse_table, Confidence, DataError, Provenance, Table};

/// An AI level (behaviour only; SPEC-010 section 4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Difficulty {
    /// Slow, noisy, uncoordinated.
    Easy,
    /// The reference level.
    Standard,
    /// Fast, accurate, well coordinated.
    Hard,
}

impl Difficulty {
    /// Every level in table order.
    pub const ALL: [Difficulty; 3] = [Difficulty::Easy, Difficulty::Standard, Difficulty::Hard];

    fn from_id(s: &str) -> Option<Difficulty> {
        match s {
            "easy" => Some(Difficulty::Easy),
            "standard" => Some(Difficulty::Standard),
            "hard" => Some(Difficulty::Hard),
            _ => None,
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// A commander trait (the commander schema's list).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trait {
    /// Attacks at lower odds.
    Aggressive,
    /// Attacks only at high odds.
    Cautious,
    /// Weights prepared ground.
    Engineer,
    /// Bolder cavalry seats.
    CavalryFlair,
    /// Holds more in reserve.
    Logistics,
    /// Zero row here.
    Political,
    /// Prefers and holds good ground.
    DefensiveSkill,
    /// Wider initiative radius.
    HighDiscretion,
    /// Narrower initiative radius.
    LowDiscretion,
    /// Zero row here.
    Inspiring,
    /// Zero row here; the feud rule reads relationships.
    Feuding,
}

impl Trait {
    const ALL: [Trait; 11] = [
        Trait::Aggressive,
        Trait::Cautious,
        Trait::Engineer,
        Trait::CavalryFlair,
        Trait::Logistics,
        Trait::Political,
        Trait::DefensiveSkill,
        Trait::HighDiscretion,
        Trait::LowDiscretion,
        Trait::Inspiring,
        Trait::Feuding,
    ];

    fn from_id(s: &str) -> Option<Trait> {
        Trait::ALL.into_iter().find(|t| t.id() == s)
    }

    fn id(self) -> &'static str {
        match self {
            Trait::Aggressive => "aggressive",
            Trait::Cautious => "cautious",
            Trait::Engineer => "engineer",
            Trait::CavalryFlair => "cavalry_flair",
            Trait::Logistics => "logistics",
            Trait::Political => "political",
            Trait::DefensiveSkill => "defensive_skill",
            Trait::HighDiscretion => "high_discretion",
            Trait::LowDiscretion => "low_discretion",
            Trait::Inspiring => "inspiring",
            Trait::Feuding => "feuding",
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// One difficulty row. Behaviour only: no field names a rules value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DifficultyRow {
    /// The level.
    pub id: Difficulty,
    /// Operational-layer decision interval, ticks.
    pub op_interval_ticks: u32,
    /// Tactical-layer decision interval, ticks.
    pub tac_interval_ticks: u32,
    /// Operational think time, ticks.
    pub op_think_ticks: u32,
    /// Tactical think time, ticks.
    pub tac_think_ticks: u32,
    /// Plans forecast per decision.
    pub forecast_candidates: u32,
    /// Samples per forecast.
    pub forecast_samples: u32,
    /// Forecast budget, soldier-ticks.
    pub forecast_budget_soldier_ticks: u32,
    /// Error in the AI's own strength estimates, permille.
    pub estimate_noise_pm: u32,
    /// How well arrivals are coordinated, permille.
    pub sync_margin_pm: u32,
    /// Provenance.
    pub provenance: Provenance,
}

/// Clamp ranges of the personality values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bounds {
    /// Upper bound of the share values (risk, reserve, entrench, discretion), permille.
    pub share_max_pm: u16,
    /// Upper bound of the two ratios, permille.
    pub ratio_max_pm: u16,
    /// Upper bound of `reaction_ticks`.
    pub reaction_max_ticks: u32,
    /// Provenance.
    pub provenance: Provenance,
}

/// One trait row of `ai.toml`: signed deltas per personality value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraitRow {
    /// The trait.
    pub id: Trait,
    /// Delta of `risk_tolerance`.
    pub risk: i32,
    /// Delta of `reserve`.
    pub reserve: i32,
    /// Delta of `assault_ratio`.
    pub assault: i32,
    /// Delta of `withdraw_ratio`.
    pub withdraw: i32,
    /// Delta of `entrench`.
    pub entrench: i32,
    /// Delta of `discretion`.
    pub discretion: i32,
    /// Delta of `reaction_ticks`.
    pub reaction: i32,
    /// True when the row applies to cavalry seats only (risk delta only).
    pub cavalry_only: bool,
    /// The commander record whose documented act fixes the signs.
    pub act_from: Option<String>,
    /// Provenance (sources and note name the act).
    pub provenance: Provenance,
}

impl TraitRow {
    /// The deltas in the order risk, reserve, assault, withdraw, entrench, discretion, reaction.
    pub fn deltas(&self) -> [i32; 7] {
        [
            self.risk,
            self.reserve,
            self.assault,
            self.withdraw,
            self.entrench,
            self.discretion,
            self.reaction,
        ]
    }
}

/// A trait a commander record documents (one `[[trait]]` row of `ai_traits.toml`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraitFact {
    /// Commander record id.
    pub commander: String,
    /// The trait.
    pub id: Trait,
    /// Page of the first source.
    pub source_page: u32,
    /// Tag of the first source.
    pub source_tag: Option<String>,
    /// Provenance (confidence, sources, the documented act as note).
    pub provenance: Provenance,
}

/// Kind of a relationship that bars a reserve commit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelationKind {
    /// A feud.
    Feud,
    /// A rivalry.
    Rivalry,
}

/// A feud or rivalry a commander record documents.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelationFact {
    /// Commander record id.
    pub commander: String,
    /// Feud or rivalry.
    pub kind: RelationKind,
    /// Partner commander id (slug of the name when the partner has no record).
    pub partner: String,
    /// Page of the first source.
    pub source_page: u32,
    /// Provenance.
    pub provenance: Provenance,
}

macro_rules! globals {
    ($($name:ident: $ty:ident, $lo:expr, $hi:expr, $doc:expr;)*) => {
        /// The global constants of SPEC-010 section 4 (placeholders and sourced values).
        #[derive(Clone, Debug, PartialEq, Eq)]
        pub struct AiGlobals {
            $(#[doc = $doc] pub $name: $ty,)*
        }

        const GLOBAL_NAMES: &[(&str, i64, i64)] = &[$((stringify!($name), $lo, $hi),)*];

        impl AiGlobals {
            fn from_values(v: &[i64]) -> AiGlobals {
                let mut it = v.iter().copied();
                // Every value was range-checked into its type's range by the caller.
                AiGlobals { $($name: it.next().and_then(|x| $ty::try_from(x).ok()).unwrap_or(0),)* }
            }
        }
    };
}

globals! {
    sector_mm: i64, 1000, 10_000_000, "Sector edge, millimetres.";
    ack_timeout_ticks: u32, 1, 100_000, "Ticks before an unacknowledged order counts as overdue.";
    max_resends: u32, 0, 16, "Resends before escalation.";
    min_dwell_ticks: u32, 1, 100_000, "Minimum ticks an objective stands before it may change.";
    withdraw_confirm_ticks: u32, 1, 100_000, "Ticks a withdraw condition must hold.";
    stall_window_ticks: u32, 1, 100_000, "Window of the stall watchdog, ticks.";
    stall_min_mm: i64, 1, 1_000_000, "Least progress inside the stall window, millimetres.";
    waypoint_tol_mm: i64, 1, 1_000_000, "Waypoint arrival tolerance, millimetres.";
    form_safety_ticks: u32, 0, 100_000, "Margin added to a formation change before an enemy arrives, ticks.";
    reposition_margin: u32, 0, 1000, "Margin that justifies a reposition, permille.";
    max_repositions: u32, 0, 16, "Repositions allowed before the first shot.";
    lane_interval_mm: i64, 1, 1_000_000, "Lane spacing, millimetres.";
    second_line_mm: i64, 1, 10_000_000, "Depth of the rally search behind the front, millimetres.";
    stand_off_mm: i64, 0, 1_000_000, "Stand-off from a work, millimetres.";
    unseen_margin_pm: u32, 0, 1000, "Margin for ground not seen, permille.";
    gun_support_mm: i64, 1, 10_000_000, "Infantry support distance for guns, millimetres.";
    gun_cease_pm: u32, 0, 1000, "Rounds share below which a battery ceases, permille.";
    fatigue_gate_pm: u32, 0, 1000, "Fatigue share above which a unit is not sent to attack, permille.";
    reg_check_ticks: u32, 1, 1000, "Regiment-level check interval, ticks (sourced).";
    canister_mm: i64, 1, 10_000_000, "Canister range, millimetres (sourced).";
    work_ratio: u32, 1, 100_000, "Casualty ratio a defender behind a work achieves, permille (sourced).";
    bled_pm: u32, 0, 1000, "Strength loss that marks a regiment as bled, permille (sourced).";
    advisor_min_interval_ticks: u32, 1, 1_000_000, "Least ticks between advisor requests (sourced).";
}

/// The loaded AI tables.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AiTables {
    /// Global constants.
    pub globals: AiGlobals,
    /// Provenance of each global, in file order, by name.
    pub global_provenance: Vec<(String, Provenance)>,
    /// Base personality.
    pub base: Personality,
    /// Provenance of the base row.
    pub base_provenance: Provenance,
    /// Clamp ranges.
    pub bounds: Bounds,
    /// Weight of a trait delta by confidence, permille: verified, partial, unverified.
    pub weights: [u32; 3],
    /// Difficulty rows indexed by [`Difficulty`].
    pub difficulties: [DifficultyRow; 3],
    /// Trait rows of `ai.toml`, in file order (each trait once).
    pub traits: Vec<TraitRow>,
    /// Documented traits of commander records, in file order.
    pub facts: Vec<TraitFact>,
    /// Documented feuds and rivalries, in file order.
    pub relations: Vec<RelationFact>,
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

fn confidence_index(c: Confidence) -> usize {
    match c {
        Confidence::Verified => 0,
        Confidence::Partial => 1,
        Confidence::Unverified => 2,
    }
}

fn base_from(t: &mut Table, bounds: &Bounds) -> Result<(Personality, Provenance), DataError> {
    let share = i64::from(bounds.share_max_pm);
    let ratio = i64::from(bounds.ratio_max_pm);
    let p = Personality {
        risk_tolerance: to_u16(t.take_int_in("risk_tolerance", 0, share)?),
        reserve: to_u16(t.take_int_in("reserve", 0, share)?),
        assault_ratio: to_u16(t.take_int_in("assault_ratio", 0, ratio)?),
        withdraw_ratio: to_u16(t.take_int_in("withdraw_ratio", 0, ratio)?),
        entrench: to_u16(t.take_int_in("entrench", 0, share)?),
        discretion: to_u16(t.take_int_in("discretion", 0, share)?),
        reaction_ticks: to_u32(t.take_int_in(
            "reaction_ticks",
            0,
            i64::from(bounds.reaction_max_ticks),
        )?),
        cavalry_risk_tolerance: 0,
        feuds: Vec::new(),
    };
    let p = Personality {
        cavalry_risk_tolerance: p.risk_tolerance,
        ..p
    };
    Ok((p, t.take_provenance()?))
}

fn to_u16(v: i64) -> u16 {
    // Every caller has range-checked `v` into `u16` already.
    u16::try_from(v).unwrap_or(u16::MAX)
}

impl AiTables {
    /// Parses and validates `ai.toml` and `ai_traits.toml` text.
    ///
    /// Errors: the shared reader's [`DataError::Float`], [`DataError::UnknownKey`],
    /// [`DataError::Provenance`] and [`DataError::Parse`]; [`DataError::Range`] for a value out of
    /// range, an unknown or repeated id, a missing difficulty, trait or global, a non-zero trait
    /// row without its documented act (`act_from` and sources), or a `cavalry_only` row with a
    /// delta other than risk.
    pub fn from_toml_str(ai: &str, traits_text: &str) -> Result<AiTables, DataError> {
        let mut root = parse_table(ai)?;

        let mut b = root.take_table("bounds")?;
        let bounds = Bounds {
            share_max_pm: to_u16(b.take_int_in("share_max_pm", 1, 10_000)?),
            ratio_max_pm: to_u16(b.take_int_in("ratio_max_pm", 1, 30_000)?),
            reaction_max_ticks: to_u32(b.take_int_in("reaction_max_ticks", 0, 100_000)?),
            provenance: b.take_provenance()?,
        };
        b.finish()?;

        let mut bt = root.take_table("base")?;
        let (base, base_provenance) = base_from(&mut bt, &bounds)?;
        bt.finish()?;

        let mut weights: [Option<u32>; 3] = [None; 3];
        for mut row in root.take_rows("weight")? {
            let level = row.take_str("level")?;
            let w = to_u32(row.take_int_in("weight_pm", 0, 1000)?);
            row.take_provenance()?;
            let at = row.path().to_owned();
            row.finish()?;
            let i = match level.as_str() {
                "verified" => confidence_index(Confidence::Verified),
                "partial" => confidence_index(Confidence::Partial),
                "unverified" => confidence_index(Confidence::Unverified),
                _ => return Err(DataError::Range(format!("{at}: unknown level {level:?}"))),
            };
            if weights[i].replace(w).is_some() {
                return Err(DataError::Range(format!("{at}: level {level} repeated")));
            }
        }
        let weights = match weights {
            [Some(a), Some(b), Some(c)] => [a, b, c],
            _ => return Err(DataError::Range("weight: every level needs one row".into())),
        };

        let mut difficulties: [Option<DifficultyRow>; 3] = [None, None, None];
        for mut row in root.take_rows("difficulty")? {
            let id = id_of(&mut row, "id", Difficulty::from_id)?;
            let mut n = |k: &str, lo: i64, hi: i64| row.take_int_in(k, lo, hi).map(to_u32);
            let parsed = (
                n("op_interval_ticks", 1, 1_000_000)?,
                n("tac_interval_ticks", 1, 1_000_000)?,
                n("op_think_ticks", 0, 1_000_000)?,
                n("tac_think_ticks", 0, 1_000_000)?,
                n("forecast_candidates", 1, 16)?,
                n("forecast_samples", 1, 16)?,
                n("forecast_budget_soldier_ticks", 1, i64::from(u32::MAX))?,
                n("estimate_noise_pm", 0, 1000)?,
                n("sync_margin_pm", 0, 1000)?,
            );
            let provenance = row.take_provenance()?;
            let at = row.path().to_owned();
            row.finish()?;
            let slot = &mut difficulties[id.index()];
            if slot.is_some() {
                return Err(DataError::Range(format!(
                    "{at}: difficulty {id:?} repeated"
                )));
            }
            *slot = Some(DifficultyRow {
                id,
                op_interval_ticks: parsed.0,
                tac_interval_ticks: parsed.1,
                op_think_ticks: parsed.2,
                tac_think_ticks: parsed.3,
                forecast_candidates: parsed.4,
                forecast_samples: parsed.5,
                forecast_budget_soldier_ticks: parsed.6,
                estimate_noise_pm: parsed.7,
                sync_margin_pm: parsed.8,
                provenance,
            });
        }
        let difficulties = match difficulties {
            [Some(a), Some(b), Some(c)] => [a, b, c],
            _ => {
                return Err(DataError::Range(
                    "difficulty: every level needs one row".into(),
                ))
            }
        };

        let mut values: Vec<Option<i64>> = vec![None; GLOBAL_NAMES.len()];
        let mut global_provenance = Vec::new();
        for mut row in root.take_rows("global")? {
            let name = row.take_str("name")?;
            let at = row.path().to_owned();
            let Some(i) = GLOBAL_NAMES.iter().position(|g| g.0 == name) else {
                return Err(DataError::Range(format!("{at}: unknown global {name:?}")));
            };
            let value = row.take_int_in("value", GLOBAL_NAMES[i].1, GLOBAL_NAMES[i].2)?;
            let provenance = row.take_provenance()?;
            row.finish()?;
            if values[i].replace(value).is_some() {
                return Err(DataError::Range(format!("{at}: global {name} repeated")));
            }
            global_provenance.push((name, provenance));
        }
        let mut flat = Vec::with_capacity(values.len());
        for (v, g) in values.into_iter().zip(GLOBAL_NAMES) {
            flat.push(v.ok_or_else(|| DataError::Range(format!("global {} is missing", g.0)))?);
        }
        let globals = AiGlobals::from_values(&flat);

        let mut slots: Vec<Option<TraitRow>> = vec![None; Trait::ALL.len()];
        for mut row in root.take_rows("trait")? {
            let id = id_of(&mut row, "id", Trait::from_id)?;
            let mut d = |k: &str| row.take_int_in(k, -2000, 2000).map(|v| v as i32);
            let deltas = [
                d("risk")?,
                d("reserve")?,
                d("assault")?,
                d("withdraw")?,
                d("entrench")?,
                d("discretion")?,
                d("reaction")?,
            ];
            let cavalry_only = row.take_bool_opt("cavalry_only")?.unwrap_or(false);
            let act_from = row.take_str_opt("act_from")?;
            let provenance = row.take_provenance()?;
            let at = row.path().to_owned();
            row.finish()?;
            let nonzero = deltas.iter().any(|v| *v != 0);
            if nonzero && (act_from.is_none() || provenance.sources.is_empty()) {
                return Err(DataError::Provenance(format!(
                    "{at}: a delta needs the documented act (act_from and sources)"
                )));
            }
            if cavalry_only && deltas[1..].iter().any(|v| *v != 0) {
                return Err(DataError::Range(format!(
                    "{at}: cavalry_only allows a risk delta only"
                )));
            }
            let slot = &mut slots[id.index()];
            if slot.is_some() {
                return Err(DataError::Range(format!("{at}: trait {id:?} repeated")));
            }
            *slot = Some(TraitRow {
                id,
                risk: deltas[0],
                reserve: deltas[1],
                assault: deltas[2],
                withdraw: deltas[3],
                entrench: deltas[4],
                discretion: deltas[5],
                reaction: deltas[6],
                cavalry_only,
                act_from,
                provenance,
            });
        }
        let trait_rows = slots
            .into_iter()
            .zip(Trait::ALL)
            .map(|(r, t)| r.ok_or_else(|| DataError::Range(format!("trait {t:?} has no row"))))
            .collect::<Result<Vec<_>, _>>()?;
        root.finish()?;

        let (facts, relations) = read_traits(traits_text)?;
        Ok(AiTables {
            globals,
            global_provenance,
            base,
            base_provenance,
            bounds,
            weights,
            difficulties,
            traits: trait_rows,
            facts,
            relations,
        })
    }
}

fn first_source(row: &mut Table, p: &Provenance) -> Result<(u32, Option<String>), DataError> {
    let page = to_u32(row.take_int_in("source_page", 1, i64::from(u32::MAX))?);
    let tag = row.take_str_opt("source_tag")?;
    match p.sources.first() {
        Some(s) if s.page == page && s.tag == tag => Ok((page, tag)),
        _ => Err(DataError::Range(format!(
            "{}: source_page and source_tag must match the first source",
            row.path()
        ))),
    }
}

/// Reads `ai_traits.toml`: each (commander, trait) and each (commander, kind, partner) once.
fn read_traits(text: &str) -> Result<(Vec<TraitFact>, Vec<RelationFact>), DataError> {
    let mut root = parse_table(text)?;
    let mut facts: Vec<TraitFact> = Vec::new();
    for mut row in root.take_rows_opt("trait")?.unwrap_or_default() {
        let commander = row.take_str("commander")?;
        let id = id_of(&mut row, "trait", Trait::from_id)?;
        let provenance = row.take_provenance()?;
        let (source_page, source_tag) = first_source(&mut row, &provenance)?;
        let at = row.path().to_owned();
        row.finish()?;
        if facts.iter().any(|f| f.commander == commander && f.id == id) {
            return Err(DataError::Range(format!(
                "{at}: {commander} {id:?} repeated"
            )));
        }
        facts.push(TraitFact {
            commander,
            id,
            source_page,
            source_tag,
            provenance,
        });
    }
    let mut relations: Vec<RelationFact> = Vec::new();
    for mut row in root.take_rows_opt("relationship")?.unwrap_or_default() {
        let commander = row.take_str("commander")?;
        let kind = match row.take_str("kind")?.as_str() {
            "feud" => RelationKind::Feud,
            "rivalry" => RelationKind::Rivalry,
            other => {
                return Err(DataError::Range(format!(
                    "{}: unknown kind {other:?}",
                    row.path()
                )))
            }
        };
        let partner = row.take_str("partner")?;
        row.take_str("partner_name")?;
        let provenance = row.take_provenance()?;
        let (source_page, _) = first_source(&mut row, &provenance)?;
        let at = row.path().to_owned();
        row.finish()?;
        if relations
            .iter()
            .any(|r| r.commander == commander && r.kind == kind && r.partner == partner)
        {
            return Err(DataError::Range(format!("{at}: relationship repeated")));
        }
        relations.push(RelationFact {
            commander,
            kind,
            partner,
            source_page,
            provenance,
        });
    }
    root.finish()?;
    Ok((facts, relations))
}

fn read_file(dir: &str, name: &str) -> Result<String, DataError> {
    let path = std::path::Path::new(dir).join(name);
    std::fs::read_to_string(&path)
        .map_err(|e| DataError::Parse(format!("cannot read {}: {e}", path.display())))
}

impl AiTables {
    /// Loads `ai.toml` and `ai_traits.toml` from `dir`.
    ///
    /// Errors: [`DataError::Parse`] when a file cannot be read, otherwise whatever
    /// [`AiTables::from_toml_str`] returns.
    pub fn load_dir(dir: &str) -> Result<AiTables, DataError> {
        AiTables::from_toml_str(
            &read_file(dir, "ai.toml")?,
            &read_file(dir, "ai_traits.toml")?,
        )
    }

    /// The row of one difficulty level.
    pub fn difficulty(&self, level: Difficulty) -> &DifficultyRow {
        &self.difficulties[level.index()]
    }

    /// The `ai.toml` row of one trait.
    pub fn trait_row(&self, t: Trait) -> &TraitRow {
        &self.traits[t.index()]
    }

    /// The traits a commander record documents, in file order.
    pub fn trait_facts<'a>(
        &'a self,
        commander: &'a str,
    ) -> impl Iterator<Item = &'a TraitFact> + 'a {
        self.facts.iter().filter(move |f| f.commander == commander)
    }

    /// The feuds and rivalries a commander record documents, in file order.
    pub fn relation_facts<'a>(
        &'a self,
        commander: &'a str,
    ) -> impl Iterator<Item = &'a RelationFact> + 'a {
        self.relations
            .iter()
            .filter(move |r| r.commander == commander)
    }

    /// Commander ids that carry at least one trait row, ascending.
    pub fn trait_commanders(&self) -> impl Iterator<Item = &str> + '_ {
        let mut last: Option<&str> = None;
        self.facts.iter().filter_map(move |f| {
            if last == Some(f.commander.as_str()) {
                None
            } else {
                last = Some(f.commander.as_str());
                last
            }
        })
    }

    /// Weight of a trait delta at a confidence level, permille.
    pub fn weight_pm(&self, c: Confidence) -> u32 {
        self.weights[confidence_index(c)]
    }
}
