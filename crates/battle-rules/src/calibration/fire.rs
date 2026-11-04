//! The fire calibration harness (SPEC-005 section 9): `cal_line`, `cal_rof`, `cal_range` and
//! `cal_canister` runs reduced to the metrics of bands B1 to B11.
//!
//! Every metric is an integer. A band holds when the mean over the seeds is inside it; the
//! per-seed minimum and maximum are reported next to it.

use crate::calibration::bands::{parse_bands, Band, Point};
use crate::scenarios::fire::{cal_canister, cal_line, cal_range, cal_rof, LINE_MEN};
use crate::{aim::SHOT_WORDS, BattleState, CasualtyKind, DataError, Tables};
use crate::{Training, WeaponClass};
use core_math::{Angle, Rng};

/// The master seeds of a band run (SPEC-005 section 9).
pub const SEEDS: [u64; 8] = [1, 2, 3, 4, 5, 6, 7, 8];
/// Length of a `cal_line` run in ticks.
pub const LINE_TICKS: u64 = 2400;
/// Casualty curves are sampled this often, in ticks.
pub const SAMPLE_EVERY: u64 = 300;
/// Ticks of a `cal_rof` run: six minutes for a musket, ten for a gun.
const ROF_TICKS: u64 = 7200;
const GUN_ROF_TICKS: u64 = 12_000;
/// Rounds fired per seed in `cal_canister` (8 seeds give the 200 rounds of the spec).
const CANISTER_ROUNDS_PER_SEED: u64 = 25;
/// Ticks a canister round is followed after the shot.
const CANISTER_TICKS: u64 = 80;
/// A ratio whose denominator is zero while the numerator is not.
const RATIO_CAP: i64 = 1_000_000;

/// What one `cal_line` run produced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineRun {
    /// Master seed.
    pub seed: u64,
    /// Casualties by victim team (`[team][killed, wounded, disabled]`).
    pub casualties: [[u32; 3]; 2],
    /// Casualties whose shooter was on the victim's own team.
    pub friendly: u32,
    /// Casualties of both teams at ticks `SAMPLE_EVERY`, `2 * SAMPLE_EVERY`, and so on.
    pub by_tick: Vec<u32>,
    /// Shots taken (misfires included) and misfires.
    pub shots: u64,
    /// Misfires.
    pub misfires: u64,
    /// State hash at the end of the run.
    pub hash: u64,
}

impl LineRun {
    /// Casualties of one team, or of both with `None`.
    pub fn total(&self, team: Option<usize>) -> u32 {
        (0..2)
            .filter(|&t| team.is_none_or(|x| x == t))
            .map(|t| self.casualties[t].iter().sum::<u32>())
            .sum()
    }

    /// Killed and wounded over both teams.
    pub fn killed_wounded(&self) -> (u32, u32) {
        (
            self.casualties[0][0] + self.casualties[1][0],
            self.casualties[0][1] + self.casualties[1][1],
        )
    }

    /// Casualties per 100 men of both lines, times 1000.
    pub fn per_100_milli(&self) -> i64 {
        i64::from(self.total(None)) * 100_000 / (2 * LINE_MEN as i64)
    }
}

/// The runs of one `cal_line` parameter set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineSet {
    /// Weapon row id.
    pub weapon: String,
    /// Training of both lines.
    pub training: Training,
    /// Distance between the lines.
    pub range_mm: u32,
    /// Whether team 1 stood behind a wall.
    pub wall: bool,
    /// One run per seed, in seed order.
    pub runs: Vec<LineRun>,
}

/// Runs `f` for every seed on its own thread and returns the results in seed order.
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

fn summarise(b: &BattleState, seed: u64) -> LineRun {
    let mut run = LineRun {
        seed,
        casualties: [[0; 3]; 2],
        friendly: 0,
        by_tick: vec![0; (LINE_TICKS / SAMPLE_EVERY) as usize],
        shots: b.counters().shots_fired,
        misfires: b.counters().misfires,
        hash: b.hash(),
    };
    for c in b.casualties() {
        let kind = match c.kind {
            CasualtyKind::Killed => 0,
            CasualtyKind::Wounded => 1,
            CasualtyKind::Disabled => 2,
        };
        run.casualties[usize::from(c.team & 1)][kind] += 1;
        let shooter = b.regiments.get(c.shooter_unit as usize).map(|r| r.team);
        run.friendly += u32::from(shooter == Some(c.team));
        let slot = (c.tick / SAMPLE_EVERY) as usize;
        for v in run.by_tick.iter_mut().skip(slot) {
            *v += 1;
        }
    }
    run
}

/// Runs `cal_line(weapon, training, range_mm, wall)` for each seed to tick 2400.
pub fn run_cal_line(
    tables: &Tables,
    weapon: &str,
    training: Training,
    range_mm: u32,
    wall: bool,
    seeds: &[u64],
) -> LineSet {
    let runs = per_seed(seeds, |seed| {
        let mut b = cal_line(tables, seed, weapon, training, range_mm, wall);
        for _ in 0..LINE_TICKS {
            b.step();
        }
        summarise(&b, seed)
    });
    LineSet {
        weapon: weapon.to_string(),
        training,
        range_mm,
        wall,
        runs,
    }
}

/// Rounds per soldier per minute times 1000 of `cal_rof`, one value per seed.
fn rof_milli(tables: &Tables, weapon: &str, training: Training, seeds: &[u64]) -> Vec<i64> {
    let gun = tables
        .weapons
        .index_of(weapon)
        .is_some_and(|i| tables.weapons.rows()[i].class == WeaponClass::FieldGun);
    let ticks = if gun { GUN_ROF_TICKS } else { ROF_TICKS };
    per_seed(seeds, |seed| {
        let mut b = cal_rof(tables, seed, weapon, training);
        let men = b.regiments[0].roster.len() as u64;
        for _ in 0..ticks {
            b.step();
        }
        (b.counters().shots_fired * 1200 * 1000 / (men * ticks)) as i64
    })
}

/// Casualties and rounds of `cal_canister(range_mm)` for one seed: 25 rounds, each fired at a
/// fresh line from the gun's own stream (round `k` reads draws `50 k` on).
fn canister_round_set(tables: &Tables, seed: u64, range_mm: u32) -> (u64, u64) {
    let base = cal_canister(tables, seed, range_mm);
    let gun = base.regiments[0].roster[0];
    let mut hits = 0;
    for k in 0..CANISTER_ROUNDS_PER_SEED {
        let mut b = base.clone();
        crate::scenarios::fire::make_ready(&mut b);
        let f = &mut b.fire[gun.index as usize];
        let mut rng = Rng::from_state(f.rng);
        for _ in 0..k * u64::from(SHOT_WORDS) {
            rng.next_u64();
        }
        f.rng = rng.state();
        for _ in 0..CANISTER_TICKS {
            b.step();
        }
        hits += b.casualties().len() as u64;
    }
    (hits, CANISTER_ROUNDS_PER_SEED)
}

/// One evaluated point of a band.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BandRow {
    /// Band id.
    pub id: String,
    /// The point: a distance in metres, a training level, an ammunition, or empty.
    pub label: String,
    /// The measured value of every seed, scaled by `scale`.
    pub per_seed: Vec<i64>,
    /// Mean over seeds (for ratios, the ratio of the sums).
    pub mean: i64,
    /// Smallest per-seed value.
    pub min: i64,
    /// Largest per-seed value.
    pub max: i64,
    /// Lower limit, scaled.
    pub low: Option<i64>,
    /// Upper limit, scaled.
    pub high: Option<i64>,
    /// The band's scale.
    pub scale: i64,
    /// Whether the mean is inside the limits.
    pub pass: bool,
}

/// Everything `run_band_suite` measured.
#[derive(Clone, Debug)]
pub struct SuiteReport {
    /// Family that was run.
    pub family: String,
    /// One row per band point, in file order (B4, B10, B11 last).
    pub rows: Vec<BandRow>,
    /// Every `cal_line` parameter set that was run.
    pub lines: Vec<LineSet>,
    /// `(range_mm, rounds, casualties)` of `cal_canister`.
    pub canister: Vec<(u32, u64, u64)>,
}

impl SuiteReport {
    /// True when every row holds.
    pub fn passed(&self) -> bool {
        self.rows.iter().all(|r| r.pass)
    }
}

fn mean(v: &[i64]) -> i64 {
    if v.is_empty() {
        0
    } else {
        v.iter().sum::<i64>() / v.len() as i64
    }
}

fn ratio_milli(a: i64, b: i64) -> i64 {
    match (a, b) {
        (0, 0) => 0,
        (_, 0) => RATIO_CAP,
        _ => (a * 1000 / b).min(RATIO_CAP),
    }
}

fn row(band: &Band, label: String, per_seed: Vec<i64>, mean: i64, p: &Point) -> BandRow {
    BandRow {
        id: band.id.clone(),
        label,
        min: per_seed.iter().copied().min().unwrap_or(mean),
        max: per_seed.iter().copied().max().unwrap_or(mean),
        per_seed,
        mean,
        low: p.low,
        high: p.high,
        scale: band.scale,
        pass: false,
    }
}

fn judged(mut r: BandRow) -> BandRow {
    r.pass = r.low.is_none_or(|l| r.mean >= l) && r.high.is_none_or(|h| r.mean <= h);
    r
}

fn training_of(name: &str) -> Training {
    match name {
        "raw" => Training::Raw,
        "veteran" => Training::Veteran,
        _ => Training::Trained,
    }
}

fn metres_label(mm: u32) -> String {
    format!("{} m", mm / 1000)
}

struct Suite<'a> {
    tables: &'a Tables,
    lines: Vec<LineSet>,
    canister: Vec<(u32, u64, u64)>,
}

impl Suite<'_> {
    /// The cached runs of a parameter set, running them on first use.
    fn line(&mut self, weapon: &str, range_mm: u32, wall: bool) -> &LineSet {
        let training = Training::Trained;
        let at = self
            .lines
            .iter()
            .position(|s| (s.weapon.as_str(), s.range_mm, s.wall) == (weapon, range_mm, wall));
        let i = at.unwrap_or_else(|| {
            let set = run_cal_line(self.tables, weapon, training, range_mm, wall, &SEEDS);
            self.lines.push(set);
            self.lines.len() - 1
        });
        &self.lines[i]
    }

    fn rows_of(&mut self, band: &Band) -> Vec<BandRow> {
        let mut out: Vec<BandRow> = Vec::new();
        for p in &band.points {
            let first = out.first().map(|r| r.mean);
            let mut p = p.clone();
            if let (Some(pm), Some(f)) = (p.high_of_first_pm, first) {
                p.high = Some(f * pm / 1000);
            }
            let r = match band.metric.as_str() {
                "casualties_per_100" => {
                    let s = self.line(&band.weapon, p.at_mm, false);
                    let v: Vec<i64> = s.runs.iter().map(LineRun::per_100_milli).collect();
                    let m = mean(&v);
                    row(band, metres_label(p.at_mm), v, m, &p)
                }
                "ratio" => {
                    let a: Vec<i64> = self
                        .line(&band.weapon, p.at_mm, false)
                        .runs
                        .iter()
                        .map(|r| i64::from(r.total(None)))
                        .collect();
                    let d: Vec<i64> = self
                        .line(&band.other, p.at_mm, false)
                        .runs
                        .iter()
                        .map(|r| i64::from(r.total(None)))
                        .collect();
                    let v = a.iter().zip(&d).map(|(&x, &y)| ratio_milli(x, y)).collect();
                    let m = ratio_milli(a.iter().sum(), d.iter().sum());
                    row(band, metres_label(p.at_mm), v, m, &p)
                }
                "wall_ratio" => {
                    let s = self.line(&band.weapon, p.at_mm, true);
                    let at = |t| s.runs.iter().map(move |r| i64::from(r.total(Some(t))));
                    let v = at(0).zip(at(1)).map(|(x, y)| ratio_milli(x, y)).collect();
                    let m = ratio_milli(at(0).sum(), at(1).sum());
                    row(band, metres_label(p.at_mm), v, m, &p)
                }
                "rof" => {
                    let v = rof_milli(self.tables, &band.weapon, training_of(&p.training), &SEEDS);
                    let m = mean(&v);
                    row(band, p.training.clone(), v, m, &p)
                }
                "range_to_ground" => {
                    let el = Angle::from_turns_fraction(p.elevation_cdeg as i32, 36_000);
                    let mm =
                        cal_range(self.tables, &p.ammo, el).map_or(0, |s| s.range_to_ground_mm());
                    let m = (mm + 500) / 1000;
                    row(band, p.ammo.clone(), vec![m], m, &p)
                }
                "canister_hits" => {
                    let sets = per_seed(&SEEDS, |s| canister_round_set(self.tables, s, p.at_mm));
                    let (hits, rounds) = sets.iter().fold((0, 0), |a, s| (a.0 + s.0, a.1 + s.1));
                    self.canister.push((p.at_mm, rounds, hits));
                    let v = sets.iter().map(|s| (s.0 * 1000 / s.1) as i64).collect();
                    let m = (hits * 1000 / rounds) as i64;
                    row(band, metres_label(p.at_mm), v, m, &p)
                }
                _ => continue,
            };
            out.push(judged(r));
        }
        out
    }

    /// Bands that read every run (B10, B11) or earlier rows (B4).
    fn derived(&self, band: &Band, done: &[BandRow]) -> Vec<BandRow> {
        let p = band.points.first().cloned().unwrap_or_else(|| Point {
            at_mm: 0,
            training: String::new(),
            ammo: String::new(),
            elevation_cdeg: 0,
            low: None,
            high: None,
            high_of_first_pm: None,
        });
        let runs = || self.lines.iter().flat_map(|s| s.runs.iter());
        let r = match band.metric.as_str() {
            "monotone" => {
                let mut rises = 0;
                for id in [band.weapon.as_str(), band.other.as_str()] {
                    let m: Vec<i64> = done.iter().filter(|r| r.id == id).map(|r| r.mean).collect();
                    rises += m.windows(2).filter(|w| w[1] > w[0]).count() as i64;
                }
                row(band, "rises".into(), vec![rises], rises, &p)
            }
            "killed_share" => {
                let share = |r: &LineRun| {
                    let (k, w) = r.killed_wounded();
                    (k + w > 0).then(|| i64::from(k) * 1000 / i64::from(k + w))
                };
                let v: Vec<i64> = runs().filter_map(share).collect();
                let (k, w) = runs()
                    .map(LineRun::killed_wounded)
                    .fold((0, 0), |a, x| (a.0 + i64::from(x.0), a.1 + i64::from(x.1)));
                row(band, "all runs".into(), v, ratio_milli(k, k + w), &p)
            }
            "friendly_share" => {
                let all: i64 = runs().map(|r| i64::from(r.total(None))).sum();
                let own: i64 = runs().map(|r| i64::from(r.friendly)).sum();
                let v = runs()
                    .filter(|r| r.total(None) > 0)
                    .map(|r| ratio_milli(i64::from(r.friendly), i64::from(r.total(None))))
                    .collect();
                row(band, "all runs".into(), v, ratio_milli(own, all), &p)
            }
            _ => return Vec::new(),
        };
        vec![judged(r)]
    }
}

/// Runs the bands of `family` (`fire` for B1 to B11) described by the text of the band file and
/// returns every measured row.
///
/// Errors: [`DataError`] when the band file does not parse.
pub fn run_band_suite(
    family: &str,
    tables: &Tables,
    bands_toml: &str,
) -> Result<SuiteReport, DataError> {
    let bands: Vec<Band> = parse_bands(bands_toml)?
        .into_iter()
        .filter(|b| b.family == family)
        .collect();
    let mut suite = Suite {
        tables,
        lines: Vec::new(),
        canister: Vec::new(),
    };
    let derived = |m: &str| matches!(m, "monotone" | "killed_share" | "friendly_share");
    let mut rows = Vec::new();
    for band in bands.iter().filter(|b| !derived(&b.metric)) {
        rows.extend(suite.rows_of(band));
    }
    for band in bands.iter().filter(|b| derived(&b.metric)) {
        let more = suite.derived(band, &rows);
        rows.extend(more);
    }
    Ok(SuiteReport {
        family: family.to_string(),
        rows,
        lines: suite.lines,
        canister: suite.canister,
    })
}
