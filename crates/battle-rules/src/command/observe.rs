//! Observation (SPEC-007 section 7): what each regiment sees of the enemy every
//! `observe_interval_ticks`, and how sightings join an officer's picture as contacts.
//!
//! Visibility is read through the [`Visibility`] trait (ADR 0010, D-03): [`OwnVisibility`] is the
//! command layer's own smoke term, and an adapter over the SPEC-009 sight oracle can replace it
//! when the environment is present. The trait supplies line of sight, transmission and range;
//! this module adds cover, noise and the loss of the enemy's identity.

use super::leg::dist_mm;
use super::picture::{confidence_pm, radius_mm};
use super::state::{CommandState, Contact, Picture};
use super::CommandTable;
use crate::registry::STREAM_OBSERVE;
use crate::{fx_ratio, gauss_unit, BattleState, SmokeGrid};
use core_math::{Rng, SplitMix64, Vec2Fx};

/// What an observer can make out of a target, as an oracle.
pub trait Visibility {
    /// Share of the target's appearance that reaches the observer, permille: 1000 in clear air,
    /// 0 when nothing does.
    fn transmit_pm(&self, observer: Vec2Fx, target: Vec2Fx) -> u32;

    /// Whether the line from `observer` to `target` is open at all. Defaults to open.
    fn visible(&self, _observer: Vec2Fx, _target: Vec2Fx) -> bool {
        true
    }

    /// Range of the observer in mm, given the nominal range of his post. Defaults to the nominal.
    fn range_mm(&self, _observer: Vec2Fx, nominal_mm: i64) -> i64 {
        nominal_mm
    }
}

/// The command layer's own visibility: open ground and the smoke at the target,
/// `1000 - min(1000, smoke * smoke_occ_per_unit_pm)`.
pub struct OwnVisibility<'a> {
    smoke: &'a SmokeGrid,
    occ_per_unit_pm: u32,
}

impl<'a> OwnVisibility<'a> {
    /// Reads smoke from `smoke` with the constants of `table`.
    pub fn new(smoke: &'a SmokeGrid, table: &CommandTable) -> OwnVisibility<'a> {
        OwnVisibility {
            smoke,
            occ_per_unit_pm: table.globals.smoke_occ_per_unit_pm,
        }
    }
}

impl Visibility for OwnVisibility<'_> {
    fn transmit_pm(&self, _observer: Vec2Fx, target: Vec2Fx) -> u32 {
        let block = u64::from(self.smoke.density_at(target)) * u64::from(self.occ_per_unit_pm);
        1000 - block.min(1000) as u32
    }
}

/// Adds a sighting or a reported contact to a picture: it joins the nearest contact (lowest id on
/// ties) within `assoc_base_mm` plus that contact's radius, taking the newer position and a
/// quality-weighted strength, else becomes a new contact with the picture's next id.
pub(crate) fn join_contact(pic: &mut Picture, c: Contact, table: &CommandTable, now: u64) {
    let reach = |k: &Contact| table.globals.assoc_base_mm + radius_mm(k, now, table);
    let near = pic
        .contacts
        .iter()
        .enumerate()
        .filter(|(_, k)| dist_mm(k.pos, c.pos) <= reach(k))
        .min_by_key(|(_, k)| (dist_mm(k.pos, c.pos), k.id))
        .map(|(i, _)| i);
    let Some(i) = near else {
        pic.contacts.push(Contact {
            id: pic.next_contact,
            ..c
        });
        pic.next_contact += 1;
        return;
    };
    let k = &mut pic.contacts[i];
    let (wa, wb) = (u64::from(k.quality_pm), u64::from(c.quality_pm));
    let mixed = (u64::from(k.strength) * wa + u64::from(c.strength) * wb) / (wa + wb).max(1);
    if c.seen >= k.seen {
        *k = Contact { id: k.id, ..c };
    }
    k.strength = u32::try_from(mixed).unwrap_or(u32::MAX).max(1);
}

impl CommandState {
    /// Removes every contact whose confidence has fallen below `drop_pm`.
    fn drop_faded(&mut self, table: &CommandTable, now: u64) {
        for pic in &mut self.pics {
            pic.contacts
                .retain(|c| confidence_pm(c, now, table) >= table.globals.drop_pm);
        }
    }
}

impl BattleState {
    /// One observation round when the tick is a multiple of `observe_interval_ticks`, with the
    /// layer's own visibility. Does nothing while the layer is off.
    pub fn step_observation(&mut self, table: &CommandTable) {
        if self.cmd.is_none()
            || !self
                .world
                .tick
                .is_multiple_of(u64::from(table.globals.observe_interval_ticks))
        {
            return;
        }
        let Some(mut cmd) = self.cmd.take() else {
            return;
        };
        let vis = OwnVisibility::new(&self.smoke, table);
        self.observe_into(table, &mut cmd, &vis);
        self.cmd = Some(cmd);
    }

    /// One observation round now (whatever the tick) reading visibility from `vis`: drops faded
    /// contacts, then each regiment with an officer, ascending, observes enemy regiments with
    /// men, ascending. The random words come from `STREAM_OBSERVE` re-derived for this tick, so
    /// no generator state needs storing.
    pub fn observe_with(&mut self, table: &CommandTable, vis: &dyn Visibility) {
        let Some(mut cmd) = self.cmd.take() else {
            return;
        };
        self.observe_into(table, &mut cmd, vis);
        self.cmd = Some(cmd);
    }

    fn observe_into(&self, table: &CommandTable, cmd: &mut CommandState, vis: &dyn Visibility) {
        let (g, now) = (&table.globals, self.world.tick);
        cmd.drop_faded(table, now);
        let mut owners: Vec<(usize, usize)> = cmd
            .officers
            .iter()
            .enumerate()
            .filter_map(|(i, o)| o.regiment.map(|r| (usize::from(r.0), i)))
            .collect();
        owners.sort_unstable();
        let seed = self.world.rng.stream(STREAM_OBSERVE).next_u64() ^ now;
        let mut rng = Rng::from_seed(SplitMix64::new(seed).next_u64());
        for (ri, oi) in owners {
            let Some(me) = self.regiments.get(ri).filter(|r| !r.roster.is_empty()) else {
                continue;
            };
            let nominal = g.sight_range_mm * i64::from(cmd.officers[oi].post_range_pm) / 1000;
            let range = vis.range_mm(me.anchor, nominal);
            for en in self
                .regiments
                .iter()
                .filter(|e| e.team != me.team && !e.roster.is_empty())
            {
                let d = dist_mm(me.anchor, en.anchor);
                if d >= range || !vis.visible(me.anchor, en.anchor) {
                    continue;
                }
                let cover = self
                    .tables
                    .terrain
                    .class(self.terrain.class_at(en.anchor))
                    .map_or(0, |c| i64::from(c.cover_pm));
                let transmit = i64::from(vis.transmit_pm(me.anchor, en.anchor).min(1000));
                let q = (1000 - 1000 * d / range) * (1000 - cover) / 1000 * transmit / 1000;
                if q < i64::from(g.min_quality_pm) {
                    continue;
                }
                let (gx, gy, gs) = (
                    gauss_unit(&mut rng),
                    gauss_unit(&mut rng),
                    gauss_unit(&mut rng),
                );
                let sigma = g.pos_floor_mm + (1000 - q) * g.pos_sigma_max_mm / 1000;
                let off = |z: core_math::Fx| {
                    fx_ratio(
                        ((i128::from(sigma) * i128::from(z.raw())) >> 32) as i64,
                        1000,
                    )
                };
                let spread = (1000 - q) * i64::from(g.strength_sigma_pm) / 1000;
                let dev = (i128::from(spread) * i128::from(gs.raw())) >> 32;
                let factor = (1000 + dev).max(0);
                let strength = (en.roster.len() as i128 * factor / 1000).max(1);
                let sighting = Contact {
                    id: 0,
                    pos: en.anchor + Vec2Fx::new(off(gx), off(gy)),
                    strength: u32::try_from(strength).unwrap_or(u32::MAX),
                    quality_pm: q as u32,
                    seen: now,
                    uncertainty_mm: 2 * sigma,
                    intercepted: false,
                };
                join_contact(&mut cmd.pics[oi], sighting, table, now);
            }
        }
    }
}
