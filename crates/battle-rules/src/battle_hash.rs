//! The battle state hash (SPEC-004 section 11).

use crate::command::write_command_block;
use crate::orders::gait_tag;
use crate::{BattleState, FireMode, Order, RegimentState};
use sim_state::StateHasher;

/// The fields of an order as hashed: `(tag, x, y, gait)`. Move writes its target and gait; Form
/// writes the formation tag in `x`; Face writes the raw angle in `x`; Hold writes zeros.
fn order_fields(order: &Order) -> (u8, i64, i64, u8) {
    match order {
        Order::Hold => (order.tag(), 0, 0, 0),
        Order::Move { to, gait } => (order.tag(), to.x.raw(), to.y.raw(), gait_tag(*gait)),
        Order::Form(f) => (order.tag(), i64::from(f.tag()), 0, 0),
        Order::Face(a) => (order.tag(), i64::from(a.raw()), 0, 0),
    }
}

/// The hash of a battle: the world, then every regiment ascending by id, then every locomotion
/// slot ascending, then the fire block, the smoke block and the casualty block of SPEC-005, then
/// the morale block when `morale` is `Some` (SPEC-006), the command block when `cmd` is `Some` (SPEC-007), then the environment block when `env` is
/// `Some` (SPEC-009). FNV-1a 64 over little-endian fields.
///
/// Per regiment: `id u16, team u8, training u8, state tag u8 (0 steady, 1 forming), from u8,
/// to u8, ticks_left u32, anchor x i64, anchor y i64, heading u32, order tag u8, order x i64,
/// order y i64, gait u8, dirty u8, roster_len u32`, then the roster's agent slot indices as
/// `u32`. A steady regiment writes its formation as both `from` and `to` and zero `ticks_left`.
/// The pending order is not part of the hash.
/// Per locomotion slot: `fatigue u32, slot.u i64, slot.v i64, catchup u8`, after a `u32` count.
pub fn battle_hash(state: &BattleState) -> u64 {
    let mut h = StateHasher::new();
    h.write_world(&state.world);
    h.write_u32(u32::try_from(state.regiments.len()).unwrap_or(u32::MAX));
    for r in &state.regiments {
        h.write_u16(r.id.0);
        h.write_u8(r.team);
        h.write_u8(r.training.tag());
        let (tag, from, to, ticks_left) = match r.state {
            RegimentState::Steady(f) => (0u8, f.tag(), f.tag(), 0u32),
            RegimentState::Forming {
                from,
                to,
                ticks_left,
            } => (1u8, from.tag(), to.tag(), ticks_left),
        };
        h.write_u8(tag);
        h.write_u8(from);
        h.write_u8(to);
        h.write_u32(ticks_left);
        h.write_i64(r.anchor.x.raw());
        h.write_i64(r.anchor.y.raw());
        h.write_u32(r.heading.raw());
        let (order_tag, x, y, gait) = order_fields(&r.order);
        h.write_u8(order_tag);
        h.write_i64(x);
        h.write_i64(y);
        h.write_u8(gait);
        h.write_u8(u8::from(r.dirty));
        h.write_u32(u32::try_from(r.roster.len()).unwrap_or(u32::MAX));
        for id in &r.roster {
            h.write_u32(id.index);
        }
    }
    h.write_u32(u32::try_from(state.loco.len()).unwrap_or(u32::MAX));
    for l in &state.loco {
        h.write_u32(l.fatigue);
        h.write_i64(l.slot.u.raw());
        h.write_i64(l.slot.v.raw());
        h.write_u8(u8::from(l.catchup));
    }
    write_fire(&mut h, state);
    write_smoke(&mut h, state);
    write_casualties(&mut h, state);
    // Optional blocks follow in REGISTRY order: morale, command, environment.
    write_morale(&mut h, state);
    if let Some(cmd) = &state.cmd {
        write_command_block(&mut h, cmd);
    }
    write_environment(&mut h, state);
    h.finish()
}

/// The fire block: `u32` count of fire slots; with none (nothing equipped) that is the whole
/// block. Otherwise every `FireExt` ascending (`rounds u16, ammo_sel u8, step u8, left u16,
/// fouling u16, mag u8, disabled_until u64, rng 4 x u64`), then `seq u64`, the pool (`u32`
/// count, then per projectile `seq u64, owner u32, unit u32, team u8, ammo u8, pos x i64,
/// pos y i64, z i64, vel x i64, vel y i64, vz i64, travelled_mm u32, bounces u8, hits u8,
/// fuse u32, seed u64`), then per regiment `weapon u8 (255 none), ammo row u8, mode u8, target
/// u16`.
fn write_fire(h: &mut StateHasher, state: &BattleState) {
    h.write_u32(u32::try_from(state.fire.len()).unwrap_or(u32::MAX));
    if state.fire.is_empty() {
        return;
    }
    for f in &state.fire {
        h.write_u16(f.rounds);
        h.write_u8(f.ammo_sel);
        h.write_u8(f.step);
        h.write_u16(f.left);
        h.write_u16(f.fouling);
        h.write_u8(f.mag);
        h.write_u64(f.disabled_until);
        for w in f.rng {
            h.write_u64(w);
        }
    }
    h.write_u64(state.seq);
    h.write_u32(u32::try_from(state.pool.len()).unwrap_or(u32::MAX));
    for p in &state.pool {
        h.write_u64(p.seq);
        h.write_u32(p.owner);
        h.write_u32(p.unit);
        h.write_u8(p.team);
        h.write_u8(p.ammo);
        for v in [p.pos.x, p.pos.y, p.z, p.vel.x, p.vel.y, p.vz] {
            h.write_i64(v.raw());
        }
        h.write_u32(p.travelled_mm);
        h.write_u8(p.bounces);
        h.write_u8(p.hits);
        h.write_u32(p.fuse as u32);
        h.write_u64(p.seed);
    }
    for r in &state.regfire {
        h.write_u8(r.weapon.unwrap_or(u8::MAX));
        h.write_u8(r.ammo_row);
        let (mode, target) = match r.mode {
            FireMode::HoldFire => (0u8, 0u16),
            FireMode::AtWill { target } => (1, target.0),
        };
        h.write_u8(mode);
        h.write_u16(target);
    }
}

/// The smoke block: `u32` count of cells with smoke, then `(cell index u32, density u32)`
/// ascending. A clear grid is a single zero.
fn write_smoke(h: &mut StateHasher, state: &BattleState) {
    let lit = state.smoke.density.iter().filter(|&&d| d != 0).count();
    h.write_u32(u32::try_from(lit).unwrap_or(u32::MAX));
    for (i, &d) in state.smoke.density.iter().enumerate() {
        if d != 0 {
            h.write_u32(i as u32);
            h.write_u32(d);
        }
    }
}

/// The casualty block: `u32` count, then per record `tick u64, target_index u32, unit u32,
/// team u8, kind u8 (0 killed, 1 wounded, 2 disabled), shooter_unit u32, ammo u8`. An empty log
/// is a single zero.
fn write_casualties(h: &mut StateHasher, state: &BattleState) {
    h.write_u32(u32::try_from(state.casualties.len()).unwrap_or(u32::MAX));
    for c in &state.casualties {
        h.write_u64(c.tick);
        h.write_u32(c.target_index);
        h.write_u32(c.unit);
        h.write_u8(c.team);
        h.write_u8(c.kind as u8);
        h.write_u32(c.shooter_unit);
        h.write_u8(c.ammo);
    }
}

/// Marks the morale block: "MOR1".
const MORALE_MAGIC: u32 = 0x4D_4F_52_31;

/// The morale block (SPEC-006 section 6), written only when `morale` is `Some`: `u32
/// 0x4D_4F_52_31`, the regiment count, then per regiment `kind u8, state_ticks u32, c u32,
/// loss u32, window u32, nerve_offset i32, broken_count u8, s0 u16, rounds_full u32, supply_pm
/// u16, leader_pm u16, colonel u32`, the officer count and indices, the absent count and indices,
/// `rng` as 4 x `u64`, `indicator u8, dwell u16`; the leader count and per leader `id u16, team
/// u8, alive u8, x i64, y i64, prestige u16`; `cursor u64`; the event count and per event `kind
/// u8, regiment u16, x i64, y i64`; then per team `present u8, x i64, y i64` (zeros when absent).
fn write_morale(h: &mut StateHasher, state: &BattleState) {
    let Some(ms) = &state.morale else {
        return;
    };
    let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    h.write_u32(MORALE_MAGIC);
    h.write_u32(count(ms.regs.len()));
    for r in &ms.regs {
        h.write_u8(r.kind.tag());
        h.write_u32(r.state_ticks);
        h.write_u32(r.c);
        h.write_u32(r.loss);
        h.write_u32(r.window);
        h.write_u32(r.nerve_offset as u32);
        h.write_u8(r.broken_count);
        h.write_u16(r.s0);
        h.write_u32(r.rounds_full);
        h.write_u16(r.supply_pm);
        h.write_u16(r.leader_pm);
        h.write_u32(r.colonel.index);
        h.write_u32(count(r.officers.len()));
        for a in &r.officers {
            h.write_u32(a.index);
        }
        h.write_u32(count(r.absent.len()));
        for a in &r.absent {
            h.write_u32(a.index);
        }
        for w in r.rng.state() {
            h.write_u64(w);
        }
        h.write_u8(r.indicator.tag());
        h.write_u16(r.dwell);
    }
    h.write_u32(count(ms.leaders.len()));
    for l in &ms.leaders {
        h.write_u16(l.id);
        h.write_u8(l.team);
        h.write_u8(u8::from(l.alive));
        h.write_i64(l.pos.x.raw());
        h.write_i64(l.pos.y.raw());
        h.write_u16(l.prestige_pm);
    }
    h.write_u64(ms.cursor as u64);
    h.write_u32(count(ms.events.len()));
    for e in &ms.events {
        h.write_u8(e.kind as u8);
        h.write_u16(e.regiment.0);
        h.write_i64(e.pos.x.raw());
        h.write_i64(e.pos.y.raw());
    }
    for p in &ms.rally_points {
        h.write_u8(u8::from(p.is_some()));
        let (x, y) = p.map_or((0, 0), |p| (p.x.raw(), p.y.raw()));
        h.write_i64(x);
        h.write_i64(y);
    }
}

/// Marks the environment block: "ENV1".
const ENV_MAGIC: u32 = 0x45_4E_56_31;

/// The environment block (SPEC-009 section 9), written only when `env` is `Some`: `u32
/// 0x45_4E_56_31`, then `WeatherNow` (`temp i32, humidity i32, wind x i64, wind y i64, rain i32,
/// clear_vis i32, light i32`), powder damp `u32`, ground wetness `u32`, and the smoke losses
/// `lost_edge u64, lost_decay u64, lost_cap u64`. Later tickets append the sight fields.
fn write_environment(h: &mut StateHasher, state: &BattleState) {
    let Some(env) = &state.env else {
        return;
    };
    h.write_u32(ENV_MAGIC);
    let n = env.now();
    h.write_u32(n.temp_f10 as u32);
    h.write_u32(n.humidity_pm as u32);
    h.write_i64(n.wind_x_mm_s);
    h.write_i64(n.wind_y_mm_s);
    h.write_u32(n.rain_pm as u32);
    h.write_u32(n.clear_vis_mm as u32);
    h.write_u32(n.light_pm as u32);
    h.write_u32(env.powder_damp_ppm());
    h.write_u32(env.ground_wet_ppm());
    h.write_u64(state.smoke.lost_edge);
    h.write_u64(state.smoke.lost_decay);
    h.write_u64(state.smoke.lost_cap);
}
