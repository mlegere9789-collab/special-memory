//! The command calibration harness (SPEC-007 sections 10 and 13, T7-15): delivery, friction,
//! interpretation, chain and observation runs reduced to the quantities of CT-007-1, CT-007-2,
//! CT-007-3, CT-007-6 and CT-007-7 and compared with `qc/calibration/command-bands.toml`.
//!
//! The band file reuses the `bands/1` rows. `metric` names the quantity, `weapon` the variant
//! (a medium, or the officer of `cemetery_hill`) and a point's `at_mm` the distance it is
//! measured at. Delivery and latency are in milli-minutes (1200 ticks to the minute), friction
//! is per 100 legs, shares and confidences are fractions, all times the band's `scale`. A band
//! holds when the aggregate over the seeds (the mean, or the median for `chain` and
//! `observe_latency`) is inside it; the per-seed minimum and maximum are reported beside it.
//! Nothing here changes a rule or a table: a missed band is reported, never tuned away.

use crate::calibration::bands::{parse_bands, Band, Point};
use crate::calibration::fire::{BandRow, SuiteReport};
use crate::command::{CommandTable, Latitude, Medium, ModKind, OfficerId, Truth};
use crate::registry::T_LOW_DISC;
use crate::scenarios::command::{
    brigade, cemetery_hill, command_line, command_tree, division, regiment_officer, ARMY, CORPS,
};
use crate::{BattleState, DataError, Order, Tables};
use core_math::{Fx, Vec2Fx};

const TICKS_PER_MINUTE: i64 = 1200;
/// Legs sent per seed when counting loss and garbling.
const LEGS: usize = 100;
/// Ticks a run may last before its quantity counts as never reached (one hour).
const HORIZON: u64 = 72_000;

/// Milli-minutes in `ticks`.
fn milli_min(ticks: u64) -> i64 {
    ticks as i64 * 1000 / TICKS_PER_MINUTE
}

fn march() -> Order {
    let at = Vec2Fx::new(Fx::from_int(4000), Fx::from_int(3200));
    Order::Move {
        to: at,
        gait: crate::Gait::Quick,
    }
}

/// Steps the command layer (without movement or fire) from the current tick until `done` holds
/// after a step or the horizon passes; returns the tick at which it held.
fn drive(
    b: &mut BattleState,
    table: &CommandTable,
    mut done: impl FnMut(&BattleState) -> bool,
) -> Option<u64> {
    while b.world.tick < HORIZON {
        b.step_command(table);
        if done(b) {
            return Some(b.world.tick);
        }
        b.world.advance_tick();
    }
    None
}

fn send(b: &mut BattleState, t: &CommandTable, from: u16, to: u16, m: Option<Medium>) {
    b.dispatch(
        t,
        OfficerId(from),
        OfficerId(to),
        m,
        march(),
        Latitude::Strict,
    )
    .expect("harness officers share a side and are up");
}

fn medium_of(name: &str) -> (Option<Medium>, Option<Medium>) {
    match name {
        "flags" => (Some(Medium::Flags), Some(Medium::Flags)),
        "telegraph" => (Some(Medium::Telegraph), Some(Medium::Telegraph)),
        _ => (Some(Medium::Courier), None),
    }
}

/// A table with loss, exposure and garbling off, to isolate latency or interpretation.
fn quiet(t: &CommandTable) -> CommandTable {
    let mut q = t.clone();
    q.media.iter_mut().for_each(|m| {
        m.loss_base_ppm = 0;
        m.garble_ppm = 0;
    });
    q.globals.loss_ppm_per_km = 0;
    q.globals.exposure_ppm_per_enemy = 0;
    q
}

fn delivered(b: &BattleState, from: u16) -> bool {
    let cmd = b.cmd.as_ref().expect("command layer is on");
    cmd.records
        .iter()
        .any(|r| r.from == OfficerId(from) && r.truth == Truth::Delivered)
}

/// Ticks from sending until an order sent by `from` is read, by `medium` over `n - 1` hops of
/// `d_mm`; none when it is lost.
fn delivery(
    tb: &Tables,
    t: &CommandTable,
    seed: u64,
    medium: &str,
    d_mm: u32,
    hops: usize,
) -> Option<i64> {
    let (m, link) = medium_of(medium);
    let mut b = command_line(tb, seed, hops + 1, i64::from(d_mm) / 1000, link);
    send(&mut b, t, 1, 2, m);
    drive(&mut b, t, |b| delivered(b, hops as u16)).map(milli_min)
}

/// Lost and garbled legs among `LEGS` couriers of `d_mm`, as counts times 1000.
fn friction(tb: &Tables, t: &CommandTable, seed: u64, d_mm: u32, garbled: bool) -> Option<i64> {
    let mut b = command_line(tb, seed, 2, i64::from(d_mm) / 1000, None);
    for _ in 0..LEGS {
        send(&mut b, t, 1, 2, Some(Medium::Courier));
    }
    drive(&mut b, t, |b| b.world.tick >= 30_000);
    let records = &b.cmd.as_ref()?.records;
    let n = records
        .iter()
        .filter(|r| match garbled {
            true => r.garble.is_some(),
            false => r.truth == Truth::Lost,
        })
        .count();
    Some(n as i64 * 1000)
}

/// 1000 when the officer of `cemetery_hill` does what the case expects (a tired officer
/// declines, a rested one goes in), else 0.
fn hill(tb: &Tables, t: &CommandTable, seed: u64, tired: bool) -> Option<i64> {
    let (fatigue, mask) = if tired { (600_000, T_LOW_DISC) } else { (0, 0) };
    let q = quiet(t);
    let mut b = cemetery_hill(tb, &q, seed, fatigue, crate::command::TraitMask(mask));
    drive(&mut b, &q, |b| {
        b.cmd.as_ref().is_some_and(|c| c.records[0].ack.is_some())
    })?;
    let declined = b.cmd.as_ref()?.records[0].ack?.kind == ModKind::Declined;
    Some(if declined == tired { 1000 } else { 0 })
}

fn median(v: &mut [i64]) -> i64 {
    v.sort_unstable();
    (v[(v.len() - 1) / 2] + v[v.len() / 2]) / 2
}

/// Per regiment `k` of the tree, the tick its order first became a move.
fn arrivals(b: &mut BattleState, t: &CommandTable, regs: std::ops::Range<usize>) -> Vec<u64> {
    let mut at = vec![None; regs.len()];
    drive(b, t, |b| {
        for (slot, k) in at.iter_mut().zip(regs.clone()) {
            if slot.is_none() && matches!(b.regiments[k].order, Order::Move { .. }) {
                *slot = Some(b.world.tick);
            }
        }
        at.iter().all(Option::is_some)
    });
    at.into_iter().flatten().collect()
}

/// Median over the 27 regiments of the time the army's move reaches them (CT-007-6).
fn chain(tb: &Tables, t: &CommandTable, seed: u64) -> Option<i64> {
    let q = quiet(t);
    let mut b = command_tree(tb, seed, false);
    send(&mut b, &q, ARMY, CORPS, Some(Medium::Courier));
    let at = arrivals(&mut b, &q, 0..27);
    (!at.is_empty())
        .then(|| milli_min(median(&mut at.iter().map(|&x| x as i64).collect::<Vec<_>>()) as u64))
}

/// Latest of the nine regiments of a division to get the division's move (CT-007-6).
fn division_reach(tb: &Tables, t: &CommandTable, seed: u64) -> Option<i64> {
    let q = quiet(t);
    let mut b = command_tree(tb, seed, false);
    for k in 0..3 {
        send(&mut b, &q, division(0), brigade(k), None);
    }
    arrivals(&mut b, &q, 0..9).into_iter().max().map(milli_min)
}

/// Ticks until a regiment 1.5 km from the enemy has put it in the army's picture (CT-007-7), or
/// the confidence of that regiment's own sighting at the first observation.
fn observe(tb: &Tables, t: &CommandTable, seed: u64, confidence: bool) -> Option<i64> {
    let mut b = command_tree(tb, seed, true);
    if confidence {
        b.step_command(t);
        let pic = b.picture_view(t, OfficerId(regiment_officer(26)))?;
        return pic
            .contacts
            .iter()
            .map(|c| i64::from(c.confidence_pm))
            .max();
    }
    let seen = |b: &BattleState| {
        b.cmd
            .as_ref()
            .is_some_and(|c| !c.pics[0].contacts.is_empty())
    };
    drive(&mut b, t, seen).map(milli_min)
}

fn per_seed<T: Send>(seeds: &[u64], f: impl Fn(u64) -> T + Sync) -> Vec<T> {
    let f = &f;
    std::thread::scope(|s| {
        let handles: Vec<_> = seeds.iter().map(|&seed| s.spawn(move || f(seed))).collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("a calibration run does not panic"))
            .collect()
    })
}

fn row(
    band: &Band,
    p: &Point,
    seeds: &[u64],
    tb: &Tables,
    t: &CommandTable,
) -> Result<BandRow, DataError> {
    let (variant, d) = (band.weapon.as_str(), p.at_mm);
    let one: Box<dyn Fn(u64) -> Option<i64> + Sync> = match band.metric.as_str() {
        "delivery" => Box::new(move |s| delivery(tb, t, s, variant, d, 1)),
        "relay_delivery" => Box::new(move |s| delivery(tb, t, s, variant, d, 2)),
        "lost_per_100" => Box::new(move |s| friction(tb, t, s, d, false)),
        "garbled_per_100" => Box::new(move |s| friction(tb, t, s, d, true)),
        "decline_share" => Box::new(move |s| hill(tb, t, s, true)),
        "go_share" => Box::new(move |s| hill(tb, t, s, false)),
        "chain" => Box::new(move |s| chain(tb, t, s)),
        "division_reach" => Box::new(move |s| division_reach(tb, t, s)),
        "observe_latency" => Box::new(move |s| observe(tb, t, s, false)),
        "observe_confidence" => Box::new(move |s| observe(tb, t, s, true)),
        other => {
            return Err(DataError::Parse(format!(
                "unknown command metric {other:?}"
            )))
        }
    };
    let mut v: Vec<i64> = per_seed(seeds, one).into_iter().flatten().collect();
    if v.is_empty() {
        return Err(DataError::Range(format!(
            "band {}: no seed gave a value",
            band.id
        )));
    }
    let (min, max) = (
        v.iter().copied().min().unwrap_or(0),
        v.iter().copied().max().unwrap_or(0),
    );
    let agg = if matches!(band.metric.as_str(), "chain" | "observe_latency") {
        median(&mut v)
    } else {
        v.iter().sum::<i64>() / v.len() as i64
    };
    let label = match (d, variant) {
        (0, v) => v.to_string(),
        (d, "") => format!("{} m", d / 1000),
        (d, v) => format!("{v} {} m", d / 1000),
    };
    Ok(BandRow {
        id: band.id.clone(),
        label,
        per_seed: v,
        mean: agg,
        min,
        max,
        low: p.low,
        high: p.high,
        scale: band.scale,
        pass: p.low.is_none_or(|l| agg >= l) && p.high.is_none_or(|h| agg <= h),
    })
}

/// Runs every band of family `command` in the text of the band file over `seeds` and returns the
/// measured rows in file order.
///
/// Errors: [`DataError`] when the band file does not parse, a metric is unknown or no seed gave a
/// value for a band.
pub fn run_command_bands(
    tables: &Tables,
    command: &CommandTable,
    bands_toml: &str,
    seeds: &[u64],
) -> Result<SuiteReport, DataError> {
    let mut rows = Vec::new();
    for band in parse_bands(bands_toml)?
        .iter()
        .filter(|b| b.family == "command")
    {
        for p in &band.points {
            rows.push(row(band, p, seeds, tables, command)?);
        }
    }
    Ok(SuiteReport {
        family: "command".to_string(),
        rows,
        lines: Vec::new(),
        canister: Vec::new(),
    })
}
