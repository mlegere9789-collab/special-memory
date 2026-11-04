//! The fire phase (SPEC-005 section 4): loading, misfire, fouling, ammunition and the spawn of
//! projectiles.
//!
//! Each equipped soldier owns a loading state machine ([`FireExt`]) and a random stream. Every
//! shot reads a fixed 50 words from that stream ([`crate::aim::draw_shot`]) whatever the
//! outcome, so a soldier's shots depend on nothing but its own stream and situation.

use crate::aim::{aim, draw_shot, launch_speeds, smoke_pm, AimInput};
use crate::registry::TAG_SET_FIRE_MODE;
use crate::smoke::shot_units;
use crate::{
    can_fire, fx_ratio, soldier_stream, BattleState, DataError, OrderDecodeError, Posture,
    Projectile, RangeTable, RegimentId, Training, WeaponRow, FATIGUE_MAX, F_DISABLED, F_GUN,
    F_KNEEL, F_PRONE, F_WOUNDED,
};
use core_math::{Angle, Rng, Vec2Fx};

/// `FireExt::step` value of a revolver waiting out `cycle_ticks` between rounds of a magazine.
pub const CYCLE_STEP: u8 = u8::MAX;

/// Loading speed by training in permille of the authored step ticks (placeholder, CT-005-4).
pub const fn load_pm(t: Training) -> u32 {
    match t {
        Training::Raw => 1200,
        Training::Trained => 1000,
        Training::Veteran => 840,
    }
}

/// Reads the 50 words of one shot from `rng` without building the shot and returns the first,
/// the misfire roll. An F1 shot (SPEC-008 section 6.1) uses this so that the stream after `k`
/// shots is the same whichever of them were F0 (SPEC-005 amendment A7).
pub fn discard_shot_words(rng: &mut Rng) -> u32 {
    let roll = rng.next_u32();
    for _ in 1..crate::aim::SHOT_WORDS {
        rng.next_u64();
    }
    roll
}

/// What a regiment shoots at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FireMode {
    /// No shooting.
    HoldFire,
    /// Every ready soldier fires at a member of `target`, spread by roster index.
    AtWill {
        /// The regiment fired on.
        target: RegimentId,
    },
}

/// One soldier's fire state, indexed by agent slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FireExt {
    /// Rounds left of the selected ammunition.
    pub rounds: u16,
    /// Position of the selected ammunition in the weapon's ammunition list.
    pub ammo_sel: u8,
    /// Loading step: an index into the weapon's steps, the step count when ready, or
    /// [`CYCLE_STEP`].
    pub step: u8,
    /// Ticks left in the current step.
    pub left: u16,
    /// Barrel fouling in permille, 0 to 1000.
    pub fouling: u16,
    /// Rounds in the magazine (1 or 0 for single-shot weapons).
    pub mag: u8,
    /// Tick until which the soldier is disabled (written by the casualty rules).
    pub disabled_until: u64,
    /// State of the soldier's random stream.
    pub rng: [u64; 4],
}

impl FireExt {
    /// A soldier with no weapon: no rounds, never fires.
    pub const UNEQUIPPED: FireExt = FireExt {
        rounds: 0,
        ammo_sel: 0,
        step: 0,
        left: 0,
        fouling: 0,
        mag: 0,
        disabled_until: 0,
        rng: [0; 4],
    };
}

/// Weather the fire and flight rules read (scenario values).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Weather {
    /// Wind velocity in m/s.
    pub wind: Vec2Fx,
    /// Powder dampness: 1000 dry; the misfire chance is scaled by `wet_pm / 1000`.
    pub wet_pm: u32,
    /// Ground restitution factor in permille: 1000 dry, 500 wet.
    pub ground_pm: u32,
}

impl Weather {
    /// Dry powder, dry ground, no wind.
    pub const DRY: Weather = Weather {
        wind: Vec2Fx::ZERO,
        wet_pm: 1000,
        ground_pm: 1000,
    };
}

/// Counters of the F0 rule (never hashed).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counters {
    /// Shots taken, misfires included.
    pub shots_fired: u64,
    /// Rounds that misfired.
    pub misfires: u64,
    /// Projectiles created by shots (children of bursts are counted by the burst rules).
    pub projectiles_spawned: u64,
    /// Hit chances drawn per unit or volley; always 0.
    pub unit_level_rolls: u64,
}

/// A regiment's weapon and what it shoots at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RegFire {
    /// Weapon row, `None` while unequipped.
    pub weapon: Option<u8>,
    /// Ammunition row fired.
    pub ammo_row: u8,
    /// Index of the range table in `BattleState::ranges`.
    pub range: u16,
    pub mode: FireMode,
}

impl RegFire {
    pub(crate) const NONE: RegFire = RegFire {
        weapon: None,
        ammo_row: 0,
        range: 0,
        mode: FireMode::HoldFire,
    };
}

/// A range table with the weapon and ammunition rows it was built for.
#[derive(Clone, Debug)]
pub(crate) struct RangeEntry {
    pub weapon: u8,
    pub ammo_row: u8,
    pub table: RangeTable,
}

/// Misfire chance in ppm: `(misfire_ppm + misfire_fouling_ppm * fouling / 1000) * wet_pm / 1000`.
pub fn misfire_ppm(w: &WeaponRow, fouling: u32, wet_pm: u32) -> u32 {
    let base =
        u64::from(w.misfire_ppm) + u64::from(w.misfire_fouling_ppm) * u64::from(fouling) / 1000;
    u32::try_from(base * u64::from(wet_pm) / 1000).unwrap_or(u32::MAX)
}

/// Ticks of loading step `step`: `ceil(ticks * load_pm / 1000)`, and for the `ram` step also
/// times `(1000 + ram_penalty_pm * fouling / 1000) / 1000`, rounded up.
pub fn step_ticks(w: &WeaponRow, step: usize, training: Training, fouling: u32) -> u16 {
    let mut t = (u64::from(w.load_steps[step].ticks) * u64::from(load_pm(training))).div_ceil(1000);
    if w.ram_step() == Some(step) {
        let pm = 1000 + u64::from(w.ram_penalty_pm) * u64::from(fouling) / 1000;
        t = (t * pm).div_ceil(1000);
    }
    t.clamp(1, u64::from(u16::MAX)) as u16
}

/// Moves a soldier whose step just ran out to the next step, or to Ready. A finished reload
/// refills the magazine; a finished cycle wait keeps it.
fn advance(f: &mut FireExt, w: &WeaponRow, training: Training) {
    let n = w.load_steps.len();
    if f.step == CYCLE_STEP {
        f.step = n as u8;
        return;
    }
    let next = usize::from(f.step) + 1;
    if next >= n {
        f.step = n as u8;
        f.mag = w.magazine.min(255) as u8;
        return;
    }
    f.step = next as u8;
    f.left = step_ticks(w, next, training, u32::from(f.fouling));
}

/// Restarts loading after a round was spent: wait out the cycle with rounds in the magazine,
/// else load from step `restart`; a soldier with no rounds left starts afresh.
fn after_discharge(f: &mut FireExt, w: &WeaponRow, training: Training, restart: usize) {
    f.mag = f.mag.saturating_sub(1);
    let start = if f.rounds == 0 {
        f.mag = 0;
        Some(0)
    } else if f.mag > 0 {
        None
    } else {
        Some(restart)
    };
    match start {
        Some(s) => {
            f.step = s as u8;
            f.left = step_ticks(w, s, training, u32::from(f.fouling));
        }
        None => {
            f.step = CYCLE_STEP;
            f.left = w.cycle_ticks.clamp(1, u32::from(u16::MAX)) as u16;
        }
    }
}

pub(crate) fn posture_of(flags: u16) -> Posture {
    if flags & F_GUN != 0 {
        Posture::Gun
    } else if flags & F_PRONE != 0 {
        Posture::Prone
    } else if flags & F_KNEEL != 0 {
        Posture::Kneeling
    } else {
        Posture::Standing
    }
}

/// Muzzle offset ahead of the soldier in millimetres.
const MUZZLE_AHEAD_MM: i64 = 600;

impl BattleState {
    /// Gives every soldier of a regiment the weapon `weapon` (a row id of the weapon table) with
    /// the ammunition at position `ammo_sel` of its list and that ammunition's starting rounds.
    /// Each soldier starts loading with a random offset (`range_u32(0, step ticks)` from its own
    /// stream) so that regiments do not fire in lockstep. The range table of the pair is built
    /// when first needed.
    ///
    /// Errors: [`DataError::Parse`] for an unknown regiment, weapon or ammunition position.
    pub fn equip_regiment(
        &mut self,
        id: RegimentId,
        weapon: &str,
        ammo_sel: u8,
    ) -> Result<(), DataError> {
        let ri = usize::from(id.0);
        if ri >= self.regiments.len() {
            return Err(DataError::Parse(format!("unknown regiment {}", id.0)));
        }
        let wi = self
            .tables
            .weapons
            .index_of(weapon)
            .ok_or_else(|| DataError::Parse(format!("unknown weapon {weapon}")))?;
        let w = &self.tables.weapons.rows()[wi];
        let load = w
            .ammo
            .get(usize::from(ammo_sel))
            .ok_or_else(|| DataError::Parse(format!("{weapon} has no ammunition {ammo_sel}")))?;
        let ai = self
            .tables
            .ammo
            .index_of(&load.id)
            .ok_or_else(|| DataError::Parse(format!("unknown ammunition {}", load.id)))?;
        let key = (wi as u8, ai as u8);
        let range = match self
            .ranges
            .iter()
            .position(|e| (e.weapon, e.ammo_row) == key)
        {
            Some(i) => i,
            None => {
                let row = &self.tables.ammo.rows()[ai];
                let gravity = self.tables.ammo.constants().gravity;
                self.ranges.push(RangeEntry {
                    weapon: key.0,
                    ammo_row: key.1,
                    table: RangeTable::build(row, gravity, w.reach_mm),
                });
                self.ranges.len() - 1
            }
        };
        let rounds = load.rounds;
        let training = self.regiments[ri].training;
        if self.fire.len() < self.world.agents.slot_count() {
            self.fire
                .resize(self.world.agents.slot_count(), FireExt::UNEQUIPPED);
        }
        let first = step_ticks(w, 0, training, 0);
        let had = self.regfire[ri].weapon.is_some();
        let (mut replaced, mut given) = (0u64, 0u64);
        for a in &self.regiments[ri].roster {
            let mut rng = self.world.rng.stream(soldier_stream(a.index));
            let left = rng.range_u32(0, u32::from(first)) as u16;
            if had {
                replaced += u64::from(self.fire[a.index as usize].rounds);
            }
            given += u64::from(rounds);
            self.fire[a.index as usize] = FireExt {
                rounds,
                ammo_sel,
                step: 0,
                left,
                fouling: 0,
                mag: 0,
                disabled_until: 0,
                rng: rng.state(),
            };
        }
        self.regiments[ri].acct.lost += replaced;
        self.regiments[ri].acct.initial += given;
        let mode = self.regfire[ri].mode;
        self.regfire[ri] = RegFire {
            weapon: Some(wi as u8),
            ammo_row: ai as u8,
            range: range as u16,
            mode,
        };
        Ok(())
    }

    /// Sets what a regiment shoots at; `false` for an unknown regiment.
    pub fn set_fire_mode(&mut self, id: RegimentId, mode: FireMode) -> bool {
        match self.regfire.get_mut(usize::from(id.0)) {
            Some(r) => {
                r.mode = mode;
                true
            }
            None => false,
        }
    }

    /// Adds `n` rounds (saturating) to every soldier of an equipped regiment; `false` for an
    /// unknown or unequipped regiment. A soldier that had run dry loads again.
    pub fn resupply(&mut self, id: RegimentId, n: u16) -> bool {
        let ri = usize::from(id.0);
        if self.regfire.get(ri).is_none_or(|r| r.weapon.is_none()) {
            return false;
        }
        let mut added = 0u64;
        for a in &self.regiments[ri].roster {
            if let Some(f) = self.fire.get_mut(a.index as usize) {
                let before = f.rounds;
                f.rounds = f.rounds.saturating_add(n);
                added += u64::from(f.rounds - before);
            }
        }
        self.regiments[ri].acct.resupplied += added;
        true
    }

    /// The F0 counters.
    pub fn counters(&self) -> Counters {
        self.counters
    }

    /// The fire phase of one tick (SPEC-005 section 4, REGISTRY 6.4), shooters in ascending slot
    /// order: a soldier with rounds advances its loading; when ready, its regiment may fire
    /// (`can_fire`, `AtWill`), it is not down and its target, `target_roster[j % len]` for roster
    /// index `j`, is within extended range, it takes a shot. A shot reads 50 stream words,
    /// spends a round and either misfires (nothing leaves the barrel, loading restarts at
    /// `misfire_restart_step`, a quarter of the smoke) or spawns a projectile with the next
    /// `seq`, adds fouling, fatigue and smoke at the muzzle, and restarts loading.
    ///
    /// A shooter with no valid target draws nothing and keeps its place in the loading cycle.
    pub fn step_fire(&mut self) {
        let (drain_pm, _) = self.heat_pm();
        let BattleState {
            world,
            regiments,
            loco,
            tables,
            fire,
            pool,
            smoke,
            seq,
            regfire,
            ranges,
            weather,
            counters,
            env,
            ..
        } = self;
        let source = env.as_ref().map(|e| &e.table().smoke);
        if regfire.iter().all(|r| r.weapon.is_none()) {
            return;
        }
        let agents = &world.agents;
        if fire.len() < agents.slot_count() {
            fire.resize(agents.slot_count(), FireExt::UNEQUIPPED);
        }
        let mut shooters: Vec<(u32, u16, u32)> = Vec::new();
        for (ri, rf) in regfire.iter().enumerate() {
            if rf.weapon.is_some() {
                for (j, a) in regiments[ri].roster.iter().enumerate() {
                    shooters.push((a.index, ri as u16, j as u32));
                }
            }
        }
        shooters.sort_unstable();
        let mut spent = vec![0u64; regiments.len()];
        for (idx, ri, j) in shooters {
            let (reg, rf) = (&regiments[usize::from(ri)], &regfire[usize::from(ri)]);
            let Some(wi) = rf.weapon else { continue };
            let w = &tables.weapons.rows()[usize::from(wi)];
            let id = reg.roster[j as usize];
            let (Some(flags), Some(pos)) = (agents.flags(id), agents.pos(id)) else {
                continue;
            };
            let f = &mut fire[idx as usize];
            if flags & (F_DISABLED | F_WOUNDED) != 0 || f.rounds == 0 {
                continue;
            }
            let n = w.load_steps.len();
            if usize::from(f.step) != n {
                f.left = f.left.saturating_sub(1);
                if f.left == 0 {
                    advance(f, w, reg.training);
                }
            }
            if usize::from(f.step) != n || !can_fire(reg) {
                continue;
            }
            let FireMode::AtWill { target } = rf.mode else {
                continue;
            };
            let Some(troster) = regiments.get(usize::from(target.0)).map(|t| &t.roster) else {
                continue;
            };
            if troster.is_empty() {
                continue;
            }
            let Some(tpos) = agents.pos(troster[j as usize % troster.len()]) else {
                continue;
            };
            let delta = tpos - pos;
            let range = delta.length();
            if range > fx_ratio(i64::from(w.extended_mm), 1000) {
                continue;
            }

            let mut rng = Rng::from_state(f.rng);
            let d = draw_shot(&mut rng);
            f.rng = rng.state();
            counters.shots_fired += 1;
            spent[usize::from(ri)] += 1;
            f.rounds -= 1;
            let forward = Vec2Fx::from_angle(agents.facing(id).unwrap_or(reg.heading));
            let muzzle = pos + forward.scale(fx_ratio(MUZZLE_AHEAD_MM, 1000));
            let chance = misfire_ppm(w, u32::from(f.fouling), weather.wet_pm);
            if d.misfire_roll % 1_000_000 < chance {
                counters.misfires += 1;
                match source {
                    Some(p) => smoke.add_smoke(muzzle, w.powder_mg, true, p),
                    None => smoke.add(muzzle, shot_units(w.powder_mg, true)),
                }
                after_discharge(f, w, reg.training, usize::from(w.misfire_restart_step));
                continue;
            }
            let ammo = &tables.ammo.rows()[usize::from(rf.ammo_row)];
            let a = aim(
                &AimInput {
                    weapon: w,
                    ammo,
                    table: &ranges[usize::from(rf.range)].table,
                    training: reg.training,
                    fatigue_ppm: loco.get(idx as usize).map_or(0, |l| l.fatigue),
                    smoke_pm: smoke_pm(smoke.cell_density(pos)),
                    range,
                    bearing: Angle::atan2(delta.y, delta.x),
                },
                &d,
            );
            let (horizontal, vz) = launch_speeds(ammo.muzzle_mm_s, a.elevation);
            let muzzle_mm = tables.damage.posture(posture_of(flags)).muzzle_height_mm;
            pool.push(Projectile {
                seq: *seq,
                owner: idx,
                unit: u32::from(reg.id.0),
                team: reg.team,
                ammo: rf.ammo_row,
                pos: muzzle,
                z: fx_ratio(i64::from(muzzle_mm), 1000),
                vel: Vec2Fx::from_angle(a.azimuth).scale(horizontal),
                vz,
                travelled_mm: 0,
                bounces: 0,
                hits: 0,
                fuse: a.fuse,
                seed: d.seed,
            });
            *seq += 1;
            counters.projectiles_spawned += 1;
            match source {
                Some(p) => smoke.add_smoke(muzzle, w.powder_mg, false, p),
                None => smoke.add(muzzle, shot_units(w.powder_mg, false)),
            }
            f.fouling = (u32::from(f.fouling) + w.fouling_per_shot).min(1000) as u16;
            if let Some(l) = loco.get_mut(idx as usize) {
                let drain = u64::from(w.fire_drain_ppm) * u64::from(drain_pm) / 1000;
                l.fatigue = u32::try_from(u64::from(l.fatigue) + drain)
                    .unwrap_or(FATIGUE_MAX)
                    .min(FATIGUE_MAX);
            }
            after_discharge(f, w, reg.training, 0);
        }
        for (reg, n) in regiments.iter_mut().zip(spent) {
            reg.acct.spent += n;
        }
    }

    /// The smoke part of the end of a tick (TK-12): with `env` `None` every cell decays
    /// (SPEC-005 section 8); with `env` `Some` the SPEC-009 update runs when
    /// `tick % smoke.period_ticks == period_ticks - 1` and nothing else changes the grid.
    pub fn step_smoke(&mut self) {
        if self.env.is_some() {
            self.update_smoke_env();
        } else {
            self.smoke.decay();
        }
    }
}

/// Encodes `set_fire_mode` as a replay input payload: `u8 5, u16 regiment, u8 mode (0 hold, 1 at
/// will), u16 target` (zero when holding), little-endian.
pub fn encode_fire_mode(regiment: RegimentId, mode: FireMode) -> Vec<u8> {
    let (m, t) = match mode {
        FireMode::HoldFire => (0u8, 0u16),
        FireMode::AtWill { target } => (1, target.0),
    };
    let mut out = vec![TAG_SET_FIRE_MODE];
    out.extend_from_slice(&regiment.0.to_le_bytes());
    out.push(m);
    out.extend_from_slice(&t.to_le_bytes());
    out
}

/// Decodes a payload written by [`encode_fire_mode`]. An unknown mode byte is reported as
/// [`OrderDecodeError::UnknownTag`].
pub fn decode_fire_mode(bytes: &[u8]) -> Result<(RegimentId, FireMode), OrderDecodeError> {
    let &[tag, r0, r1, m, t0, t1] = bytes else {
        return Err(if bytes.len() < 6 {
            OrderDecodeError::Truncated
        } else {
            OrderDecodeError::TrailingBytes
        });
    };
    if tag != TAG_SET_FIRE_MODE {
        return Err(OrderDecodeError::UnknownTag(tag));
    }
    let mode = match m {
        0 => FireMode::HoldFire,
        1 => FireMode::AtWill {
            target: RegimentId(u16::from_le_bytes([t0, t1])),
        },
        other => return Err(OrderDecodeError::UnknownTag(other)),
    };
    Ok((RegimentId(u16::from_le_bytes([r0, r1])), mode))
}
