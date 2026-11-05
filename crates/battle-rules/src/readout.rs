//! Setters and read accessors for building a world from reports and for the interface
//! (SPEC-004 amendment A8, SPEC-005 amendment A13). None of them runs in a step.

use crate::{BattleState, FireExt, FireMode, RegimentId, F2, FATIGUE_MAX};

impl BattleState {
    /// Sets the fatigue of every rostered soldier to `min(ppm, FATIGUE_MAX)`; `false` for an
    /// unknown regiment (SPEC-004 amendment A8).
    pub fn set_fatigue(&mut self, id: RegimentId, ppm: u32) -> bool {
        let Some(reg) = self.regiments.get(usize::from(id.0)) else {
            return false;
        };
        for a in &reg.roster {
            if let Some(l) = self.loco.get_mut(a.index as usize) {
                l.fatigue = ppm.min(FATIGUE_MAX);
            }
        }
        true
    }

    /// Keeps the first `n` soldiers of the roster and despawns the rest, adding their rounds to
    /// the round account's `lost`; the regiment becomes `dirty`. `false` and no change for an
    /// unknown regiment, for `n` above the roster length and for a regiment of class 2
    /// (SPEC-004 amendment A8).
    pub fn set_strength(&mut self, id: RegimentId, n: usize) -> bool {
        let ri = usize::from(id.0);
        let Some(reg) = self.regiments.get(ri) else {
            return false;
        };
        if reg.class == F2 || n > reg.roster.len() {
            return false;
        }
        let equipped = self.regfire[ri].weapon.is_some();
        let gone: Vec<_> = self.regiments[ri].roster.split_off(n);
        let mut lost = 0u64;
        for a in gone {
            if equipped {
                lost += self
                    .fire
                    .get(a.index as usize)
                    .map_or(0, |f| u64::from(f.rounds));
            }
            self.world.agents.despawn(a);
        }
        let reg = &mut self.regiments[ri];
        reg.acct.lost += lost;
        reg.dirty = true;
        true
    }

    /// Fit soldiers of a regiment (SPEC-004 amendment A8).
    pub fn strength(&self, id: RegimentId) -> Option<u32> {
        let r = self.regiments.get(usize::from(id.0))?;
        Some(if r.class == F2 {
            r.aggregate.fit
        } else {
            u32::try_from(r.roster.len()).unwrap_or(u32::MAX)
        })
    }

    /// Mean fatigue of the roster in permille, truncating (SPEC-004 amendment A8).
    pub fn fatigue_pm(&self, id: RegimentId) -> Option<u16> {
        let r = self.regiments.get(usize::from(id.0))?;
        let sum: u64 = r
            .roster
            .iter()
            .filter_map(|a| self.loco.get(a.index as usize))
            .map(|l| u64::from(l.fatigue))
            .sum();
        let n = r.roster.len() as u64;
        Some(u16::try_from(sum.checked_div(n * 1000).unwrap_or(0)).unwrap_or(u16::MAX))
    }

    /// Starting rounds of the ammunition a regiment fires, `None` while unequipped.
    fn full_load(&self, id: RegimentId) -> Option<u64> {
        let wi = self.regfire.get(usize::from(id.0))?.weapon?;
        let w = self.tables.weapons.rows().get(usize::from(wi))?;
        let reg = self.regiments.get(usize::from(id.0))?;
        let sel = reg
            .roster
            .first()
            .and_then(|a| self.fire.get(a.index as usize))
            .map_or(0, |f| usize::from(f.ammo_sel));
        w.ammo.get(sel).map(|l| u64::from(l.rounds))
    }

    /// Gives every rostered soldier of an equipped regiment `load * min(pm, 1000) / 1000`
    /// rounds; a rise goes to `resupplied`, a fall to `lost`. `false` for an unknown or
    /// unequipped regiment (SPEC-005 amendment A13).
    pub fn set_rounds_pm(&mut self, id: RegimentId, pm: u16) -> bool {
        let Some(load) = self.full_load(id) else {
            return false;
        };
        let want = u16::try_from(load * u64::from(pm.min(1000)) / 1000).unwrap_or(u16::MAX);
        let ri = usize::from(id.0);
        let (mut up, mut down) = (0u64, 0u64);
        for a in &self.regiments[ri].roster {
            if let Some(f) = self.fire.get_mut(a.index as usize) {
                up += u64::from(want.saturating_sub(f.rounds));
                down += u64::from(f.rounds.saturating_sub(want));
                f.rounds = want;
            }
        }
        let acct = &mut self.regiments[ri].acct;
        acct.resupplied += up;
        acct.lost += down;
        true
    }

    /// Rounds held over a full load, permille, truncating (SPEC-005 amendment A13).
    pub fn rounds_pm(&self, id: RegimentId) -> Option<u16> {
        let load = self.full_load(id)?;
        let r = &self.regiments[usize::from(id.0)];
        let held: u64 = r
            .roster
            .iter()
            .filter_map(|a| self.fire.get(a.index as usize))
            .map(|f| u64::from(f.rounds))
            .sum();
        let full = load * r.roster.len() as u64;
        Some(u16::try_from((held * 1000).checked_div(full)?).unwrap_or(u16::MAX))
    }

    /// The fire mode of a regiment (SPEC-005 amendment A13).
    pub fn fire_mode(&self, id: RegimentId) -> Option<FireMode> {
        self.regfire.get(usize::from(id.0)).map(|r| r.mode)
    }

    /// The fire state of an agent slot (SPEC-005 amendment A13).
    pub fn fire_ext(&self, slot: usize) -> Option<&FireExt> {
        self.fire.get(slot)
    }
}
