//! Reading an order (SPEC-007 section 5): garbling by medium and kind, and the stale rule.

use super::leg::hit;
use super::state::{GarbleKind, OrderBody};
use super::{CommandTable, Medium, Officer, OfficerId};
use crate::{fx_ratio, gauss_unit, Formation, Gait, Order};
use core_math::{Angle, Rng, Vec2Fx};

/// Reads `sent` the way its addressee does. Draws in fixed order whatever the outcome: `r1`
/// (garble roll), `r2` (kind roll), two Gaussian values. Garbled iff `hit(r1, garble_ppm)`; a
/// hold is never garbled. Move: `r2 % 1000` below `garble_move_displace_pm` displaces the
/// target, below `garble_move_gait_pm` shifts the gait by one step (sign of bit 31 of `r2`,
/// kept within walk to double-quick), else the order becomes a hold. Form: below
/// `garble_form_swap_pm` another formation (`+1` or `+2` modulo three by the sign), else a
/// hold. Face: below `garble_face_turn_pm` a turn of `garble_angle_deg` degrees times the first
/// Gaussian value, else a hold.
pub fn read_order(
    table: &CommandTable,
    medium: Medium,
    rng: &mut Rng,
    sent: OrderBody,
) -> (OrderBody, Option<GarbleKind>) {
    let (r1, r2) = (rng.next_u32(), rng.next_u32());
    let (g1, g2) = (gauss_unit(rng), gauss_unit(rng));
    let g = &table.globals;
    let row = &table.media[usize::from(medium.tag()) - 1];
    let roll = r2 % 1000;
    let negative = r2 >> 31 == 1;
    let with = |order, kind| (OrderBody { order, ..sent }, Some(kind));
    let stalled = || with(Order::Hold, GarbleKind::Stalled);
    if !hit(r1, row.garble_ppm) {
        return (sent, None);
    }
    match sent.order {
        Order::Hold => (sent, None),
        Order::Move { to, gait } if roll < g.garble_move_displace_pm => {
            let sigma = fx_ratio(g.garble_sigma_mm, 1000);
            let to = Vec2Fx::new(to.x + sigma * g1, to.y + sigma * g2);
            with(Order::Move { to, gait }, GarbleKind::Displaced)
        }
        Order::Move { to, gait } if roll < g.garble_move_gait_pm => {
            let i = gait.index() as i32 + if negative { -1 } else { 1 };
            let gait = Gait::ALL[i.clamp(0, Gait::DoubleQuick.index() as i32) as usize];
            with(Order::Move { to, gait }, GarbleKind::GaitShifted)
        }
        Order::Form(f) if roll < g.garble_form_swap_pm => {
            let step = if negative { 2 } else { 1 };
            let swapped = Formation::ALL[(usize::from(f.tag()) + step) % 3];
            with(Order::Form(swapped), GarbleKind::FormSwapped)
        }
        Order::Face(a) if roll < g.garble_face_turn_pm => {
            let full = i128::from(Angle::from_degrees_int(g.garble_angle_deg as i32).raw());
            let turn = ((full * i128::from(g1.raw())) >> 32) as u32;
            with(
                Order::Face(Angle::from_raw(a.raw().wrapping_add(turn))),
                GarbleKind::Turned,
            )
        }
        _ => stalled(),
    }
}

/// The stale rule: records `seq` as read from `sender` and returns true when it is not above the
/// highest read so far (the order is out of date and discarded).
pub(crate) fn note_seq(reader: &mut Officer, sender: OfficerId, seq: u32) -> bool {
    match reader.last_seq_from.binary_search_by_key(&sender, |e| e.0) {
        Ok(i) if seq <= reader.last_seq_from[i].1 => true,
        Ok(i) => {
            reader.last_seq_from[i].1 = seq;
            false
        }
        Err(i) => {
            reader.last_seq_from.insert(i, (sender, seq));
            false
        }
    }
}
