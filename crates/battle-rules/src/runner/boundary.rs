//! The work of a boundary tick (SPEC-008 section 4): departures, arrivals, triggers, weather,
//! classification, hand-overs and the sector update, in that order.

use super::{metres, Battle};
use crate::scenario::{Arrival, Trigger, TriggerKind, TriggerOrder, Unit};
use crate::{
    classify, hand_over, slot_layout, Aggregate, Arm, BattleState, ClassInput, FireMode, Order,
    Regiment, RegimentId, RegimentState, RegimentStatus, Scenario, ScenarioError, SectorBody,
    SmokeGrid, Weather, F2,
};
use core_math::{Angle, Vec2Fx};
use std::collections::BTreeMap;

/// Soldiers a unit puts on the field: the men, or the guns of a battery (SPEC-008 section 2.3).
fn soldiers(u: &Unit) -> u32 {
    if u.arm == Arm::Battery {
        u.guns
    } else {
        u.men
    }
}

fn point(x_mm: i64, y_mm: i64) -> Vec2Fx {
    Vec2Fx::new(metres(x_mm), metres(y_mm))
}

/// Creates one regiment per non-headquarters unit, in regiment order: not arrived, standing at
/// its arrival point in its arrival formation, an empty aggregate, equipped with its weapon.
pub(super) fn add_regiments(state: &mut BattleState, s: &Scenario) -> Result<(), ScenarioError> {
    let arrivals: BTreeMap<&str, &Arrival> =
        s.arrivals.iter().map(|a| (a.unit.as_str(), a)).collect();
    for u in s.units.iter().filter(|u| u.arm != Arm::Hq) {
        let Some(a) = arrivals.get(u.id.as_str()) else {
            return Err(ScenarioError::Reference(format!(
                "unit {} has no arrival",
                u.id
            )));
        };
        let id = RegimentId(
            u16::try_from(state.regiments.len())
                .map_err(|_| ScenarioError::Range("more than 65536 regiments".to_owned()))?,
        );
        state.regiments.push(Regiment {
            id,
            team: u.side.team(),
            training: u.training,
            state: RegimentState::Steady(a.formation),
            anchor: point(a.x_mm, a.y_mm),
            heading: Angle::from_degrees_int(i32::from(a.heading_deg)),
            order: Order::Hold,
            pending: None,
            roster: Vec::new(),
            dirty: false,
            slots: Vec::new(),
            status: RegimentStatus::NotArrived,
            class: F2,
            since: 0,
            aggregate: Aggregate::default(),
            acct: crate::RoundsAccount::default(),
        });
        state.regfire.push(crate::fire::RegFire::NONE);
        state.equip_regiment(id, &u.weapon, 0)?;
    }
    Ok(())
}

/// Ceiling of `a / b` for a positive `b`.
fn ceil_div(a: i64, b: i64) -> i64 {
    (a + b - 1).div_euclid(b)
}

/// Smoke units in the cells whose centres lie in sector `(sx, sy)` of side `sector_mm`
/// (map-relative millimetres; the smoke grid shares the map origin).
fn sector_smoke(smoke: &SmokeGrid, sector_mm: i64, sx: i64, sy: i64) -> u64 {
    if smoke.density.is_empty() {
        return 0;
    }
    let cell = i64::from(smoke.cell_mm);
    let span = |s: i64, n: u32| {
        let lo = ceil_div(s * sector_mm - cell / 2, cell).clamp(0, i64::from(n));
        let hi = ceil_div((s + 1) * sector_mm - cell / 2, cell).clamp(0, i64::from(n));
        (lo as usize, hi as usize)
    };
    let (x0, x1) = span(sx, smoke.width);
    let (y0, y1) = span(sy, smoke.height);
    let w = smoke.width as usize;
    let mut total = 0u64;
    for y in y0..y1 {
        total += smoke.density[y * w + x0..y * w + x1]
            .iter()
            .map(|&d| u64::from(d))
            .sum::<u64>();
    }
    total
}

impl Battle {
    fn ticks_of(&self, at_s: u32) -> u64 {
        u64::from(at_s) * u64::from(self.scenario.tick_hz)
    }

    /// The boundary work for the current tick (SPEC-008 section 4, steps a to d).
    pub(super) fn boundary(&mut self) {
        let tick = self.state.world.tick;
        self.apply_departures(tick);
        self.apply_arrivals(tick);
        self.apply_triggers(tick);
        self.apply_weather(tick);
        self.classify_and_hand_over(tick);
        self.update_sectors();
    }

    fn apply_departures(&mut self, tick: u64) {
        while let Some(d) = self.scenario.departures.get(self.fid.departures_done) {
            if self.ticks_of(d.at_s) > tick {
                break;
            }
            let reg = self.scenario.unit(&d.unit).and_then(|u| u.regiment);
            self.fid.departures_done += 1;
            if let Some(id) = reg {
                self.depart(id, tick);
            }
        }
    }

    /// Removes a present regiment from the field: its agents are despawned, its aggregate is
    /// emptied (the killed count stays), and its soldiers and rounds go to the ledger as
    /// departed and lost.
    fn depart(&mut self, id: RegimentId, tick: u64) {
        let ri = usize::from(id.0);
        if self.state.regiments[ri].status != RegimentStatus::Present {
            return;
        }
        let equipped = self.state.regfire[ri].weapon.is_some();
        let members: Vec<_> = self
            .state
            .world
            .agents
            .iter_alive()
            .filter(|&a| self.state.world.agents.unit(a) == Some(u32::from(id.0)))
            .collect();
        let (mut count, mut rounds) = (members.len() as u64, 0u64);
        for a in members {
            if let (true, Some(f)) = (equipped, self.state.fire.get(a.index as usize)) {
                rounds += u64::from(f.rounds);
            }
            self.state.world.agents.despawn(a);
        }
        let reg = &mut self.state.regiments[ri];
        let agg = std::mem::take(&mut reg.aggregate);
        count += u64::from(agg.fit) + agg.disabled_count() + u64::from(agg.wounded);
        rounds += agg.rounds;
        reg.aggregate = Aggregate {
            killed: agg.killed,
            ..Aggregate::default()
        };
        reg.acct.lost += rounds;
        reg.roster.clear();
        reg.slots.clear();
        reg.dirty = false;
        reg.class = F2;
        reg.since = tick;
        reg.status = RegimentStatus::Departed;
        let team = usize::from(reg.team.min(1));
        self.fid.departed[team] += count;
    }

    fn apply_arrivals(&mut self, tick: u64) {
        while let Some(a) = self.scenario.arrivals.get(self.fid.arrivals_done) {
            if self.ticks_of(a.at_s) > tick {
                break;
            }
            let Some(unit) = self.scenario.unit(&a.unit) else {
                self.fid.arrivals_done += 1;
                continue;
            };
            let (n, rpm, team) = (soldiers(unit), unit.rounds_per_man, unit.side.team());
            let reg = unit.regiment;
            let at = (point(a.x_mm, a.y_mm), a.heading_deg, a.formation);
            self.fid.arrivals_done += 1;
            let Some(id) = reg else { continue };
            let rounds = u64::from(n) * u64::from(rpm);
            let r = &mut self.state.regiments[usize::from(id.0)];
            if r.status != RegimentStatus::NotArrived {
                continue;
            }
            r.anchor = at.0;
            r.heading = Angle::from_degrees_int(i32::from(at.1));
            r.state = RegimentState::Steady(at.2);
            r.status = RegimentStatus::Present;
            r.class = F2;
            r.since = tick;
            r.aggregate = Aggregate {
                fit: n,
                rounds,
                ..Aggregate::default()
            };
            r.acct.initial += rounds;
            self.fid.arrived[usize::from(team.min(1))] += u64::from(n);
        }
    }

    fn apply_triggers(&mut self, tick: u64) {
        while let Some(t) = self.scenario.triggers.get(self.fid.triggers_done) {
            if self.ticks_of(t.at_s) > tick {
                break;
            }
            let t = t.clone();
            self.fid.triggers_done += 1;
            self.fire_trigger(&t);
        }
    }

    /// Acts on a trigger. A trigger addressing a regiment that is not on the field does nothing.
    fn fire_trigger(&mut self, t: &Trigger) {
        let Some(id) = self.scenario.unit(&t.unit).and_then(|u| u.regiment) else {
            return;
        };
        let present = |b: &Battle, id: RegimentId| {
            b.state.regiments[usize::from(id.0)].status == RegimentStatus::Present
        };
        if !present(self, id) {
            return;
        }
        match &t.kind {
            TriggerKind::Order(o) => {
                let order = match *o {
                    TriggerOrder::Hold => Order::Hold,
                    TriggerOrder::Move { x_mm, y_mm, gait } => Order::Move {
                        to: point(x_mm, y_mm),
                        gait,
                    },
                    TriggerOrder::Form(f) => Order::Form(f),
                    TriggerOrder::Face(deg) => Order::Face(Angle::from_degrees_int(i32::from(deg))),
                };
                self.state.issue_order(id, order);
            }
            TriggerKind::FireMode { target } => {
                if let Some(target) = self.scenario.unit(target).and_then(|u| u.regiment) {
                    self.state.set_fire_mode(id, FireMode::AtWill { target });
                }
            }
            TriggerKind::Resupply { rounds } => {
                let n = u16::try_from(*rounds).unwrap_or(u16::MAX);
                let reg = &mut self.state.regiments[usize::from(id.0)];
                if reg.class == F2 {
                    let added = u64::from(n) * u64::from(reg.aggregate.fit);
                    reg.aggregate.rounds += added;
                    reg.acct.resupplied += added;
                } else {
                    self.state.resupply(id, n);
                }
            }
            TriggerKind::Marker => {}
        }
    }

    fn apply_weather(&mut self, tick: u64) {
        let row = self.weather_row_at(tick);
        if row == self.fid.weather_row {
            return;
        }
        self.fid.weather_row = row;
        if let (Some(w), None) = (
            row.and_then(|i| self.scenario.weather.get(i)),
            &self.state.env,
        ) {
            self.state.weather = Weather {
                wind: point(w.wind_x_mm_s, w.wind_y_mm_s),
                wet_pm: 1000 + w.wet_pm,
                ground_pm: 1000 - w.ground_pm / 2,
            };
        }
    }

    /// Reach of regiment `ri` in mm: the largest `|u|` or `|v|` of its slot layout (section 1).
    fn reach_mm(&self, ri: usize, fit: u32) -> u32 {
        let reg = &self.state.regiments[ri];
        let layout = slot_layout(
            reg.state.target_formation(),
            fit as usize,
            &self.state.tables.drill,
        );
        let mm = |v: core_math::Fx| ((i128::from(v.raw().unsigned_abs()) * 1000) >> 32) as u32;
        layout
            .iter()
            .map(|s| mm(s.u).max(mm(s.v)))
            .max()
            .unwrap_or(0)
    }

    /// Indices of the regiments on the field, ascending.
    fn present(&self) -> Vec<usize> {
        (0..self.state.regiments.len())
            .filter(|&i| self.state.regiments[i].status == RegimentStatus::Present)
            .collect()
    }

    fn fit_of(&self, ri: usize) -> u32 {
        let r = &self.state.regiments[ri];
        if r.class == F2 {
            r.aggregate.fit
        } else {
            u32::try_from(r.roster.len()).unwrap_or(u32::MAX)
        }
    }

    fn classify_and_hand_over(&mut self, tick: u64) {
        let present = self.present();
        let inputs: Vec<ClassInput> = present
            .iter()
            .map(|&ri| {
                let r = &self.state.regiments[ri];
                let fit = self.fit_of(ri);
                ClassInput {
                    id: r.id,
                    team: r.team,
                    arm: self.scenario.units[self.fid.reg_unit[ri]].arm,
                    anchor: r.anchor,
                    reach_mm: self.reach_mm(ri, fit),
                    fit,
                    class: r.class,
                    since: r.since,
                }
            })
            .collect();
        let classes = classify(
            &self.fid.table,
            &inputs,
            self.scenario.fidelity_class,
            self.scenario.f0_cap,
            self.scenario.agent_cap,
            tick,
        );
        for (&ri, to) in present.iter().zip(classes) {
            if self.state.regiments[ri].class != to {
                hand_over(&mut self.state, RegimentId(ri as u16), to);
            }
        }
    }

    fn update_sectors(&mut self) {
        let bodies: Vec<SectorBody> = self
            .present()
            .into_iter()
            .map(|ri| SectorBody {
                anchor: self.state.regiments[ri].anchor,
                reach_mm: self.reach_mm(ri, self.fit_of(ri)),
                class: self.state.regiments[ri].class,
            })
            .collect();
        let Battle { state, fid, .. } = self;
        let sector_mm = i64::from(fid.table.globals.sector_mm);
        let width = i64::from(fid.sectors.width());
        let smoke = &state.smoke;
        fid.sectors.update(&bodies, |i| {
            let i = i as i64;
            sector_smoke(smoke, sector_mm, i % width, i / width)
        });
    }
}
