//! Interpretation of a discretionary move (SPEC-007 section 6): the officer scores whether to go
//! from the odds his own picture gives him and the tiredness of his men, then pushes the target
//! along its line by his traits' reach and shifts the gait, or declines and holds.
//!
//! [`go_score`] and [`interpret`] are pure; [`BattleState::interpret_for`] gathers their inputs
//! from the officer's formation and his picture, never from the world's enemy.

use super::picture::confidence_pm;
use super::state::ModKind;
use super::{CommandTable, Latitude, OfficerId, OrderBody, TraitMask};
use crate::{BattleState, Gait, Order};
use core_math::{Fx, Vec2Fx};

use super::leg::dist_mm;

/// What an officer weighs when he reads a move.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InterpretInput {
    /// The order as read.
    pub body: OrderBody,
    /// Centroid of his formation, where the line of the move starts.
    pub centroid: Vec2Fx,
    /// His share of the strength in the contest at the target, permille:
    /// `1000 * own / max(1, own + threat)`.
    pub own_pm: u32,
    /// Mean fatigue of his men, ppm.
    pub fatigue_ppm: u32,
}

/// The order an officer executes and how it differs from what he read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Interpreted {
    /// What he will do.
    pub body: OrderBody,
    /// `None`, `Extended`, `StoppedShort` or `Declined`.
    pub kind: ModKind,
}

/// The go score of a discretionary move, permille: `base_go` plus the sum of the traits'
/// `go_bias`, plus `(own_pm - 500) * odds_weight / 1000`, minus
/// `fatigue * fatigue_weight / 10^6`, plus `judge_sigma * gauss`. Each term is integer
/// arithmetic: division truncates toward zero, the product with the Gaussian value shifts
/// right by 32.
pub fn go_score(
    table: &CommandTable,
    mask: TraitMask,
    own_pm: u32,
    fatigue_ppm: u32,
    gauss: Fx,
) -> i64 {
    let g = &table.globals;
    let bias: i64 = table
        .traits
        .iter()
        .filter(|r| mask.contains(r.mask))
        .map(|r| i64::from(r.go_bias_pm))
        .sum();
    let odds = (i64::from(own_pm) - 500) * i64::from(g.odds_weight_pm) / 1000;
    let tired = i64::from(fatigue_ppm) * i64::from(g.fatigue_weight_pm) / 1_000_000;
    let judge = ((i128::from(g.judge_sigma_pm) * i128::from(gauss.raw())) >> 32) as i64;
    i64::from(g.base_go_pm) + bias + odds - tired + judge
}

/// Interprets an order. Only a discretionary move is changed: below `go_threshold_pm` the
/// officer declines (a hold, `Declined`); otherwise the target moves along the line from the
/// centroid by the sum of the traits' `reach_mm`, never behind the centroid (`Extended` for a
/// positive reach, `StoppedShort` for a negative one), and the gait shifts by the sum of
/// `gait_shift` within walk to double-quick (a gait already faster is kept). Anything else is
/// executed as received.
pub fn interpret(
    table: &CommandTable,
    mask: TraitMask,
    input: &InterpretInput,
    gauss: Fx,
) -> Interpreted {
    let as_read = Interpreted {
        body: input.body,
        kind: ModKind::None,
    };
    let (Order::Move { to, gait }, Latitude::Discretionary) =
        (input.body.order, input.body.latitude)
    else {
        return as_read;
    };
    let score = go_score(table, mask, input.own_pm, input.fatigue_ppm, gauss);
    if score < i64::from(table.globals.go_threshold_pm) {
        return Interpreted {
            body: OrderBody {
                order: Order::Hold,
                ..input.body
            },
            kind: ModKind::Declined,
        };
    }
    let (mut reach, mut shift) = (0i64, 0i32);
    for r in table.traits.iter().filter(|r| mask.contains(r.mask)) {
        reach += r.reach_mm;
        shift += r.gait_shift;
    }
    let mut kind = ModKind::None;
    let mut target = to;
    let line = to - input.centroid;
    let len = i128::from(line.length().raw());
    if reach != 0 && len > 0 {
        let new_len = (len + ((i128::from(reach) << 32) / 1000)).max(0);
        let scale = |v: Fx| Fx::from_raw((i128::from(v.raw()) * new_len / len) as i64);
        target = input.centroid + Vec2Fx::new(scale(line.x), scale(line.y));
        kind = if reach > 0 {
            ModKind::Extended
        } else {
            ModKind::StoppedShort
        };
    }
    let idx = gait.index() as i32;
    let gait = if shift == 0 {
        gait
    } else {
        let hi = idx.max(Gait::DoubleQuick.index() as i32);
        Gait::ALL[(idx + shift).clamp(0, hi) as usize]
    };
    Interpreted {
        body: OrderBody {
            order: Order::Move { to: target, gait },
            ..input.body
        },
        kind,
    }
}

impl BattleState {
    /// Interprets `body` for officer `id` from his own formation and picture: `own` is the men in
    /// his formation, `threat` the sum of `strength * confidence / 1000` of the contacts in his
    /// picture within `practicable_radius_mm` of the target, and the fatigue is the mean of his
    /// men. An officer or order that gives no inputs is executed as received.
    pub(crate) fn interpret_for(
        &self,
        table: &CommandTable,
        id: OfficerId,
        body: OrderBody,
        gauss: Fx,
    ) -> Interpreted {
        let as_read = Interpreted {
            body,
            kind: ModKind::None,
        };
        let (Some(cmd), Order::Move { to, .. }) = (self.cmd.as_ref(), body.order) else {
            return as_read;
        };
        let (Some(i), Some(centroid)) = (cmd.index_of(id), self.formation_centroid(id)) else {
            return as_read;
        };
        let now = self.world.tick;
        let own = u64::from(cmd.formation_strength(self, id));
        let threat: u64 = cmd.pics[i]
            .contacts
            .iter()
            .filter(|c| dist_mm(c.pos, to) <= table.globals.practicable_radius_mm)
            .map(|c| (c, confidence_pm(c, now, table)))
            .filter(|(_, conf)| *conf >= table.globals.drop_pm)
            .map(|(c, conf)| u64::from(c.strength) * u64::from(conf) / 1000)
            .sum();
        let (mut tired, mut men) = (0u64, 0u64);
        for r in cmd.formation_regiments(id) {
            for a in &self.regiments[r].roster {
                tired += u64::from(self.loco[a.index as usize].fatigue);
                men += 1;
            }
        }
        let input = InterpretInput {
            body,
            centroid,
            own_pm: (1000 * own / (own + threat).max(1)) as u32,
            fatigue_ppm: (tired / men.max(1)) as u32,
        };
        interpret(table, cmd.officers[i].mask, &input, gauss)
    }
}
