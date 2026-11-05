//! Headquarters sight (SPEC-009 section 8): observers, the per-team sighting log and the
//! period step that fills it, plus an adapter that offers the sight query to the command layer
//! through its `Visibility` trait (ADR 0010, D-03).
//!
//! Every regiment is an observer at its anchor; headquarters and signal stations are registered
//! with [`BattleState::add_observer`]. An observer runs once per sight period, on the tick whose
//! residue modulo the period equals its id's. The log is the only thing headquarters may read
//! about the enemy.

use crate::command::Visibility;
use crate::fire::posture_of;
use crate::{aim_mm, distance_mm, sight, BattleState, RegimentId, SightWorld, WeatherNow};
use core_math::{Fx, Vec2Fx};
use std::collections::BTreeMap;

/// Ids of registered observers start here; regiment observers use their regiment id.
pub const OBSERVER_ID_BASE: u16 = 0x8000;
/// Eye height of a regiment observer, mm.
const REGIMENT_EYE_MM: i32 = 1700;
/// Vision of a regiment observer, mm.
const REGIMENT_VISION_MM: u32 = 3_000_000;

/// A place from which the enemy is looked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Observer {
    /// Regiment id for a regiment, otherwise from [`OBSERVER_ID_BASE`]; sets the schedule.
    pub id: u16,
    /// The side the observer reports to.
    pub team: u8,
    /// Position in metres.
    pub pos: Vec2Fx,
    /// Eye height above local ground, mm.
    pub eye_mm: i32,
    /// Nominal range, mm; the effective range is also capped by clear visibility times light.
    pub vision_mm: u32,
}

/// What one observer made of one enemy regiment in one period.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sighting {
    /// Who saw it.
    pub observer: u16,
    /// Whom.
    pub enemy: RegimentId,
    /// The tick of the observation.
    pub tick: u64,
    /// Men seen, scaled from the sample to the whole roster.
    pub men_seen: u16,
    /// Transmission weighted by nearness, permille.
    pub quality_pm: u16,
    /// Mean position of the visible men sampled.
    pub centroid: Vec2Fx,
}

impl Sighting {
    /// Ticks since the sighting.
    pub fn staleness(&self, now: u64) -> u64 {
        now.saturating_sub(self.tick)
    }
}

/// One team's sightings: a slot per enemy regiment, newest wins, ascending by enemy id.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SightLog {
    entries: Vec<Sighting>,
}

impl SightLog {
    /// The entries, ascending by enemy id.
    pub fn entries(&self) -> &[Sighting] {
        &self.entries
    }

    /// The entry for `enemy`.
    pub fn get(&self, enemy: RegimentId) -> Option<&Sighting> {
        self.entries.iter().find(|s| s.enemy == enemy)
    }

    /// Puts `s` in the slot of its enemy, replacing what was there.
    pub fn record(&mut self, s: Sighting) {
        match self.entries.binary_search_by_key(&s.enemy, |e| e.enemy) {
            Ok(i) => self.entries[i] = s,
            Err(i) => self.entries.insert(i, s),
        }
    }
}

/// The registered observers (ascending id) and the logs by team (ascending team).
#[derive(Clone, Debug, Default)]
pub(crate) struct Observers {
    pub(crate) list: Vec<Observer>,
    pub(crate) logs: BTreeMap<u8, SightLog>,
}

impl BattleState {
    /// Registers a headquarters or signal station. Returns `false`, registering nothing, when the
    /// id is below [`OBSERVER_ID_BASE`] or already taken.
    pub fn add_observer(&mut self, o: Observer) -> bool {
        let list = &mut self.observers.list;
        if o.id < OBSERVER_ID_BASE {
            return false;
        }
        match list.binary_search_by_key(&o.id, |e| e.id) {
            Ok(_) => false,
            Err(i) => {
                list.insert(i, o);
                true
            }
        }
    }

    /// The sighting log of `team`, ascending by enemy regiment; empty when nothing was seen.
    pub fn sightings(&self, team: u8) -> &[Sighting] {
        self.observers.logs.get(&team).map_or(&[], |l| l.entries())
    }

    /// Writes a sighting into the log of `team` (test hook).
    #[doc(hidden)]
    pub fn record_sighting(&mut self, team: u8, s: Sighting) {
        self.observers.logs.entry(team).or_default().record(s);
    }

    /// Step TK-13, observer part (SPEC-009 section 8): every observer whose id matches the tick
    /// modulo the sight period tests up to `observer_samples` men of each enemy regiment within
    /// its range and logs what it saw. Does nothing with `env` `None`.
    pub(crate) fn step_observers(&mut self) {
        let BattleState {
            world,
            regiments,
            smoke,
            obstacles,
            env,
            tables,
            observers,
            ..
        } = self;
        let Some(env) = env.as_ref() else {
            return;
        };
        let (table, now) = (env.table(), env.now());
        let sp = &table.sight;
        let period = u64::from(sp.period_ticks.max(1));
        let phase = world.tick % period;
        let reach = i64::from(now.clear_vis_mm) * i64::from(now.light_pm) / 1000;
        let mut due: Vec<Observer> = regiments
            .iter()
            .filter(|r| u64::from(r.id.0) % period == phase && !r.roster.is_empty())
            .map(|r| Observer {
                id: r.id.0,
                team: r.team,
                pos: r.anchor,
                eye_mm: REGIMENT_EYE_MM,
                vision_mm: REGIMENT_VISION_MM,
            })
            .collect();
        due.extend(
            observers
                .list
                .iter()
                .filter(|o| u64::from(o.id) % period == phase),
        );
        if due.is_empty() {
            return;
        }
        let sw = SightWorld::new(env.relief(), obstacles, smoke, &table.smoke, sp);
        let want = sp.observer_samples as usize;
        for o in due {
            let range = i64::from(o.vision_mm).min(reach);
            if range <= 0 {
                continue;
            }
            for e in regiments.iter().filter(|e| e.team != o.team) {
                let len = e.roster.len();
                let m = want.min(len);
                if m == 0 || distance_mm(o.pos, e.anchor) > range {
                    continue;
                }
                let (mut seen, mut quality, mut sx, mut sy) = (0i64, 0i64, 0i128, 0i128);
                for k in 0..m {
                    let id = e.roster[k * len / m];
                    let (Some(pos), Some(flags)) = (world.agents.pos(id), world.agents.flags(id))
                    else {
                        continue;
                    };
                    let aim = aim_mm(&tables.damage, posture_of(flags));
                    let s = sight(&sw, o.pos, o.eye_mm, pos, aim, sp.far_step_mm);
                    if !s.visible {
                        continue;
                    }
                    let near = 1000 - (distance_mm(o.pos, pos) * 1000 / range).min(1000);
                    seen += 1;
                    quality += i64::from(s.transmit_pm) * near / 1000;
                    sx += i128::from(pos.x.raw());
                    sy += i128::from(pos.y.raw());
                }
                if seen == 0 {
                    continue;
                }
                let n = i128::from(seen);
                let log = observers.logs.entry(o.team).or_default();
                log.record(Sighting {
                    observer: o.id,
                    enemy: e.id,
                    tick: world.tick,
                    men_seen: u16::try_from(len as u64 * seen as u64 / m as u64)
                        .unwrap_or(u16::MAX),
                    quality_pm: u16::try_from(quality / seen).unwrap_or(u16::MAX),
                    centroid: Vec2Fx::new(
                        Fx::from_raw((sx / n) as i64),
                        Fx::from_raw((sy / n) as i64),
                    ),
                });
            }
        }
    }
}

/// The sight query as the command layer's visibility oracle: line of sight with relief, walls
/// and smoke between a fixed observer eye height and target aim height, transmission, and a
/// range capped by clear visibility times light.
pub struct SightVisibility<'a> {
    world: SightWorld<'a>,
    step_mm: u32,
    eye_mm: i32,
    aim_mm: i32,
    reach_mm: i64,
}

impl<'a> SightVisibility<'a> {
    /// An oracle over `world` sampling every `step_mm`, with the weather of the moment.
    pub fn new(
        world: SightWorld<'a>,
        step_mm: u32,
        eye_mm: i32,
        aim_mm: i32,
        weather: &WeatherNow,
    ) -> SightVisibility<'a> {
        SightVisibility {
            world,
            step_mm,
            eye_mm,
            aim_mm,
            reach_mm: i64::from(weather.clear_vis_mm) * i64::from(weather.light_pm) / 1000,
        }
    }
}

impl Visibility for SightVisibility<'_> {
    fn transmit_pm(&self, observer: Vec2Fx, target: Vec2Fx) -> u32 {
        let s = sight(
            &self.world,
            observer,
            self.eye_mm,
            target,
            self.aim_mm,
            self.step_mm,
        );
        u32::from(s.transmit_pm)
    }

    fn visible(&self, observer: Vec2Fx, target: Vec2Fx) -> bool {
        sight(
            &self.world,
            observer,
            self.eye_mm,
            target,
            self.aim_mm,
            self.step_mm,
        )
        .visible
    }

    fn range_mm(&self, _observer: Vec2Fx, nominal_mm: i64) -> i64 {
        nominal_mm.min(self.reach_mm)
    }
}
