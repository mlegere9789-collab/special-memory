//! `snapshot_hash` (SPEC-015 section 12): FNV-1a 64 over the canonical little-endian encoding.
//!
//! Encoding, in order: `tick u64, clock_s u32, side u8, full_view u8`; then a `u32` count and per
//! regiment `id u16, side u8, pos.x i64, pos.y i64, heading u32, formation u8, strength u32,
//! order u8, level u8, margin i32, trend i8, factor count u32` and per factor `id u8,
//! contribution i32`; then a `u32` count and per enemy `unit u16, last_seen u64, pos.x i64,
//! pos.y i64, err u32, lo u32, hi u32, formation_known u8`; then a `u32` count and per order
//! `id u32`, the `encode_order` bytes (regiment and order), `sent_tick`, then `eta_lo u64, eta_hi
//! u64, eta_hi0 u64`, `ack_tick`, the interpreted order (`0` or `1` and its `encode_order`
//! bytes), `executed_tick`, `completed_tick`, `lost_known u8`. An optional tick is `0`, or `1`
//! and a `u64`.

use crate::snapshot::Snapshot;
use battle_rules::encode_order;
use sim_state::StateHasher;

fn opt_u64(h: &mut StateHasher, v: Option<u64>) {
    match v {
        None => h.write_u8(0),
        Some(t) => {
            h.write_u8(1);
            h.write_u64(t);
        }
    }
}

fn count(h: &mut StateHasher, n: usize) {
    h.write_u32(u32::try_from(n).unwrap_or(u32::MAX));
}

/// The hash of everything the snapshot shows.
pub fn snapshot_hash(s: &Snapshot) -> u64 {
    let mut h = StateHasher::new();
    h.write_u64(s.tick);
    h.write_u32(s.clock_s);
    h.write_u8(s.side);
    h.write_u8(u8::from(s.full_view));
    count(&mut h, s.regiments.len());
    for r in &s.regiments {
        h.write_u16(r.id);
        h.write_u8(r.side);
        h.write_i64(r.pos.x.raw());
        h.write_i64(r.pos.y.raw());
        h.write_u32(r.heading.raw());
        h.write_u8(r.formation);
        h.write_u32(r.strength);
        h.write_u8(r.order);
        h.write_u8(r.morale.level);
        h.write_u32(r.morale.margin as u32);
        h.write_u8(r.morale.trend as u8);
        count(&mut h, r.morale.factors.len());
        for &(id, c) in &r.morale.factors {
            h.write_u8(id);
            h.write_u32(c as u32);
        }
    }
    count(&mut h, s.enemies.len());
    for e in &s.enemies {
        h.write_u16(e.unit);
        h.write_u64(e.last_seen_tick);
        h.write_i64(e.pos.x.raw());
        h.write_i64(e.pos.y.raw());
        h.write_u32(e.pos_err_mm);
        h.write_u32(e.strength_lo);
        h.write_u32(e.strength_hi);
        h.write_u8(u8::from(e.formation_known));
    }
    count(&mut h, s.orders.len());
    for o in &s.orders {
        h.write_u32(o.id);
        h.write_bytes(&encode_order(o.regiment, &o.order));
        opt_u64(&mut h, o.sent_tick);
        h.write_u64(o.eta_lo);
        h.write_u64(o.eta_hi);
        h.write_u64(o.eta_hi0);
        opt_u64(&mut h, o.ack_tick);
        match &o.interpreted {
            None => h.write_u8(0),
            Some(i) => {
                h.write_u8(1);
                h.write_bytes(&encode_order(o.regiment, i));
            }
        }
        opt_u64(&mut h, o.executed_tick);
        opt_u64(&mut h, o.completed_tick);
        h.write_u8(u8::from(o.lost_known));
    }
    h.finish()
}
