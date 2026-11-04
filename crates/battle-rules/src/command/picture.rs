//! Pictures as the interface and the AI read them (SPEC-007 section 7): confidence and radius of
//! a contact at a given tick, and [`BattleState::picture_view`].

use super::state::{Contact, FriendEntry};
use super::{CommandTable, OfficerId};
use crate::{BattleState, RegimentId};
use core_math::Vec2Fx;

/// `quality_pm * max(0, stale_ticks - age) / stale_ticks`, permille.
pub(crate) fn fade_pm(quality_pm: u32, age: u64, table: &CommandTable) -> u32 {
    let stale = u64::from(table.globals.stale_ticks);
    (u64::from(quality_pm) * stale.saturating_sub(age) / stale) as u32
}

/// Confidence of a contact at tick `now`, permille.
pub(crate) fn confidence_pm(c: &Contact, now: u64, table: &CommandTable) -> u32 {
    fade_pm(c.quality_pm, now.saturating_sub(c.seen), table)
}

/// Uncertainty radius of a contact at tick `now`: the sighting's `2 * sigma` plus the drift since.
pub(crate) fn radius_mm(c: &Contact, now: u64, table: &CommandTable) -> i64 {
    let age = i64::try_from(now.saturating_sub(c.seen)).unwrap_or(i64::MAX);
    c.uncertainty_mm
        .saturating_add(age.saturating_mul(table.globals.drift_mm_per_tick))
}

/// An enemy contact as shown. It never names the enemy regiment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContactView {
    /// Identity within the officer's picture.
    pub id: u32,
    /// Believed position.
    pub pos: Vec2Fx,
    /// Radius of the area the contact may lie in, mm.
    pub radius_mm: i64,
    /// Believed strength, men.
    pub strength: u32,
    /// Confidence now, permille.
    pub confidence_pm: u32,
    /// Ticks since it was last seen.
    pub age_ticks: u64,
    /// It came from a captured order.
    pub intercepted: bool,
}

/// A friendly regiment as the officer last heard of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FriendView {
    /// The regiment.
    pub regiment: RegimentId,
    /// Believed position.
    pub pos: Vec2Fx,
    /// Believed strength, men.
    pub strength: u32,
    /// Confidence now, permille (1000 when just reported, fading like a contact).
    pub confidence_pm: u32,
    /// Ticks since the entry was made.
    pub age_ticks: u64,
}

/// What one officer knows, in ascending id order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PictureView {
    /// Enemy contacts ascending by id.
    pub contacts: Vec<ContactView>,
    /// Friends ascending by regiment.
    pub friends: Vec<FriendView>,
}

fn friend_view(f: &FriendEntry, now: u64, table: &CommandTable) -> FriendView {
    let age = now.saturating_sub(f.seen);
    FriendView {
        regiment: f.regiment,
        pos: f.pos,
        strength: f.strength,
        confidence_pm: fade_pm(1000, age, table),
        age_ticks: age,
    }
}

impl BattleState {
    /// The picture of an officer now. Contacts whose confidence has fallen below `drop_pm` are
    /// not listed. With the side's `omniscient_view` the contacts are instead every living enemy
    /// regiment (numbered in ascending regiment order, never by its identity) at confidence 1000
    /// and radius 0; friends stay as the officer heard of them. `None` when the command layer is
    /// off or the officer does not exist.
    pub fn picture_view(&self, table: &CommandTable, officer: OfficerId) -> Option<PictureView> {
        let cmd = self.cmd.as_ref()?;
        let i = cmd.index_of(officer)?;
        let (now, pic) = (self.world.tick, &cmd.pics[i]);
        let side = cmd.officers[i].side;
        let contacts = if cmd.opt[usize::from(side)].omniscient_view {
            self.regiments
                .iter()
                .filter(|r| r.team != side && !r.roster.is_empty())
                .zip(0u32..)
                .map(|(r, id)| ContactView {
                    id,
                    pos: r.anchor,
                    radius_mm: 0,
                    strength: r.roster.len() as u32,
                    confidence_pm: 1000,
                    age_ticks: 0,
                    intercepted: false,
                })
                .collect()
        } else {
            pic.contacts
                .iter()
                .filter(|c| confidence_pm(c, now, table) >= table.globals.drop_pm)
                .map(|c| ContactView {
                    id: c.id,
                    pos: c.pos,
                    radius_mm: radius_mm(c, now, table),
                    strength: c.strength,
                    confidence_pm: confidence_pm(c, now, table),
                    age_ticks: now.saturating_sub(c.seen),
                    intercepted: c.intercepted,
                })
                .collect()
        };
        let friends = pic
            .friends
            .iter()
            .map(|f| friend_view(f, now, table))
            .collect();
        Some(PictureView { contacts, friends })
    }
}
