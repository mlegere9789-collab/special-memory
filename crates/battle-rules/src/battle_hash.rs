//! The battle state hash (SPEC-004 section 11).

use crate::orders::gait_tag;
use crate::{BattleState, Order, RegimentState};
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
/// slot ascending, then three reserved `u32 = 0` blocks (fire, smoke, casualty log) that SPEC-005
/// fills. FNV-1a 64 over little-endian fields.
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
    for _reserved in 0..3 {
        h.write_u32(0);
    }
    h.finish()
}
