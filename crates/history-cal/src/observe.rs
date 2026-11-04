//! Observations (SPEC-011 sections 4.2 and 4.5): emergent events watched while the replay runs,
//! their evidenced windows, and the verdict over a seed list. Integers only.

use crate::metrics::{counts_in, SetEntry, UnitIndex};
use crate::script::{ObserveKind, ObserveRow};
use crate::units::UnitRecord;
use crate::{ReplayScript, ScriptError, Variant, Verdict};
use battle_rules::{BattleState, Casualty, Confidence, TICK_HZ};

/// What one observation row has seen so far.
#[derive(Clone, Debug)]
enum Watch {
    /// A casualty by regiments `a` on regiments `b`; `last` keeps the latest instead of the first.
    Casualty {
        a: Vec<u32>,
        b: Vec<u32>,
        last: bool,
    },
    /// The unit's kw count against `need = ceil(rate_pm * strength / 1000)`.
    Loss {
        set: Vec<u32>,
        strength: i64,
        rate_pm: i64,
        count: i64,
    },
    /// The mean anchor of the regiments within `radius` of `(x, y)`, all in raw `Fx`.
    Reached {
        set: Vec<u32>,
        x: i128,
        y: i128,
        radius: i128,
    },
}

/// The watched rows of a driver, resolved against a unit list by [`Observers::bind`].
#[derive(Clone, Debug, Default)]
pub(crate) struct Observers {
    rows: Vec<ObserveRow>,
    trigger_ids: Vec<String>,
    clock_start_s: i64,
    watches: Vec<Watch>,
    events: Vec<Option<u64>>,
    cursor: usize,
}

/// One row's result in one run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Observation {
    /// The row id.
    pub id: String,
    /// The tick of the event: the casualty's tick, or the tick after the step that satisfied the
    /// condition; `None` when it did not happen. A `last_casualty_by` event is the latest so far.
    pub tick: Option<u64>,
    /// The half-open tick window `[lo, hi)`; `None` for an `after` row whose trigger has not fired.
    pub window: Option<(u64, u64)>,
    /// The row's confidence is `partial` or `verified`.
    pub gated: bool,
}

impl Observation {
    /// True when the event happened inside the window.
    pub fn inside(&self) -> bool {
        match (self.tick, self.window) {
            (Some(t), Some((lo, hi))) => (lo..hi).contains(&t),
            _ => false,
        }
    }
}

fn ids(set: &[SetEntry<'_>]) -> Vec<u32> {
    set.iter().map(|&(r, _)| r).collect()
}

impl Observers {
    pub(crate) fn from_script(script: &ReplayScript) -> Observers {
        let mut rows = script.observes.clone();
        rows.sort_by(|a, b| a.id.cmp(&b.id));
        let mut trigger_ids: Vec<String> = script.triggers.iter().map(|t| t.id.clone()).collect();
        trigger_ids.sort();
        Observers {
            rows,
            trigger_ids,
            clock_start_s: script.meta.clock_start_s,
            ..Observers::default()
        }
    }

    /// Resolves every row's units and starts watching; casualties already in the log are scanned.
    pub(crate) fn bind(&mut self, units: &[UnitRecord]) -> Result<(), ScriptError> {
        let index = UnitIndex::new(units);
        let set = |id: &str| {
            index
                .regiments_under(id)
                .ok_or_else(|| ScriptError::Reference(format!("observe unit {id}")))
        };
        let mut watches = Vec::new();
        for row in &self.rows {
            if let Some(t) = &row.after {
                if self.trigger_ids.binary_search(t).is_err() {
                    return Err(ScriptError::Reference(format!(
                        "observe {} after {t}",
                        row.id
                    )));
                }
            }
            watches.push(match &row.kind {
                ObserveKind::FirstCasualty { a, b } | ObserveKind::LastCasualtyBy { a, b } => {
                    Watch::Casualty {
                        a: ids(&set(a)?),
                        b: ids(&set(b)?),
                        last: matches!(row.kind, ObserveKind::LastCasualtyBy { .. }),
                    }
                }
                ObserveKind::LossRateAtLeast { unit, rate_pm } => {
                    let s = set(unit)?;
                    Watch::Loss {
                        strength: s.iter().map(|e| i64::from(e.1.strength)).sum(),
                        set: ids(&s),
                        rate_pm: *rate_pm,
                        count: 0,
                    }
                }
                ObserveKind::Reached {
                    unit,
                    x_m,
                    y_m,
                    radius_m,
                } => Watch::Reached {
                    set: ids(&set(unit)?),
                    x: i128::from(*x_m) << 32,
                    y: i128::from(*y_m) << 32,
                    radius: i128::from(*radius_m) << 32,
                },
            });
        }
        self.events = vec![None; watches.len()];
        self.watches = watches;
        self.cursor = 0;
        Ok(())
    }

    /// Evaluates every watch at the battle's tick, after a step.
    pub(crate) fn update(&mut self, state: &BattleState) {
        let tick = state.world.tick;
        let new: &[Casualty] = state.casualties.get(self.cursor..).unwrap_or(&[]);
        for (watch, event) in self.watches.iter_mut().zip(&mut self.events) {
            match watch {
                Watch::Casualty { a, b, last } => {
                    for c in new.iter().filter(|c| {
                        counts_in(crate::Basis::Kw, c.kind)
                            && a.contains(&c.shooter_unit)
                            && b.contains(&c.unit)
                    }) {
                        if *last || event.is_none() {
                            *event = Some(c.tick);
                        }
                    }
                }
                Watch::Loss {
                    set,
                    strength,
                    rate_pm,
                    count,
                } => {
                    for c in new
                        .iter()
                        .filter(|c| counts_in(crate::Basis::Kw, c.kind) && set.contains(&c.unit))
                    {
                        *count += 1;
                        if event.is_none() && *strength > 0 && 1000 * *count >= *rate_pm * *strength
                        {
                            *event = Some(c.tick);
                        }
                    }
                    if event.is_none() && *rate_pm == 0 && *strength > 0 {
                        *event = Some(tick);
                    }
                }
                Watch::Reached { set, x, y, radius } => {
                    if event.is_none() && !set.is_empty() {
                        let n = set.len() as i128;
                        let at = |f: fn(&battle_rules::Regiment) -> i64| -> i128 {
                            let sum: i128 = set
                                .iter()
                                .filter_map(|&r| state.regiments.get(r as usize))
                                .map(|r| i128::from(f(r)))
                                .sum();
                            sum.div_euclid(n)
                        };
                        let (dx, dy) =
                            (at(|r| r.anchor.x.raw()) - *x, at(|r| r.anchor.y.raw()) - *y);
                        if dx * dx + dy * dy <= *radius * *radius {
                            *event = Some(tick);
                        }
                    }
                }
            }
        }
        self.cursor = state.casualties.len();
    }

    /// Every row's result so far, ascending row id; `log` is the driver's trigger log.
    pub(crate) fn report(&self, log: &[(u64, u16)]) -> Vec<Observation> {
        let per = i64::from(TICK_HZ);
        let ticks = |s: i64| u64::try_from(s).unwrap_or(0) * per as u64;
        self.rows
            .iter()
            .enumerate()
            .map(|(i, row)| {
                let window = match &row.after {
                    None => Some((
                        ticks(row.lo_s - self.clock_start_s),
                        ticks(row.hi_s - self.clock_start_s),
                    )),
                    Some(t) => self.trigger_ids.binary_search(t).ok().and_then(|rank| {
                        log.iter()
                            .find(|&&(_, idx)| usize::from(idx) == rank)
                            .map(|&(at, _)| (at + ticks(row.lo_s), at + ticks(row.hi_s)))
                    }),
                };
                Observation {
                    id: row.id.clone(),
                    tick: self.events.get(i).copied().flatten(),
                    window,
                    gated: row.provenance.confidence != Confidence::Unverified,
                }
            })
            .collect()
    }
}

/// The verdict over one row's per-seed observations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObservedResult {
    /// Lower median of the event ticks, a missing event counting as later than any tick.
    pub median: Option<u64>,
    /// The window, from the first observation.
    pub window: Option<(u64, u64)>,
    /// `Report` when the row does not gate, else `Pass` or `Fail`; `NotMeasurable` for no runs.
    pub verdict: Verdict,
}

/// Judges one observation row over its per-seed results (SPEC-011 section 4.5): the median over
/// seeds lies in the half-open window; gating follows the row's confidence.
pub fn evaluate_observed(runs: &[Observation]) -> ObservedResult {
    let Some(first) = runs.first() else {
        return ObservedResult {
            median: None,
            window: None,
            verdict: Verdict::NotMeasurable,
        };
    };
    let mut ticks: Vec<u64> = runs.iter().map(|o| o.tick.unwrap_or(u64::MAX)).collect();
    ticks.sort_unstable();
    let median = Some(ticks[(ticks.len() - 1) / 2]).filter(|&t| t != u64::MAX);
    let inside = match (median, first.window) {
        (Some(t), Some((lo, hi))) => (lo..hi).contains(&t),
        _ => false,
    };
    let verdict = match (first.gated, inside) {
        (false, _) => Verdict::Report,
        (true, true) => Verdict::Pass,
        (true, false) => Verdict::Fail,
    };
    ObservedResult {
        median,
        window: first.window,
        verdict,
    }
}

/// The variants a script must be run in (SPEC-011 section 4.5): the nominal run alone, or, when
/// any trigger row has an `early_s` or `late_s`, nominal, early and late.
pub fn variants_to_run(script: &ReplayScript) -> Vec<Variant> {
    if script
        .triggers
        .iter()
        .any(|t| t.early_s.is_some() || t.late_s.is_some())
    {
        vec![Variant::Nominal, Variant::Early, Variant::Late]
    } else {
        vec![Variant::Nominal]
    }
}
