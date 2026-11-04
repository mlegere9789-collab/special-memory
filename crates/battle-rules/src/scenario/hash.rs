//! `scenario_hash`: FNV-1a 64 over a canonical little-endian encoding (SPEC-008 section 3).

use super::{Arm, Scenario, Side, TriggerKind, TriggerOrder};
use sim_state::StateHasher;

/// Magic that opens the encoding: "SCN1".
const MAGIC: u32 = 0x5343_4e31;

fn count(h: &mut StateHasher, n: usize) {
    h.write_u32(u32::try_from(n).unwrap_or(u32::MAX));
}

fn text(h: &mut StateHasher, s: &str) {
    count(h, s.len());
    h.write_bytes(s.as_bytes());
}

fn side(h: &mut StateHasher, s: Side) {
    h.write_u8(s.team());
}

fn arm_tag(a: Arm) -> u8 {
    match a {
        Arm::Infantry => 0,
        Arm::Cavalry => 1,
        Arm::Battery => 2,
        Arm::Hq => 3,
    }
}

fn trigger(h: &mut StateHasher, kind: &TriggerKind) {
    match kind {
        TriggerKind::Order(o) => {
            h.write_u8(0);
            match *o {
                TriggerOrder::Hold => h.write_u8(0),
                TriggerOrder::Move { x_mm, y_mm, gait } => {
                    h.write_u8(1);
                    h.write_i64(x_mm);
                    h.write_i64(y_mm);
                    h.write_u8(u8::try_from(gait.index()).unwrap_or(u8::MAX));
                }
                TriggerOrder::Form(f) => {
                    h.write_u8(2);
                    h.write_u8(f.tag());
                }
                TriggerOrder::Face(deg) => {
                    h.write_u8(3);
                    h.write_u16(deg);
                }
            }
        }
        TriggerKind::FireMode { target } => {
            h.write_u8(1);
            text(h, target);
        }
        TriggerKind::Resupply { rounds } => {
            h.write_u8(2);
            h.write_u32(*rounds);
        }
        TriggerKind::Marker => h.write_u8(3),
    }
}

/// The hash of a parsed scenario: the sorted value, so whitespace and row order of the file do
/// not matter. Title, date and provenance are prose and are left out; everything else is in.
///
/// Order: magic, id, family, clock start, duration, tick rate, class, caps, map, terrain
/// (cell, legend, rows hash), then units, arrivals, departures, strength checks, objectives,
/// triggers and weather, each as a `u32` count and its rows ascending.
pub fn scenario_hash(s: &Scenario) -> u64 {
    let mut h = StateHasher::new();
    h.write_u32(MAGIC);
    text(&mut h, &s.id);
    text(&mut h, &s.family);
    h.write_u32(s.clock_start_s);
    h.write_u32(s.duration_s);
    h.write_u32(s.tick_hz);
    h.write_u8(s.fidelity_class);
    h.write_u32(s.f0_cap);
    h.write_u32(s.agent_cap);
    for v in [
        s.map.width_mm,
        s.map.height_mm,
        s.map.origin_x_mm,
        s.map.origin_y_mm,
        s.terrain.cell_mm,
    ] {
        h.write_i64(v);
    }
    count(&mut h, s.terrain.legend.len());
    for (ch, class) in &s.terrain.legend {
        h.write_u32(u32::from(*ch));
        h.write_u8(*class);
    }
    h.write_u64(s.terrain.rows_hash);
    count(&mut h, s.units.len());
    for u in &s.units {
        text(&mut h, &u.id);
        side(&mut h, u.side);
        text(&mut h, &u.parent);
        h.write_u8(arm_tag(u.arm));
        h.write_u32(u.men);
        h.write_u32(u.guns);
        h.write_u8(u.training.tag());
        text(&mut h, &u.weapon);
        h.write_u32(u.rounds_per_man);
        text(&mut h, &u.commander);
    }
    count(&mut h, s.arrivals.len());
    for a in &s.arrivals {
        text(&mut h, &a.unit);
        h.write_u32(a.at_s);
        h.write_i64(a.x_mm);
        h.write_i64(a.y_mm);
        h.write_u16(a.heading_deg);
        h.write_u8(a.formation.tag());
        h.write_u32(a.low_s);
        h.write_u32(a.high_s);
    }
    count(&mut h, s.departures.len());
    for d in &s.departures {
        text(&mut h, &d.unit);
        h.write_u32(d.at_s);
    }
    count(&mut h, s.strength_checks.len());
    for c in &s.strength_checks {
        h.write_u32(c.at_s);
        side(&mut h, c.side);
        h.write_u32(c.men);
        h.write_u32(c.tol_men);
    }
    count(&mut h, s.objectives.len());
    for o in &s.objectives {
        text(&mut h, &o.id);
        side(&mut h, o.side);
        h.write_i64(o.x_mm);
        h.write_i64(o.y_mm);
        h.write_u32(o.radius_mm);
        h.write_u32(o.points);
    }
    count(&mut h, s.triggers.len());
    for t in &s.triggers {
        text(&mut h, &t.id);
        h.write_u32(t.at_s);
        text(&mut h, &t.unit);
        trigger(&mut h, &t.kind);
    }
    count(&mut h, s.weather.len());
    for w in &s.weather {
        h.write_u32(w.from_s);
        h.write_i64(w.wind_x_mm_s);
        h.write_i64(w.wind_y_mm_s);
        h.write_u32(w.wet_pm);
        h.write_u32(w.ground_pm);
        h.write_u32(w.visibility_m);
    }
    h.finish()
}
