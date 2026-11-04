//! The replay script (SPEC-011 section 3 and amendment A1): orders, trigger hooks, observations
//! and overrides on the clock of the scenario.

use crate::units::{parse_formation, COORD_LIMIT_M};
use crate::ScriptError;
use battle_rules::{parse_table, DataError, Formation, Gait, Provenance, Table, TICK_HZ};
use std::collections::BTreeSet;

/// The `[meta]` table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptMeta {
    /// Scenario id the script is bound to.
    pub scenario: String,
    /// Fidelity class, 0 to 2.
    pub fidelity_class: u8,
    /// Clock second of tick 0.
    pub clock_start_s: i64,
    /// Clock second at which the run ends.
    pub end_s: i64,
}

impl ScriptMeta {
    /// Tick of a clock second: `(at_s - clock_start_s) * 20`. The caller has checked the span.
    pub fn tick_of(&self, at_s: i64) -> u64 {
        u64::try_from(at_s - self.clock_start_s).unwrap_or(0) * u64::from(TICK_HZ)
    }
}

/// What an `[[order]]` row commands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OrderKind {
    /// Stand where you are.
    Hold,
    /// March to a point (whole metres) at a gait.
    Move {
        /// Target x.
        x_m: i64,
        /// Target y.
        y_m: i64,
        /// Gait.
        gait: Gait,
    },
    /// Take up a formation.
    Form(Formation),
    /// Turn to a heading, whole degrees counter-clockwise from east.
    Face {
        /// Heading, 0 to 359.
        heading_deg: u16,
    },
    /// Fire at another unit's regiment.
    FireAt {
        /// Target unit id.
        target_unit: String,
    },
}

/// An order at a clock time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrderRow {
    /// Row id.
    pub id: String,
    /// Addressed unit, any level.
    pub unit: String,
    /// Clock second.
    pub at_s: i64,
    /// The command.
    pub kind: OrderKind,
    /// Source and confidence.
    pub provenance: Provenance,
}

/// A fixed-time hook, with the times of conflicting accounts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TriggerRow {
    /// Row id; the `Trigger` payload indexes these rows in ascending id.
    pub id: String,
    /// Nominal clock second.
    pub at_s: i64,
    /// Earlier conflicting time.
    pub early_s: Option<i64>,
    /// Later conflicting time.
    pub late_s: Option<i64>,
    /// Hook name.
    pub hook: String,
    /// Source and confidence.
    pub provenance: Provenance,
}

/// What an `[[observe]]` row watches for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ObserveKind {
    /// First casualty of unit `b` caused by unit `a`.
    FirstCasualty {
        /// Causing unit.
        a: String,
        /// Suffering unit.
        b: String,
    },
    /// The unit's centroid within `radius_m` of the point.
    Reached {
        /// Unit.
        unit: String,
        /// Point x.
        x_m: i64,
        /// Point y.
        y_m: i64,
        /// Radius.
        radius_m: i64,
    },
    /// The unit's loss rate reaches `rate_pm`.
    LossRateAtLeast {
        /// Unit.
        unit: String,
        /// Permille.
        rate_pm: i64,
    },
    /// Last casualty of unit `b` caused by unit `a`.
    LastCasualtyBy {
        /// Causing unit.
        a: String,
        /// Suffering unit.
        b: String,
    },
}

/// An emergent event with an evidenced half-open window `[lo_s, hi_s)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObserveRow {
    /// Row id.
    pub id: String,
    /// What is watched.
    pub kind: ObserveKind,
    /// Window start (an offset when `after` is set).
    pub lo_s: i64,
    /// Window end.
    pub hi_s: i64,
    /// Trigger id the window is measured from.
    pub after: Option<String>,
    /// Source and confidence.
    pub provenance: Provenance,
}

/// Which kind of scenario row an override replaces the time of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverrideTarget {
    /// A scenario `[[arrival]]`, named by unit id.
    Arrival,
    /// A scenario `[[departure]]`, named by unit id.
    Departure,
    /// A scenario `[[trigger]]`, named by trigger id.
    Trigger,
}

/// Which variant of section 4.5 an override belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverrideVariant {
    /// The early account.
    Early,
    /// The late account.
    Late,
}

/// A replacement time for a scenario row in one variant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OverrideRow {
    /// Row id.
    pub id: String,
    /// Kind of scenario row.
    pub target_kind: OverrideTarget,
    /// Unit id or scenario trigger id.
    pub target: String,
    /// Variant it applies to.
    pub variant: OverrideVariant,
    /// Replacement clock second.
    pub at_s: i64,
    /// Source and confidence.
    pub provenance: Provenance,
}

/// A parsed `script.toml`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplayScript {
    /// The `[meta]` table.
    pub meta: ScriptMeta,
    /// Orders in file order.
    pub orders: Vec<OrderRow>,
    /// Triggers in ascending id.
    pub triggers: Vec<TriggerRow>,
    /// Observations in file order.
    pub observes: Vec<ObserveRow>,
    /// Overrides in file order.
    pub overrides: Vec<OverrideRow>,
}

fn ident(t: &Table, key: &str, v: &str) -> Result<(), ScriptError> {
    let ok = !v.is_empty()
        && v.bytes()
            .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'_'));
    if ok {
        Ok(())
    } else {
        Err(DataError::Parse(format!("{}.{key} {v:?} must match ^[a-z0-9_]+$", t.path())).into())
    }
}

fn name(t: &mut Table, key: &str) -> Result<String, ScriptError> {
    let v = t.take_str(key)?;
    ident(t, key, &v)?;
    Ok(v)
}

fn unknown(t: &Table, key: &str, v: &str) -> ScriptError {
    DataError::Parse(format!("{}.{key}: unknown value {v:?}", t.path())).into()
}

fn coord(t: &mut Table, key: &str) -> Result<i64, ScriptError> {
    Ok(t.take_int_in(key, -COORD_LIMIT_M, COORD_LIMIT_M)?)
}

/// A clock second inside the run: `clock_start_s <= s <= end_s`.
fn in_span(meta: &ScriptMeta, t: &Table, key: &str, s: i64) -> Result<i64, ScriptError> {
    if s < meta.clock_start_s || s > meta.end_s {
        return Err(ScriptError::Time(format!(
            "{}.{key} = {s} outside {}..={}",
            t.path(),
            meta.clock_start_s,
            meta.end_s
        )));
    }
    Ok(s)
}

fn time(meta: &ScriptMeta, t: &mut Table, key: &str) -> Result<i64, ScriptError> {
    let s = t.take_int(key)?;
    in_span(meta, t, key, s)
}

fn read_meta(root: &mut Table) -> Result<ScriptMeta, ScriptError> {
    let mut t = root.take_table("meta")?;
    let scenario = t.take_str("scenario")?;
    let fidelity_class = t.take_int_in("fidelity_class", 0, 2)? as u8;
    let clock_start_s = t.take_int_opt("clock_start_s")?.unwrap_or(0);
    let end_s = t.take_int("end_s")?;
    t.finish()?;
    if scenario.is_empty() || clock_start_s < 0 || end_s <= clock_start_s {
        return Err(ScriptError::Time(format!(
            "meta: clock {clock_start_s}..{end_s}"
        )));
    }
    Ok(ScriptMeta {
        scenario,
        fidelity_class,
        clock_start_s,
        end_s,
    })
}

fn read_order(meta: &ScriptMeta, mut t: Table) -> Result<(OrderRow, String), ScriptError> {
    let id = name(&mut t, "id")?;
    let unit = name(&mut t, "unit")?;
    let at_s = time(meta, &mut t, "at_s")?;
    let kind_s = t.take_str("kind")?;
    let kind = match kind_s.as_str() {
        "hold" => OrderKind::Hold,
        "move" => {
            let (x_m, y_m) = (coord(&mut t, "x_m")?, coord(&mut t, "y_m")?);
            let g = t.take_str("gait")?;
            let gait = Gait::ALL
                .into_iter()
                .find(|x| x.id() == g)
                .ok_or_else(|| unknown(&t, "gait", &g))?;
            OrderKind::Move { x_m, y_m, gait }
        }
        "form" => {
            let f = t.take_str("formation")?;
            OrderKind::Form(parse_formation(&t, &f)?)
        }
        "face" => OrderKind::Face {
            heading_deg: t.take_int_in("heading_deg", 0, 359)? as u16,
        },
        "fire_at" => OrderKind::FireAt {
            target_unit: name(&mut t, "target_unit")?,
        },
        other => return Err(unknown(&t, "kind", other)),
    };
    let provenance = t.take_provenance()?;
    t.finish()?;
    Ok((
        OrderRow {
            id: id.clone(),
            unit,
            at_s,
            kind,
            provenance,
        },
        id,
    ))
}

fn read_trigger(meta: &ScriptMeta, mut t: Table) -> Result<TriggerRow, ScriptError> {
    let id = name(&mut t, "id")?;
    let at_s = time(meta, &mut t, "at_s")?;
    let alt =
        |t: &mut Table, key: &str, ok: fn(i64, i64) -> bool| -> Result<Option<i64>, ScriptError> {
            let Some(s) = t.take_int_opt(key)? else {
                return Ok(None);
            };
            in_span(meta, t, key, s)?;
            if !ok(s, at_s) {
                return Err(ScriptError::Time(format!(
                    "{}.{key} = {s} is on the wrong side of at_s",
                    t.path()
                )));
            }
            Ok(Some(s))
        };
    let early_s = alt(&mut t, "early_s", |s, at| s < at)?;
    let late_s = alt(&mut t, "late_s", |s, at| s > at)?;
    let hook = t
        .take_str_opt("hook")?
        .unwrap_or_else(|| "marker".to_owned());
    ident(&t, "hook", &hook)?;
    let provenance = t.take_provenance()?;
    t.finish()?;
    Ok(TriggerRow {
        id,
        at_s,
        early_s,
        late_s,
        hook,
        provenance,
    })
}

fn read_observe(meta: &ScriptMeta, mut t: Table) -> Result<ObserveRow, ScriptError> {
    let id = name(&mut t, "id")?;
    let kind_s = t.take_str("kind")?;
    let kind = match kind_s.as_str() {
        "first_casualty" => ObserveKind::FirstCasualty {
            a: name(&mut t, "a")?,
            b: name(&mut t, "b")?,
        },
        "last_casualty_by" => ObserveKind::LastCasualtyBy {
            a: name(&mut t, "a")?,
            b: name(&mut t, "b")?,
        },
        "reached" => ObserveKind::Reached {
            unit: name(&mut t, "unit")?,
            x_m: coord(&mut t, "x_m")?,
            y_m: coord(&mut t, "y_m")?,
            radius_m: t.take_int_in("radius_m", 0, COORD_LIMIT_M)?,
        },
        "loss_rate_at_least" => ObserveKind::LossRateAtLeast {
            unit: name(&mut t, "unit")?,
            rate_pm: t.take_int_in("rate_pm", 0, 1000)?,
        },
        other => return Err(unknown(&t, "kind", other)),
    };
    let (lo_s, hi_s) = (t.take_int("lo_s")?, t.take_int("hi_s")?);
    let after = t.take_str_opt("after")?;
    if let Some(a) = &after {
        ident(&t, "after", a)?;
    }
    let provenance = t.take_provenance()?;
    t.finish()?;
    // With `after` the window is an offset from the trigger; otherwise clock seconds in the run.
    let floor = if after.is_some() {
        0
    } else {
        meta.clock_start_s
    };
    if lo_s >= hi_s || lo_s < floor {
        return Err(ScriptError::Time(format!(
            "observe {id}: window {lo_s}..{hi_s}"
        )));
    }
    Ok(ObserveRow {
        id,
        kind,
        lo_s,
        hi_s,
        after,
        provenance,
    })
}

fn read_override(meta: &ScriptMeta, mut t: Table) -> Result<OverrideRow, ScriptError> {
    let id = name(&mut t, "id")?;
    let k = t.take_str("target_kind")?;
    let target_kind = match k.as_str() {
        "arrival" => OverrideTarget::Arrival,
        "departure" => OverrideTarget::Departure,
        "trigger" => OverrideTarget::Trigger,
        other => return Err(unknown(&t, "target_kind", other)),
    };
    let target = name(&mut t, "target")?;
    let v = t.take_str("variant")?;
    let variant = match v.as_str() {
        "early" => OverrideVariant::Early,
        "late" => OverrideVariant::Late,
        other => return Err(unknown(&t, "variant", other)),
    };
    let at_s = time(meta, &mut t, "at_s")?;
    let provenance = t.take_provenance()?;
    t.finish()?;
    Ok(OverrideRow {
        id,
        target_kind,
        target,
        variant,
        at_s,
        provenance,
    })
}

impl ReplayScript {
    /// Parses `script.toml` text (`schema = "replay-script/1"`).
    ///
    /// Errors: [`ScriptError::Data`] for TOML, float, unknown key, provenance, range or missing-key
    /// faults; [`ScriptError::Duplicate`] for a repeated row id; [`ScriptError::Time`] for a time
    /// outside `clock_start_s..=end_s` (or an empty or inverted window); [`ScriptError::Reference`]
    /// for an `after` that names no trigger. Units named by rows are resolved by
    /// [`compile_script`](crate::compile_script).
    pub fn from_toml_str(text: &str) -> Result<ReplayScript, ScriptError> {
        let mut root = parse_table(text)?;
        if root.take_str("schema")? != "replay-script/1" {
            return Err(DataError::Parse("schema must be replay-script/1".into()).into());
        }
        let meta = read_meta(&mut root)?;
        let mut ids = BTreeSet::new();
        let mut orders = Vec::new();
        for t in root.take_rows_opt("order")?.unwrap_or_default() {
            let (row, id) = read_order(&meta, t)?;
            unique(&mut ids, id)?;
            orders.push(row);
        }
        let mut triggers = Vec::new();
        for t in root.take_rows_opt("trigger")?.unwrap_or_default() {
            let row = read_trigger(&meta, t)?;
            unique(&mut ids, row.id.clone())?;
            triggers.push(row);
        }
        triggers.sort_by(|a, b| a.id.cmp(&b.id));
        let mut observes = Vec::new();
        for t in root.take_rows_opt("observe")?.unwrap_or_default() {
            let row = read_observe(&meta, t)?;
            unique(&mut ids, row.id.clone())?;
            if let Some(a) = &row.after {
                if !triggers.iter().any(|x| &x.id == a) {
                    return Err(ScriptError::Reference(format!(
                        "observe {} after {a}",
                        row.id
                    )));
                }
            }
            observes.push(row);
        }
        let mut overrides = Vec::new();
        for t in root.take_rows_opt("override")?.unwrap_or_default() {
            let row = read_override(&meta, t)?;
            unique(&mut ids, row.id.clone())?;
            overrides.push(row);
        }
        root.finish()?;
        Ok(ReplayScript {
            meta,
            orders,
            triggers,
            observes,
            overrides,
        })
    }
}

fn unique(ids: &mut BTreeSet<String>, id: String) -> Result<(), ScriptError> {
    if ids.insert(id.clone()) {
        Ok(())
    } else {
        Err(ScriptError::Duplicate(id))
    }
}
