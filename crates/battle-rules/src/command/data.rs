//! The friction table (`data/rules/command.toml`, SPEC-007 section 3) and the officers sidecar
//! (`data/scenarios/<family>/officers.toml`) with their loaders.

use super::{Level, Medium, OfficerId, TraitMask};
use crate::registry::{T_AGGRESSIVE, T_CAUTIOUS, T_HIGH_DISC, T_LOW_DISC};
use crate::{parse_table, DataError, Provenance, RegimentId, Table};
use std::collections::BTreeMap;

fn fit<T: TryFrom<i64>>(v: i64, key: &str) -> Result<T, DataError> {
    T::try_from(v).map_err(|_| DataError::Range(format!("{key} = {v} does not fit its type")))
}

fn id_of<T>(row: &mut Table, key: &str, parse: fn(&str) -> Option<T>) -> Result<T, DataError> {
    let text = row.take_str(key)?;
    parse(&text).ok_or_else(|| {
        DataError::Range(format!("{}.{key} = {text:?} is not a known id", row.path()))
    })
}

fn medium_id(id: &str) -> Option<Medium> {
    match id {
        "courier" => Some(Medium::Courier),
        "flags" => Some(Medium::Flags),
        "telegraph" => Some(Medium::Telegraph),
        _ => None,
    }
}

fn trait_bit(id: &str) -> Option<TraitMask> {
    match id {
        "aggressive" => Some(TraitMask(T_AGGRESSIVE)),
        "cautious" => Some(TraitMask(T_CAUTIOUS)),
        "high_discretion" => Some(TraitMask(T_HIGH_DISC)),
        "low_discretion" => Some(TraitMask(T_LOW_DISC)),
        _ => None,
    }
}

macro_rules! globals {
    ($($(#[$doc:meta])* $f:ident: $t:ty = $lo:expr, $hi:expr;)*) => {
        /// Table-wide constants of `[globals]` (all placeholders); each is range-checked by the
        /// loader. Units are in the key suffix.
        #[derive(Clone, Debug, PartialEq, Eq)]
        pub struct CommandGlobals {
            $($(#[$doc])* pub $f: $t,)*
            /// Provenance of the row.
            pub provenance: Provenance,
        }

        fn read_globals(g: &mut Table) -> Result<CommandGlobals, DataError> {
            Ok(CommandGlobals {
                $($f: fit(g.take_int_in(stringify!($f), $lo, $hi)?, stringify!($f))?,)*
                provenance: g.take_provenance()?,
            })
        }
    };
}

globals! {
    /// Courier route length over straight-line distance, permille.
    path_factor_pm: u32 = 1000, 10000;
    /// Largest random delay added to one leg, ticks.
    jitter_ticks: u32 = 0, 100000;
    /// A courier finds a recipient within this distance of the aim point, mm.
    find_radius_mm: i64 = 0, 100000000;
    /// Most courier legs one message may take.
    max_legs: u32 = 1, 16;
    /// Floor of the terrain speed factor on a courier leg, permille.
    min_terrain_pm: u32 = 1, 1000;
    /// Courier loss probability per kilometre of route, ppm.
    loss_ppm_per_km: u32 = 0, 1000000;
    /// Courier loss probability per enemy agent near the route, ppm.
    exposure_ppm_per_enemy: u32 = 0, 1000000;
    /// Most enemy agents counted toward exposure.
    exposure_cap: u32 = 0, 1000;
    /// Half width of the route corridor searched for enemies, mm.
    exposure_halfwidth_mm: i64 = 1, 10000000;
    /// Upper bound of the loss probability, ppm.
    loss_cap_ppm: u32 = 0, 1000000;
    /// Share of losses near an enemy that are captures, permille.
    capture_share_pm: u32 = 0, 1000;
    /// Quality of the contact a captured order gives the enemy, permille.
    capture_quality_pm: u32 = 0, 1000;
    /// Longest distance a flag link works over, mm.
    flag_range_mm: i64 = 0, 100000000;
    /// Smoke density above which flags cannot be read.
    smoke_limit: u32 = 0, 1000000;
    /// Interval between smoke re-tests of a flag message, ticks.
    smoke_wait_ticks: u32 = 1, 100000;
    /// Smoke re-tests before a flag message falls back to a courier.
    smoke_max_waits: u32 = 0, 100;
    /// Standard deviation of a garbled destination, mm per axis.
    garble_sigma_mm: i64 = 0, 10000000;
    /// Share of garbled moves that displace the target, permille.
    garble_move_displace_pm: u32 = 0, 1000;
    /// Cumulative threshold of garbled moves that shift the gait, permille.
    garble_move_gait_pm: u32 = 0, 1000;
    /// Share of garbled forms that name another formation, permille.
    garble_form_swap_pm: u32 = 0, 1000;
    /// Share of garbled faces that turn the heading, permille.
    garble_face_turn_pm: u32 = 0, 1000;
    /// Heading error of a garbled face per unit of draw, degrees.
    garble_angle_deg: u32 = 0, 180;
    /// Largest random delay added to staff time, ticks.
    staff_jitter_ticks: u32 = 0, 100000;
    /// Base go score of a discretionary order, permille.
    base_go_pm: i32 = -2000, 2000;
    /// Weight of the odds term in the go score, permille.
    odds_weight_pm: u32 = 0, 10000;
    /// Weight of the fatigue term in the go score, permille.
    fatigue_weight_pm: u32 = 0, 10000;
    /// Standard deviation of the officer's judgement, permille.
    judge_sigma_pm: u32 = 0, 10000;
    /// Go score below which a discretionary order is declined, permille.
    go_threshold_pm: i32 = -2000, 2000;
    /// Radius around the target in which known enemies count, mm.
    practicable_radius_mm: i64 = 1, 100000000;
    /// Ticks an unacknowledged order stays overdue before it is presumed lost.
    lost_grace_ticks: u32 = 0, 1000000;
    /// Delay before a message to a fallen officer reaches his deputy, ticks.
    redirect_ticks: u32 = 0, 1000000;
    /// Interval between officer position refreshes, ticks.
    pos_interval_ticks: u32 = 1, 10000;
    /// Distance at which a regiment can see an enemy regiment, mm.
    sight_range_mm: i64 = 1, 100000000;
    /// Observation quality below which a sighting is dropped, permille.
    min_quality_pm: u32 = 0, 1000;
    /// Occlusion per unit of smoke density, permille.
    smoke_occ_per_unit_pm: u32 = 0, 1000;
    /// Position error of a perfect sighting, mm per axis.
    pos_floor_mm: i64 = 0, 10000000;
    /// Added position error of a worst-quality sighting, mm per axis.
    pos_sigma_max_mm: i64 = 0, 10000000;
    /// Strength estimate error of a worst-quality sighting, permille.
    strength_sigma_pm: u32 = 0, 10000;
    /// Base distance within which a sighting joins a known contact, mm.
    assoc_base_mm: i64 = 0, 100000000;
    /// Interval between observations, ticks.
    observe_interval_ticks: u32 = 1, 100000;
    /// Age at which a contact has no confidence left, ticks.
    stale_ticks: u32 = 1, 10000000;
    /// Confidence below which a contact is dropped, permille.
    drop_pm: u32 = 0, 1000;
    /// Assumed enemy drift used to grow uncertainty, mm per tick.
    drift_mm_per_tick: i64 = 0, 10000;
    /// Interval between an officer's reports, ticks.
    report_interval_ticks: u32 = 1, 1000000;
    /// Most contacts in one report.
    report_max: u32 = 1, 1000;
}

/// Latency and friction of one medium.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MediumRow {
    /// The medium.
    pub id: Medium,
    /// Ticks to write the message.
    pub write_ticks: u32,
    /// Fixed ticks to send it.
    pub base_ticks: u32,
    /// Extra ticks per word (flags, telegraph).
    pub per_word_ticks: u32,
    /// Courier speed over open ground, millimetres per second (0 for lines).
    pub speed_mm_s: u32,
    /// Loss probability before distance and exposure, ppm.
    pub loss_base_ppm: u32,
    /// Probability that an order is garbled in reading, ppm.
    pub garble_ppm: u32,
    /// Provenance of the row.
    pub provenance: Provenance,
}

/// Staff time of one level.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LevelRow {
    /// The level.
    pub level: Level,
    /// Ticks the staff takes to pass an order on.
    pub staff_ticks: u32,
    /// Provenance of the row.
    pub provenance: Provenance,
}

/// Effect of one command trait.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraitRow {
    /// The single trait bit of this row.
    pub mask: TraitMask,
    /// Staff time factor, permille.
    pub process_pm: u32,
    /// Added to the go score of a discretionary order, permille.
    pub go_bias_pm: i32,
    /// Distance the target of a move is pushed along its line, millimetres (negative pulls back).
    pub reach_mm: i64,
    /// Gait steps added to a move.
    pub gait_shift: i32,
    /// Provenance of the row.
    pub provenance: Provenance,
}

/// The loaded friction table. Every medium, level and trait is present exactly once.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandTable {
    /// Medium rows indexed by `Medium::tag() - 1`.
    pub media: [MediumRow; 3],
    /// Level rows indexed by `Level::tag()`.
    pub levels: [LevelRow; 5],
    /// Trait rows in the order aggressive, cautious, high discretion, low discretion.
    pub traits: [TraitRow; 4],
    /// Table-wide constants.
    pub globals: CommandGlobals,
}

fn all_present<T, const N: usize>(slots: [Option<T>; N], what: &str) -> Result<[T; N], DataError> {
    if slots.iter().any(Option::is_none) {
        return Err(DataError::Range(format!("{what}: every id needs one row")));
    }
    let mut it = slots.into_iter().flatten();
    Ok(std::array::from_fn(|_| {
        it.next().expect("presence checked above")
    }))
}

fn place<T>(slot: &mut Option<T>, row: T, at: &str) -> Result<(), DataError> {
    if slot.replace(row).is_some() {
        return Err(DataError::Range(format!("{at}: id repeated")));
    }
    Ok(())
}

fn expect_schema(root: &mut Table, want: &str) -> Result<(), DataError> {
    let got = root.take_str("schema")?;
    if got == want {
        Ok(())
    } else {
        Err(DataError::Parse(format!(
            "schema = {got:?}, expected {want:?}"
        )))
    }
}

impl CommandTable {
    /// Parses and validates `command.toml` text.
    ///
    /// Errors: the shared reader's [`DataError::Float`], [`DataError::UnknownKey`],
    /// [`DataError::Provenance`] and [`DataError::Parse`] (also a wrong `schema`);
    /// [`DataError::Range`] for a value out of range, an unknown, repeated or missing medium,
    /// level or trait, or a courier with no speed.
    pub fn from_toml_str(text: &str) -> Result<CommandTable, DataError> {
        let mut root = parse_table(text)?;
        expect_schema(&mut root, "command/1")?;

        let mut media: [Option<MediumRow>; 3] = [None, None, None];
        for mut row in root.take_rows("medium")? {
            let at = row.path().to_owned();
            let id = id_of(&mut row, "id", medium_id)?;
            let mut int =
                |k: &str, hi: i64| -> Result<u32, DataError> { fit(row.take_int_in(k, 0, hi)?, k) };
            let write_ticks = int("write_ticks", 1_000_000)?;
            let base_ticks = int("base_ticks", 1_000_000)?;
            let per_word_ticks = int("per_word_ticks", 10_000)?;
            let speed_mm_s = int("speed_mm_s", 100_000)?;
            let loss_base_ppm = int("loss_base_ppm", 1_000_000)?;
            let garble_ppm = int("garble_ppm", 1_000_000)?;
            let provenance = row.take_provenance()?;
            row.finish()?;
            if id == Medium::Courier && speed_mm_s == 0 {
                return Err(DataError::Range(format!("{at}: a courier needs a speed")));
            }
            let medium = MediumRow {
                id,
                write_ticks,
                base_ticks,
                per_word_ticks,
                speed_mm_s,
                loss_base_ppm,
                garble_ppm,
                provenance,
            };
            place(&mut media[usize::from(id.tag()) - 1], medium, &at)?;
        }

        let mut levels: [Option<LevelRow>; 5] = [None, None, None, None, None];
        for mut row in root.take_rows("level")? {
            let at = row.path().to_owned();
            let tag: u8 = fit(row.take_int_in("level", 0, 4)?, "level")?;
            let level = Level::from_tag(tag)
                .ok_or_else(|| DataError::Range(format!("{at}: unknown level")))?;
            let staff_ticks = fit(row.take_int_in("staff_ticks", 0, 1_000_000)?, "staff_ticks")?;
            let provenance = row.take_provenance()?;
            row.finish()?;
            let value = LevelRow {
                level,
                staff_ticks,
                provenance,
            };
            place(&mut levels[usize::from(tag)], value, &at)?;
        }

        let mut traits: [Option<TraitRow>; 4] = [None, None, None, None];
        for mut row in root.take_rows("trait")? {
            let at = row.path().to_owned();
            let mask = id_of(&mut row, "trait", trait_bit)?;
            let process_pm = fit(row.take_int_in("process_pm", 1, 10_000)?, "process_pm")?;
            let go_bias_pm = fit(row.take_int_in("go_bias_pm", -1000, 1000)?, "go_bias_pm")?;
            let reach_mm = row.take_int_in("reach_mm", -10_000_000, 10_000_000)?;
            let gait_shift = fit(row.take_int_in("gait_shift", -3, 3)?, "gait_shift")?;
            let provenance = row.take_provenance()?;
            row.finish()?;
            let value = TraitRow {
                mask,
                process_pm,
                go_bias_pm,
                reach_mm,
                gait_shift,
                provenance,
            };
            place(&mut traits[mask.0.trailing_zeros() as usize], value, &at)?;
        }

        let mut g = root.take_table("globals")?;
        let globals = read_globals(&mut g)?;
        g.finish()?;
        root.finish()?;
        Ok(CommandTable {
            media: all_present(media, "medium")?,
            levels: all_present(levels, "level")?,
            traits: all_present(traits, "trait")?,
            globals,
        })
    }

    /// Row of a medium.
    pub fn medium(&self, m: Medium) -> &MediumRow {
        &self.media[usize::from(m.tag()) - 1]
    }

    /// Staff time of a level, ticks.
    pub fn staff_ticks(&self, l: Level) -> u32 {
        self.levels[usize::from(l.tag())].staff_ticks
    }

    /// Row of a single trait bit; `None` for a mask that is not exactly one known bit.
    pub fn trait_row(&self, mask: TraitMask) -> Option<&TraitRow> {
        self.traits.iter().find(|r| r.mask == mask)
    }
}

/// One `[[officer]]` row of the officers sidecar (the fixed part of an officer; state is added by
/// the command state).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OfficerSpec {
    /// Unique id; rows ascend.
    pub id: OfficerId,
    /// Side, 0 or 1.
    pub side: u8,
    /// Rank.
    pub level: Level,
    /// Superior; none for the army.
    pub parent: Option<OfficerId>,
    /// The regiment a regiment-level officer owns; none for every other level.
    pub regiment: Option<RegimentId>,
    /// Id of a `data/history/commanders` record, if the officer is historical.
    pub commander: Option<String>,
    /// Successor when this officer falls.
    pub deputy: Option<OfficerId>,
    /// Fixed headquarters `(x, y)` in millimetres.
    pub hq: Option<(i64, i64)>,
    /// Observer range multiplier, permille (1000 when absent).
    pub post_range_pm: u32,
}

/// A symmetric signal link between two officers of one side.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LinkSpec {
    /// `Flags` or `Telegraph`.
    pub medium: Medium,
    /// Lower officer id.
    pub a: OfficerId,
    /// Higher officer id.
    pub b: OfficerId,
}

/// The loaded officers sidecar: officers ascending by id, links ascending by `(a, b, medium)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OfficerSet {
    /// Officers in ascending id order.
    pub officers: Vec<OfficerSpec>,
    /// Links, each stored once with `a < b`.
    pub links: Vec<LinkSpec>,
}

fn bad<T>(msg: String) -> Result<T, DataError> {
    Err(DataError::Range(msg))
}

fn read_officer(mut row: Table, previous: Option<OfficerId>) -> Result<OfficerSpec, DataError> {
    let at = row.path().to_owned();
    let id = OfficerId(fit(row.take_int_in("id", 0, 65_535)?, "id")?);
    let side = fit(row.take_int_in("side", 0, 1)?, "side")?;
    let tag = fit(row.take_int_in("level", 0, 4)?, "level")?;
    let level = Level::from_tag(tag).ok_or_else(|| DataError::Range(format!("{at}: level")))?;
    let parent = row.take_int_opt("parent")?;
    let regiment = row.take_int_opt("regiment")?;
    let commander = row.take_str_opt("commander")?;
    let deputy = row.take_int_opt("deputy")?;
    let hq_x = row.take_int_opt("hq_x_mm")?;
    let hq_y = row.take_int_opt("hq_y_mm")?;
    let post_range_pm = row.take_int_opt("post_range_pm")?.unwrap_or(1000);
    row.finish()?;
    if previous.is_some_and(|p| p >= id) {
        return bad(format!("{at}: officer ids must be unique and ascending"));
    }
    if !(1..=10_000).contains(&post_range_pm) {
        return bad(format!("{at}: post_range_pm outside 1..=10000"));
    }
    let post_range_pm = fit(post_range_pm, "post_range_pm")?;
    let id_opt = |v: Option<i64>, key: &str| -> Result<Option<OfficerId>, DataError> {
        v.map(|n| fit(n, key).map(OfficerId)).transpose()
    };
    let (parent, deputy) = (id_opt(parent, "parent")?, id_opt(deputy, "deputy")?);
    if (level == Level::Army) == parent.is_some() {
        return bad(format!("{at}: only the army has no parent"));
    }
    if (level == Level::Regiment) != regiment.is_some() {
        return bad(format!(
            "{at}: only a regiment-level officer owns a regiment"
        ));
    }
    if deputy == Some(id) {
        return bad(format!("{at}: an officer cannot be his own deputy"));
    }
    let hq = match (hq_x, hq_y) {
        (Some(x), Some(y)) => Some((x, y)),
        (None, None) => None,
        _ => return bad(format!("{at}: hq_x_mm and hq_y_mm come together")),
    };
    let regiment = regiment
        .map(|n| fit(n, "regiment").map(RegimentId))
        .transpose()?;
    Ok(OfficerSpec {
        id,
        side,
        level,
        parent,
        regiment,
        commander,
        deputy,
        hq,
        post_range_pm,
    })
}

/// Checks that parents and deputies exist and that the parent chains, levels, sides, armies and
/// regiment owners are sound.
fn check_structure(officers: &[OfficerSpec]) -> Result<(), DataError> {
    let find = |id: OfficerId| officers.binary_search_by_key(&id, |o| o.id).ok();
    for o in officers {
        for (what, target) in [("parent", o.parent), ("deputy", o.deputy)] {
            if let Some(t) = target {
                if find(t).is_none() {
                    return bad(format!("officer {}: {what} {} does not exist", o.id.0, t.0));
                }
            }
        }
    }
    for o in officers {
        let (mut cur, mut steps) = (o, 0usize);
        while let Some(p) = cur.parent.and_then(find) {
            cur = &officers[p];
            steps += 1;
            if steps > officers.len() {
                return bad(format!("cycle in the parent chain of officer {}", o.id.0));
            }
        }
    }
    let mut armies: [Option<OfficerId>; 2] = [None, None];
    for o in officers.iter().filter(|o| o.level == Level::Army) {
        if let Some(first) = armies[usize::from(o.side)].replace(o.id) {
            return bad(format!(
                "side {} has two armies (officers {} and {})",
                o.side, first.0, o.id.0
            ));
        }
    }
    for o in officers {
        if let Some(p) = o.parent.and_then(find).map(|i| &officers[i]) {
            if o.level.tag() != p.level.tag() + 1 {
                return bad(format!("officer {}: level is not parent level + 1", o.id.0));
            }
            if o.side != p.side {
                return bad(format!("officer {}: parent is on the other side", o.id.0));
            }
        }
        if let Some(d) = o.deputy.and_then(find).map(|i| &officers[i]) {
            if o.side != d.side {
                return bad(format!("officer {}: deputy is on the other side", o.id.0));
            }
        }
    }
    let mut owners: BTreeMap<RegimentId, OfficerId> = BTreeMap::new();
    for o in officers {
        if let Some(r) = o.regiment {
            if let Some(first) = owners.insert(r, o.id) {
                return bad(format!(
                    "regiment {} owned twice (officers {} and {})",
                    r.0, first.0, o.id.0
                ));
            }
        }
    }
    Ok(())
}

fn read_link(mut row: Table, officers: &[OfficerSpec]) -> Result<LinkSpec, DataError> {
    let at = row.path().to_owned();
    let medium = match id_of(&mut row, "medium", medium_id)? {
        Medium::Courier => return bad(format!("{at}: a link is flags or telegraph")),
        m => m,
    };
    let a = OfficerId(fit(row.take_int_in("a", 0, 65_535)?, "a")?);
    let b = OfficerId(fit(row.take_int_in("b", 0, 65_535)?, "b")?);
    row.finish()?;
    let side_of = |id: OfficerId| {
        officers
            .binary_search_by_key(&id, |o| o.id)
            .map(|i| officers[i].side)
            .map_err(|_| DataError::Range(format!("{at}: officer {} does not exist", id.0)))
    };
    if a == b {
        return bad(format!("{at}: a link joins two different officers"));
    }
    if side_of(a)? != side_of(b)? {
        return bad(format!("{at}: a link stays within one side"));
    }
    Ok(LinkSpec {
        medium,
        a: a.min(b),
        b: a.max(b),
    })
}

/// Parses and validates an officers sidecar (`schema = "officers/1"`).
///
/// Errors: the shared reader's [`DataError::Float`], [`DataError::UnknownKey`] and
/// [`DataError::Parse`]; [`DataError::Range`] for ids that do not ascend, a missing or unknown
/// parent, deputy or officer in a link, a parent cycle (message starts "cycle"), a level that is not
/// parent level + 1, two armies on a side, a regiment owned twice, a repeated link and the other
/// faults named in SPEC-007 section 3.
pub fn load_officers(text: &str) -> Result<OfficerSet, DataError> {
    let mut root = parse_table(text)?;
    expect_schema(&mut root, "officers/1")?;
    let mut officers: Vec<OfficerSpec> = Vec::new();
    for row in root.take_rows("officer")? {
        let previous = officers.last().map(|o| o.id);
        officers.push(read_officer(row, previous)?);
    }
    let rows = root.take_rows_opt("link")?.unwrap_or_default();
    root.finish()?;
    check_structure(&officers)?;
    let mut links = Vec::new();
    for row in rows {
        links.push(read_link(row, &officers)?);
    }
    links.sort_by_key(|l| (l.a, l.b, l.medium));
    if links.windows(2).any(|w| w[0] == w[1]) {
        return bad("link repeated".to_owned());
    }
    Ok(OfficerSet { officers, links })
}
