//! Casualties (SPEC-005 section 6): the queue of hits, the log, application at the end of the
//! tick and the release of expired disables.

use crate::hit::QueuedHit;
use crate::{BattleState, FireExt, HitOutcome, F_DISABLED, F_WOUNDED};

/// Hit points of a wounded soldier.
const WOUNDED_HP: u16 = 25;
/// Hit points of a disabled soldier.
pub(crate) const DISABLED_HP: u16 = 60;

/// What happened to a casualty.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CasualtyKind {
    /// Dead; removed from the store.
    Killed,
    /// Out for the battle.
    Wounded,
    /// Down for a number of ticks.
    Disabled,
}

/// One entry of the persistent casualty log.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Casualty {
    /// Tick on which the hit was applied.
    pub tick: u64,
    /// Slot index of the casualty (`u32::MAX` for an aggregate, ADR 0010 D-07).
    pub target_index: u32,
    /// The casualty's unit.
    pub unit: u32,
    /// The casualty's team.
    pub team: u8,
    /// What happened.
    pub kind: CasualtyKind,
    /// Unit of the shooter.
    pub shooter_unit: u32,
    /// Ammunition row of the round.
    pub ammo: u8,
}

impl BattleState {
    /// The casualty log, in order of application.
    pub fn casualties(&self) -> &[Casualty] {
        &self.casualties
    }

    /// Queues a hit for [`BattleState::step_casualties`]. The flight phase does this itself;
    /// the method is the hook for tests and for later layers that wound soldiers.
    #[doc(hidden)]
    pub fn push_hit(&mut self, hit: QueuedHit) {
        self.pending.push(hit);
    }

    /// Applies the queued hits (REGISTRY TK-10): grouped by target ascending; per target the most
    /// severe outcome wins, ties to the lowest `seq`, `disabled_ticks` the maximum of the group.
    /// A killed soldier is logged and then despawned; a wounded one gets `F_WOUNDED` and 25 hit
    /// points; a disabled one `F_DISABLED`, 60 hit points and `disabled_until`.
    pub fn step_casualties(&mut self) {
        let mut queue = std::mem::take(&mut self.pending);
        queue.sort_by_key(|h| (h.target.index, std::cmp::Reverse(h.outcome), h.seq));
        let tick = self.world.tick;
        let mut i = 0;
        while i < queue.len() {
            let win = queue[i];
            let end = queue[i..]
                .iter()
                .position(|h| h.target != win.target)
                .map_or(queue.len(), |n| i + n);
            let ticks = queue[i..end]
                .iter()
                .map(|h| h.disabled_ticks)
                .max()
                .unwrap_or(0);
            i = end;
            let agents = &mut self.world.agents;
            let Some(a) = agents.get(win.target) else {
                continue;
            };
            let kind = match win.outcome {
                HitOutcome::Killed => CasualtyKind::Killed,
                HitOutcome::Wounded => CasualtyKind::Wounded,
                HitOutcome::Disabled => CasualtyKind::Disabled,
                HitOutcome::NoEffect => continue,
            };
            self.casualties.push(Casualty {
                tick,
                target_index: win.target.index,
                unit: a.unit,
                team: a.team,
                kind,
                shooter_unit: win.shooter_unit,
                ammo: win.ammo,
            });
            match kind {
                CasualtyKind::Killed => {
                    // The rounds a killed soldier carried leave the ledger as lost.
                    let ui = a.unit as usize;
                    if self.regfire.get(ui).is_some_and(|r| r.weapon.is_some()) {
                        let carried = self
                            .fire
                            .get(win.target.index as usize)
                            .map_or(0, |f| f.rounds);
                        self.regiments[ui].acct.lost += u64::from(carried);
                    }
                    agents.despawn(win.target);
                }
                CasualtyKind::Wounded => {
                    agents.set_flags(win.target, a.flags | F_WOUNDED);
                    agents.set_hp(win.target, WOUNDED_HP);
                }
                CasualtyKind::Disabled => {
                    agents.set_flags(win.target, a.flags | F_DISABLED);
                    agents.set_hp(win.target, DISABLED_HP);
                    if self.fire.len() < agents.slot_count() {
                        self.fire.resize(agents.slot_count(), FireExt::UNEQUIPPED);
                    }
                    self.fire[win.target.index as usize].disabled_until = tick + u64::from(ticks);
                }
            }
        }
    }

    /// Releases every disabled soldier whose `disabled_until` has come (REGISTRY TK-05): the
    /// flag is cleared and the soldier rejoins the roster of its regiment (amendment A5).
    pub fn release_disables(&mut self) {
        let tick = self.world.tick;
        let fire = &self.fire;
        let due: Vec<_> = self
            .world
            .agents
            .iter_alive()
            .filter(|id| {
                fire.get(id.index as usize)
                    .is_some_and(|f| f.disabled_until != 0 && f.disabled_until <= tick)
            })
            .collect();
        for id in due {
            let agents = &mut self.world.agents;
            self.fire[id.index as usize].disabled_until = 0;
            let flags = agents.flags(id).unwrap_or(0);
            if flags & F_DISABLED == 0 {
                continue;
            }
            agents.set_flags(id, flags & !F_DISABLED);
            if flags & crate::registry::F_SKULK != 0 {
                // An absent soldier stays off the roster until the morale step returns it.
                continue;
            }
            let unit = agents.unit(id).unwrap_or(u32::MAX);
            if let Some(reg) = self.regiments.get_mut(unit as usize) {
                if let Err(at) = reg.roster.binary_search(&id) {
                    reg.roster.insert(at, id);
                    reg.dirty = true;
                }
            }
        }
    }

    /// Roster updates (REGISTRY TK-14): casualties leave their regiment's roster and mark it
    /// `dirty` for the close-up.
    pub fn step_rosters(&mut self) {
        for reg in &mut self.regiments {
            crate::battle::sync_roster(&self.world.agents, reg);
        }
    }
}
