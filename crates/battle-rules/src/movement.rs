//! Steering, avoidance and anchor movement (SPEC-004 sections 7 and 9, with Amendment A2).
//!
//! [`BattleState::step_movement`] runs one tick: rebuild the spatial grid, advance the drill
//! state machine, move the anchors, then phase A (every soldier's desired velocity from a frozen
//! snapshot, see [`BattleState::desired_velocity`]) and phase B (acceleration limit, terrain,
//! fatigue and snapping, ascending id), and finally advance the tick counter.

use crate::{
    fatigue_speed_pm, fatigue_step_scaled, fx_ratio, BattleState, Formation, Gait, LocoExt, Order,
    Regiment, RegimentState, F_DISABLED, F_WOUNDED, TICK_HZ,
};
use core_math::{Angle, Fx, Vec2Fx};
use sim_state::AgentId;

/// Constants derived once per step from the locomotion globals.
struct Consts {
    personal_space: Fx,
    avoid_gain: Fx,
    accel_tick: Fx,
    catchup: Fx,
    max_lag_sq: Fx,
    catchup_fatigue: u32,
    tick: Fx,
}

impl Consts {
    fn new(t: &crate::LocomotionTable) -> Consts {
        let g = t.globals();
        let lag = fx_ratio(i64::from(g.max_lag_mm), 1000);
        Consts {
            personal_space: fx_ratio(i64::from(g.personal_space_mm), 1000),
            avoid_gain: fx_ratio(i64::from(g.avoid_gain_mm_s), 1000),
            accel_tick: fx_ratio(i64::from(g.accel_mm_s2), 1000 * i64::from(TICK_HZ)),
            catchup: fx_ratio(i64::from(g.catchup_mm), 1000),
            max_lag_sq: lag * lag,
            catchup_fatigue: g.catchup_fatigue_limit_ppm,
            tick: Fx::from_int(TICK_HZ as i32),
        }
    }
}

/// What phase A decides for one soldier.
#[derive(Clone, Copy)]
struct Plan {
    desired: Vec2Fx,
    gait: Gait,
    class: u8,
    catchup: bool,
    push: bool,
}

const IDLE: Plan = Plan {
    desired: Vec2Fx::ZERO,
    gait: Gait::Walk,
    class: 0,
    catchup: false,
    push: false,
};

/// The unit vectors `(forward, right)` of a regiment's frame.
fn frame(reg: &Regiment) -> (Vec2Fx, Vec2Fx) {
    (
        Vec2Fx::from_angle(reg.heading),
        Vec2Fx::from_angle(reg.heading - Angle::QUARTER),
    )
}

fn slot_world(reg: &Regiment, f: (Vec2Fx, Vec2Fx), slot: crate::SlotLocal) -> Vec2Fx {
    reg.anchor + f.1.scale(slot.u) + f.0.scale(slot.v)
}

/// The gait a regiment's soldiers use: double-quick while forming, the ordered gait on the
/// march, quick otherwise.
fn base_gait(reg: &Regiment) -> Gait {
    match (reg.state, reg.order) {
        (RegimentState::Forming { .. }, _) => Gait::DoubleQuick,
        (_, Order::Move { gait, .. }) => gait,
        _ => Gait::Quick,
    }
}

/// `v` limited to length `max`, in integer arithmetic (no component exceeds the exact result).
fn clamp_len(v: Vec2Fx, max: Fx) -> Vec2Fx {
    if v.length_sq() <= max * max {
        return v;
    }
    let len = v.length();
    if max <= Fx::ZERO || len == Fx::ZERO {
        return Vec2Fx::ZERO;
    }
    let part = |c: Fx| {
        let q = i128::from(c.raw()) * i128::from(max.raw()) / i128::from(len.raw());
        Fx::from_raw(i64::try_from(q).unwrap_or(0))
    };
    Vec2Fx::new(part(v.x), part(v.y))
}

/// `a` turned toward `target` by at most `step`.
fn turn_toward(a: Angle, target: Angle, step: Angle) -> Angle {
    let diff = i64::from(target.raw().wrapping_sub(a.raw()) as i32);
    if diff.abs() <= i64::from(step.raw()) {
        target
    } else if diff > 0 {
        Angle::from_raw(a.raw().wrapping_add(step.raw()))
    } else {
        Angle::from_raw(a.raw().wrapping_sub(step.raw()))
    }
}

/// Speed in m/s: `speed_mm_s * terrain_pm * fatigue_pm * extra_pm / 10^12`.
fn cap_fx(speed_mm_s: u32, terrain_pm: u32, fatigue_pm: u32, extra_pm: u32) -> Fx {
    let num =
        i64::from(speed_mm_s) * i64::from(terrain_pm) * i64::from(fatigue_pm) * i64::from(extra_pm);
    fx_ratio(num, 1_000_000_000_000)
}

impl BattleState {
    fn terrain_row(&self, class: u8) -> (u32, u32, u32) {
        self.tables
            .terrain
            .class(class)
            .map_or((1000, 1000, 0), |c| {
                (c.speed_pm, c.effort_pm, c.line_disorder_pm)
            })
    }

    fn soldier_fit(&self, id: AgentId) -> bool {
        self.world
            .agents
            .flags(id)
            .is_some_and(|f| f & (F_DISABLED | F_WOUNDED) == 0)
    }

    /// Phase A for one soldier against the current agent store and spatial grid.
    fn plan(&self, id: AgentId, fr: (Vec2Fx, Vec2Fx), c: &Consts, hits: &mut Vec<AgentId>) -> Plan {
        let agents = &self.world.agents;
        let Some(a) = agents.get(id) else { return IDLE };
        let Some(reg) = self.regiments.get(a.unit as usize) else {
            return IDLE;
        };
        let loco = self
            .loco
            .get(id.index as usize)
            .copied()
            .unwrap_or_default();
        let class = self.terrain.class_at(a.pos);
        let to = slot_world(reg, fr, loco.slot) - a.pos;
        let dir = to.normalize();
        let dist = to.dot(dir);

        let lt = &self.tables.locomotion;
        let catchup = dist > c.catchup && loco.fatigue < c.catchup_fatigue;
        let gait = if catchup {
            base_gait(reg).max(Gait::DoubleQuick)
        } else {
            base_gait(reg)
        };
        let cap = if self.soldier_fit(id) {
            let (spm, _, _) = self.terrain_row(class);
            cap_fx(
                lt.gait(gait).speed_mm_s,
                spm,
                fatigue_speed_pm(loco.fatigue, lt),
                1000,
            )
        } else {
            Fx::ZERO
        };

        // Seek: never faster than the cap or the distance covered in one tick.
        let speed = cap.min(dist * c.tick);
        let seek = dir.scale(speed);

        // Avoid: neighbours closer than personal space push apart, nearest first.
        self.grid.query_radius(a.pos, c.personal_space, hits);
        let mut push = Vec2Fx::ZERO;
        for &n in hits.iter() {
            if n.index == id.index {
                continue;
            }
            let Some(np) = agents.pos(n) else { continue };
            let diff = a.pos - np;
            let d = diff.length();
            if d >= c.personal_space {
                continue;
            }
            let unit = if d == Fx::ZERO {
                let raw = id.index.wrapping_mul(0x9E37_79B9);
                Vec2Fx::from_angle(Angle::from_raw(raw))
            } else {
                diff.normalize()
            };
            let w = (c.personal_space - d) * c.avoid_gain / c.personal_space;
            push = push + unit.scale(w);
        }
        let pushed = push != Vec2Fx::ZERO;
        let push = clamp_len(push, Fx::from_raw(cap.raw() / 2));
        Plan {
            desired: clamp_len(seek + push, cap),
            gait,
            class,
            catchup,
            push: pushed,
        }
    }

    /// The velocity soldier `id` wants this tick (SPEC-004 section 9, phase A): a seek toward its
    /// slot plus a push away from neighbours inside personal space, limited to its speed cap.
    /// It reads the agent store and the spatial grid as of the last
    /// [`rebuild`](sim_state::SpatialGrid::rebuild) and changes nothing, so the result does not
    /// depend on the order in which soldiers are asked. Zero for a soldier that belongs to no
    /// regiment, is disabled or wounded, or is not alive.
    pub fn desired_velocity(&self, id: AgentId) -> Vec2Fx {
        let reg = self
            .world
            .agents
            .unit(id)
            .and_then(|u| self.regiments.get(u as usize));
        let Some(reg) = reg else {
            return Vec2Fx::ZERO;
        };
        let c = Consts::new(&self.tables.locomotion);
        self.plan(id, frame(reg), &c, &mut Vec::new()).desired
    }

    /// New `(anchor, heading, order)` of a marching or turning regiment, `None` if nothing changes.
    fn anchor_step(&self, reg: &Regiment, c: &Consts) -> Option<(Vec2Fx, Angle, Order)> {
        let RegimentState::Steady(formation) = reg.state else {
            return None;
        };
        let turn = Angle::from_degrees_int(1);
        match reg.order {
            Order::Face(a) => {
                let h = turn_toward(reg.heading, a, turn);
                Some((reg.anchor, h, if h == a { Order::Hold } else { reg.order }))
            }
            Order::Move { to, gait } => {
                let (lt, fr) = (&self.tables.locomotion, frame(reg));
                let mut speed: Option<Fx> = None;
                for &id in &reg.roster {
                    let a = self.world.agents.get(id)?;
                    let l = self
                        .loco
                        .get(id.index as usize)
                        .copied()
                        .unwrap_or_default();
                    if (slot_world(reg, fr, l.slot) - a.pos).length_sq() > c.max_lag_sq {
                        return None;
                    }
                    let (spm, _, dis) = self.terrain_row(self.terrain.class_at(a.pos));
                    let extra = if formation == Formation::Line {
                        1000 - dis.min(1000)
                    } else {
                        1000
                    };
                    let cap = cap_fx(
                        lt.gait(gait).speed_mm_s,
                        spm,
                        fatigue_speed_pm(l.fatigue, lt),
                        extra,
                    );
                    speed = Some(speed.map_or(cap, |s| s.min(cap)));
                }
                let step = speed? / c.tick;
                let delta = to - reg.anchor;
                if delta == Vec2Fx::ZERO {
                    return Some((reg.anchor, reg.heading, Order::Hold));
                }
                let want = Angle::atan2(delta.y, delta.x);
                let h = turn_toward(reg.heading, want, turn);
                let dist = delta.length();
                if h == want && dist <= step {
                    return Some((to, h, Order::Hold));
                }
                let moved = Vec2Fx::from_angle(h).scale(step.min(dist));
                Some((reg.anchor + moved, h, reg.order))
            }
            _ => None,
        }
    }

    /// One movement tick (SPEC-004 section 11): rebuild the spatial grid; the drill state machine
    /// ([`step_drill`](BattleState::step_drill): rosters, close-up, `Forming` countdown, pending
    /// orders); anchors in ascending regiment id; phase A, the desired velocity of every live
    /// soldier from the frozen snapshot; phase B in ascending id: velocity limited by the
    /// acceleration, position moved through the terrain rule, facing set to the regiment heading,
    /// fatigue updated, and a soldier within one step of its slot, unpushed and nearly at rest,
    /// snapped onto it. The tick counter advances last.
    pub fn step_movement(&mut self) {
        self.move_agents();
        self.world.advance_tick();
    }

    /// Everything [`step_movement`](BattleState::step_movement) does except advancing the tick
    /// counter, which [`step`](BattleState::step) does last.
    pub(crate) fn move_agents(&mut self) {
        self.grid.rebuild(&self.world.agents);
        self.step_drill();
        let c = Consts::new(&self.tables.locomotion);
        for i in 0..self.regiments.len() {
            if let Some((anchor, heading, order)) = self.anchor_step(&self.regiments[i], &c) {
                let reg = &mut self.regiments[i];
                (reg.anchor, reg.heading, reg.order) = (anchor, heading, order);
            }
        }

        // Phase A.
        let ids = self.world.agents.ids_alive();
        let frames: Vec<_> = self.regiments.iter().map(frame).collect();
        let mut plans = vec![IDLE; self.world.agents.slot_count()];
        let mut hits = Vec::new();
        for &id in &ids {
            let fr = self
                .world
                .agents
                .unit(id)
                .and_then(|u| frames.get(u as usize))
                .copied()
                .unwrap_or_default();
            plans[id.index as usize] = self.plan(id, fr, &c, &mut hits);
        }

        // Phase B.
        let heat = self.heat_pm();
        for &id in &ids {
            let p = plans[id.index as usize];
            let Some(a) = self.world.agents.get(id) else {
                continue;
            };
            let reg = self.regiments.get(a.unit as usize);
            let vel = a.vel + clamp_len(p.desired - a.vel, c.accel_tick);
            let disp = Vec2Fx::new(vel.x / c.tick, vel.y / c.tick);
            let moved = self.terrain.apply_move(a.pos, disp);
            let step = moved - a.pos;
            let applied = if step == Vec2Fx::ZERO {
                Fx::ZERO
            } else {
                step.length() * c.tick
            };
            let (mut pos, mut vel) = (moved, vel);
            let mut fatigue = None;
            if let (Some(reg), Some(l)) = (reg, self.loco.get(id.index as usize)) {
                let (_, effort, _) = self.terrain_row(p.class);
                fatigue = Some(fatigue_step_scaled(
                    l.fatigue,
                    p.gait,
                    effort,
                    applied,
                    &self.tables.locomotion,
                    heat.0,
                    heat.1,
                ));
                let slot = slot_world(reg, frames[usize::from(reg.id.0)], l.slot);
                let reach = Fx::from_raw(vel.length_sq().raw() / 400);
                if !p.push
                    && a.vel.length_sq() <= c.accel_tick * c.accel_tick
                    && slot.distance_sq(pos) <= reach
                    && !self.terrain.blocks(slot)
                {
                    (pos, vel) = (slot, Vec2Fx::ZERO);
                }
            }
            let facing = reg.map_or(a.facing, |r| r.heading);
            let agents = &mut self.world.agents;
            agents.set_pos(id, pos);
            agents.set_vel(id, vel);
            agents.set_facing(id, facing);
            if let (Some(f), Some(l)) = (fatigue, self.loco.get_mut(id.index as usize)) {
                *l = LocoExt {
                    fatigue: f,
                    catchup: p.catchup,
                    ..*l
                };
            }
        }
    }
}
