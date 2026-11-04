//! Metrics and band evaluation (SPEC-011 sections 4.3 and 4.4): casualty-log measures over a
//! unit's regiments, then a verdict over the per-seed values. Integers only.

use crate::bands::{Band, Basis, Metric, Need};
use crate::units::UnitRecord;
use crate::{Policy, ReplayDriver, ScriptError};
use battle_rules::{Casualty, CasualtyKind, TICK_HZ};

/// Fewest seeds from which a spread is judged: one or two values have no meaningful spread.
const MIN_SEEDS_FOR_SPREAD: usize = 3;

/// The rules the running simulation implements (SPEC-011 assumption A2).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Capabilities {
    /// Captured soldiers are recorded in the casualty log.
    pub capture: bool,
    /// Morale and rout are modelled.
    pub morale: bool,
    /// Melee is modelled.
    pub melee: bool,
}

impl Capabilities {
    fn has(&self, need: Need) -> bool {
        match need {
            Need::Capture => self.capture,
            Need::Morale => self.morale,
            Need::Melee => self.melee,
        }
    }
}

/// The outcome of judging one band over a seed list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// A rule the band needs is not implemented.
    NotMeasurable,
    /// The band does not gate; the numbers are reported.
    Report,
    /// The seeds disagree by more than the spread limit.
    Unstable,
    /// The median is inside the band.
    Pass,
    /// The median is outside the band.
    Fail,
}

/// The evaluation of one band (SPEC-011 section 4.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BandResult {
    /// Smallest sample.
    pub min: i64,
    /// Lower median of the samples.
    pub median: i64,
    /// Largest sample.
    pub max: i64,
    /// `1000 * (max - min) / max(median, 1)`.
    pub spread_pm: i64,
    /// 0 inside the band, else the distance to it in permille of its width.
    pub excess_pm: i64,
    /// The verdict, first match of the section 4.4 order.
    pub verdict: Verdict,
    /// A gated band wider than `weak_width_pm`.
    pub weak: bool,
    /// The band needs capture, which is absent: only its upper edge is judged.
    pub upper_only: bool,
}

/// Regiments of the unit list numbered by ascending unit id among regiment-level units
/// (amendment A1), with the descendants of any unit.
pub(crate) struct UnitIndex<'a> {
    units: Vec<&'a UnitRecord>,
    regiments: Vec<usize>,
}

/// A regiment of the set: its `RegimentId` and its record.
pub(crate) type SetEntry<'a> = (u32, &'a UnitRecord);

impl<'a> UnitIndex<'a> {
    pub(crate) fn new(records: &'a [UnitRecord]) -> UnitIndex<'a> {
        let mut units: Vec<&UnitRecord> = records.iter().collect();
        units.sort_by(|a, b| a.id.cmp(&b.id));
        let regiments = (0..units.len())
            .filter(|&i| units[i].level.is_regiment())
            .collect();
        UnitIndex { units, regiments }
    }

    fn find(&self, id: &str) -> Option<usize> {
        self.units.binary_search_by(|u| u.id.as_str().cmp(id)).ok()
    }

    /// The regiments at or below `unit`, ascending id; `None` when `unit` is not in the list.
    pub(crate) fn regiments_under(&self, unit: &str) -> Option<Vec<SetEntry<'a>>> {
        self.find(unit)?;
        let under = |mut at: usize| loop {
            if self.units[at].id == unit {
                return true;
            }
            match self.find(&self.units[at].parent) {
                Some(p) => at = p,
                None => return false,
            }
        };
        Some(
            self.regiments
                .iter()
                .enumerate()
                .filter(|&(_, &u)| under(u))
                .map(|(rank, &u)| (rank as u32, self.units[u]))
                .collect(),
        )
    }
}

/// True for the kinds a basis counts. No `Captured` kind exists yet, so both bases count killed
/// and wounded; a `Disabled` soldier is down for a while and not a loss.
pub(crate) fn counts_in(basis: Basis, kind: CasualtyKind) -> bool {
    match basis {
        Basis::Kw | Basis::Total => matches!(kind, CasualtyKind::Killed | CasualtyKind::Wounded),
    }
}

fn permille(count: i64, strength: i64) -> i64 {
    if strength <= 0 {
        0
    } else {
        1000 * count / strength
    }
}

fn in_window(log: &[Casualty], lo: u64, hi: u64, basis: Basis, regiment: u32) -> i64 {
    log.iter()
        .filter(|c| c.unit == regiment && (lo..hi).contains(&c.tick) && counts_in(basis, c.kind))
        .count() as i64
}

/// The value of `band` in the driver's casualty log (SPEC-011 section 4.3). Strength is the sum of
/// the strengths of the unit's regiments, which stand on the field from tick 0 until the scenario
/// hands over arrival rows.
///
/// Errors: [`ScriptError::Reference`] when the band's unit is not in `units`.
pub fn try_measure(
    band: &Band,
    driver: &ReplayDriver,
    units: &[UnitRecord],
) -> Result<i64, ScriptError> {
    let set = UnitIndex::new(units)
        .regiments_under(&band.unit)
        .ok_or_else(|| ScriptError::Reference(format!("band {} unit {}", band.id, band.unit)))?;
    let tick = |s: i64| u64::try_from(s - driver.clock_start_s()).unwrap_or(0) * u64::from(TICK_HZ);
    let (lo, hi) = (tick(band.from_s), tick(band.to_s));
    let log = driver.casualties();
    let per_regiment: Vec<(i64, i64)> = set
        .iter()
        .map(|&(r, u)| (in_window(log, lo, hi, band.basis, r), i64::from(u.strength)))
        .collect();
    Ok(match band.metric {
        Metric::Count => per_regiment.iter().map(|p| p.0).sum(),
        Metric::RatePm => permille(
            per_regiment.iter().map(|p| p.0).sum(),
            per_regiment.iter().map(|p| p.1).sum(),
        ),
        Metric::RatePmMedian => {
            let mut rates: Vec<i64> = per_regiment
                .iter()
                .filter(|p| p.1 > 0)
                .map(|p| permille(p.0, p.1))
                .collect();
            rates.sort_unstable();
            rates.len().checked_sub(1).map_or(0, |n| rates[n / 2])
        }
    })
}

/// [`try_measure`], with 0 for a band whose unit is not in the list.
pub fn measure(band: &Band, driver: &ReplayDriver, units: &[UnitRecord]) -> i64 {
    try_measure(band, driver, units).unwrap_or(0)
}

/// How far `median` lies outside the band; an upper-only band has no lower edge.
fn distance(band: &Band, median: i64, upper_only: bool) -> i64 {
    match band.high {
        Some(h) if median > h => median - h,
        _ if !upper_only && median < band.low => band.low - median,
        _ => 0,
    }
}

/// `distance` in permille of the band's width (an open or zero-width band divides by its low
/// edge, at least 1).
fn excess(band: &Band, distance: i64) -> i64 {
    let width = match band.high {
        Some(h) if h > band.low => h - band.low,
        _ => band.low.max(1),
    };
    (1000 * i128::from(distance) / i128::from(width)) as i64
}

/// Judges `band` over the per-seed `samples` (any order; the median is the lower median).
///
/// Verdicts, first match: `NotMeasurable` (a needed capability is absent; a `total` band that
/// needs only `capture` is instead measured as killed and wounded and judged on its upper edge,
/// marked `upper_only`), `Report` (the band does not gate), `Unstable` (at least three samples
/// and a spread above `spread_limit_pm`), `Pass`, `Fail`. No samples is `NotMeasurable`.
pub fn evaluate(band: &Band, samples: &[i64], caps: &Capabilities, policy: &Policy) -> BandResult {
    let mut v = samples.to_vec();
    v.sort_unstable();
    let (Some(&min), Some(&max)) = (v.first(), v.last()) else {
        return BandResult {
            min: 0,
            median: 0,
            max: 0,
            spread_pm: 0,
            excess_pm: 0,
            verdict: Verdict::NotMeasurable,
            weak: false,
            upper_only: false,
        };
    };
    let median = v[(v.len() - 1) / 2];
    let spread_pm = (1000 * i128::from(max - min) / i128::from(median.max(1))) as i64;
    let missing = band.needs.is_some_and(|n| !caps.has(n));
    let upper_only = missing && band.needs == Some(Need::Capture) && band.basis == Basis::Total;
    let weak = band.gates()
        && band.high.is_some_and(|h| {
            1000 * i128::from(h - band.low) / i128::from(band.low.max(1))
                > i128::from(policy.weak_width_pm)
        });
    let outside = distance(band, median, upper_only);
    let excess_pm = excess(band, outside);
    let verdict = if missing && !upper_only {
        Verdict::NotMeasurable
    } else if !band.gates() {
        Verdict::Report
    } else if v.len() >= MIN_SEEDS_FOR_SPREAD && spread_pm > policy.spread_limit_pm {
        Verdict::Unstable
    } else if outside == 0 {
        Verdict::Pass
    } else {
        Verdict::Fail
    };
    BandResult {
        min,
        median,
        max,
        spread_pm,
        excess_pm,
        verdict,
        weak,
        upper_only,
    }
}
