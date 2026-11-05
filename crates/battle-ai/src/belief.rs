//! The headquarters picture the lookahead is built from (SPEC-010 sections 7 and 8): which own
//! units and contacts a forecast keeps, and the tracks of the contacts. This module is pure
//! integer arithmetic on port types; the world itself is built in `lookahead.rs`.
//!
//! The picture is what a headquarters would hold: reports with their ages, contacts, and the
//! public map. It names no enemy regiment and no count the port did not report.

use crate::port::{Contact, ContactId, MapView, OwnReport, UnitRef};
use battle_rules::Tables;
use core_math::{Fx, Vec2Fx};

/// Ticks between an order being decided and the forecast applying it (SPEC-010 section 7).
pub const EST_LATENCY_TICKS: u64 = 100;
/// Regiments farther than this from the objective are left out, millimetres.
pub const BELIEF_RADIUS_MM: i64 = 1_500_000;
/// Most soldiers a belief holds; the farthest regiments are dropped first.
pub const BELIEF_MAX_SOLDIERS: u32 = 3000;

/// The spatial index of the belief world: public map knowledge, not a measure of the enemy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GridSpec {
    /// Cell edge in metres, at least 1.
    pub cell_m: i32,
    /// Cells in x, 1 to 4096.
    pub width: u32,
    /// Cells in y, 1 to 4096.
    pub height: u32,
    /// Corner of the grid, metres.
    pub origin: Vec2Fx,
}

/// The ground an objective covers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Area {
    /// Centre, metres.
    pub center: Vec2Fx,
    /// Radius, millimetres.
    pub radius_mm: i64,
}

/// Everything a belief is built from.
#[derive(Clone, Copy, Debug)]
pub struct BeliefView<'a> {
    /// The AI's side (the team of its own regiments).
    pub side: u8,
    /// Tick the belief world starts at.
    pub now: u64,
    /// Own reports, ascending unit id.
    pub reports: &'a [OwnReport],
    /// Contacts, ascending id.
    pub contacts: &'a [Contact],
    /// The contacts of the previous picture, ascending id; a contact present in both gives a
    /// track from its last two sightings.
    pub previous: &'a [Contact],
    /// Terrain and obstacles.
    pub map: &'a MapView,
    /// Rules tables, read only.
    pub tables: &'a Tables,
    /// Spatial index of the belief world.
    pub grid: GridSpec,
    /// The objective the forecast is about.
    pub area: Area,
    /// Weapon row id of every own regiment.
    pub own_weapon: &'a str,
    /// Weapon row id of a contact whose weapon is not made out (the side's majority weapon).
    pub enemy_weapon: &'a str,
}

/// A contact kept in the belief with its track.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Foe {
    /// The contact as the picture holds it.
    pub contact: Contact,
    /// Velocity from the last two sightings, metres per tick; zero without a track.
    pub vel: Vec2Fx,
}

/// The units a belief keeps, in the order they become regiments: own units ascending by unit
/// id, then contacts ascending by id. Regiment `i` is `own[i]`, regiment `own.len() + j` is
/// `foes[j]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    /// The AI's side.
    pub side: u8,
    /// The objective area.
    pub area: Area,
    /// Own units kept.
    pub own: Vec<OwnReport>,
    /// Contacts kept.
    pub foes: Vec<Foe>,
}

/// Squared distance of two points in raw `Fx` units, exact.
pub(crate) fn dist_sq(a: Vec2Fx, b: Vec2Fx) -> i128 {
    let dx = i128::from(a.x.raw()) - i128::from(b.x.raw());
    let dy = i128::from(a.y.raw()) - i128::from(b.y.raw());
    dx * dx + dy * dy
}

/// A radius in millimetres as raw `Fx` metres.
pub(crate) fn radius_raw(radius_mm: i64) -> i128 {
    (i128::from(radius_mm) << Fx::FRAC_BITS) / 1000
}

/// The track of a contact: displacement between its last two sightings over their interval.
fn track(c: &Contact, previous: &[Contact]) -> Vec2Fx {
    let Some(p) = previous.iter().find(|p| p.id == c.id) else {
        return Vec2Fx::ZERO;
    };
    let dt = i64::try_from(c.last_seen.saturating_sub(p.last_seen)).unwrap_or(i64::MAX);
    if dt <= 0 {
        return Vec2Fx::ZERO;
    }
    Vec2Fx::new(
        Fx::from_raw((c.pos.x.raw() - p.pos.x.raw()) / dt),
        Fx::from_raw((c.pos.y.raw() - p.pos.y.raw()) / dt),
    )
}

impl Frame {
    /// Keeps the units within [`BELIEF_RADIUS_MM`] of the objective, then, while more than
    /// [`BELIEF_MAX_SOLDIERS`] soldiers remain, drops the farthest unit, ties to the highest id
    /// (so the lowest ids survive). A unit with no men is not kept.
    pub fn of(view: &BeliefView<'_>) -> Frame {
        let r2 = radius_raw(BELIEF_RADIUS_MM).pow(2);
        let near = |p: Vec2Fx| dist_sq(p, view.area.center) <= r2;
        let mut own: Vec<OwnReport> = view
            .reports
            .iter()
            .filter(|r| r.strength > 0 && near(r.pos))
            .copied()
            .collect();
        let mut foes: Vec<Foe> = view
            .contacts
            .iter()
            .filter(|c| c.strength_est > 0 && near(c.pos))
            .map(|c| Foe {
                contact: *c,
                vel: track(c, view.previous),
            })
            .collect();
        let total = |own: &[OwnReport], foes: &[Foe]| -> u32 {
            own.iter().map(|r| u32::from(r.strength)).sum::<u32>()
                + foes
                    .iter()
                    .map(|f| u32::from(f.contact.strength_est))
                    .sum::<u32>()
        };
        while total(&own, &foes) > BELIEF_MAX_SOLDIERS {
            // Key (distance, id, kind, index): the largest goes, so the lowest ids survive a tie.
            let c = view.area.center;
            let own_keys = own
                .iter()
                .enumerate()
                .map(|(i, r)| (dist_sq(r.pos, c), u32::from(r.unit.0), 0u8, i));
            let foe_keys = foes
                .iter()
                .enumerate()
                .map(|(j, f)| (dist_sq(f.contact.pos, c), f.contact.id.0, 1u8, j));
            let Some((_, _, kind, i)) = own_keys.chain(foe_keys).max() else {
                break;
            };
            if kind == 0 {
                own.remove(i);
            } else {
                foes.remove(i);
            }
        }
        Frame {
            side: view.side,
            area: view.area,
            own,
            foes,
        }
    }

    /// Index of the regiment of an own unit in the belief world.
    pub fn own_index(&self, unit: UnitRef) -> Option<usize> {
        self.own.iter().position(|r| r.unit == unit)
    }

    /// Index of the regiment of a contact in the belief world.
    pub fn foe_index(&self, id: ContactId) -> Option<usize> {
        self.foes
            .iter()
            .position(|f| f.contact.id == id)
            .map(|j| self.own.len() + j)
    }
}

/// `strength * (1000 + noise_pm * g) / 1000` for a standard-normal draw `g`, at least 1 and at
/// most `u16::MAX`; the product is truncated toward zero.
pub(crate) fn noised(strength: u16, noise_pm: u16, g: Fx) -> u16 {
    let delta = i64::from(noise_pm) * g.raw() / (1i64 << Fx::FRAC_BITS);
    let pm = (1000 + delta).max(0);
    u16::try_from((i64::from(strength) * pm / 1000).clamp(1, i64::from(u16::MAX))).unwrap_or(1)
}
