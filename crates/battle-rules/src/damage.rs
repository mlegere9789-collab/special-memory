//! Damage table (SPEC-005 section 1): `data/rules/damage.toml`.
//!
//! Postures give a target's body size and cover; hit height maps to a [`Region`]; impact energy
//! ([`energy_joules`]) maps to one of four bands; a (band, region) cell gives the permille of
//! killed, wounded and disabled, and [`DamageTable::outcome`] turns a roll into a [`HitOutcome`]
//! through cumulative thresholds built at load. No cell may sum above 1000 permille.

use crate::ammo::{check_schema, take_u32};
use crate::{parse_table, AmmoKind, DataError, Provenance, Table};
use core_math::Fx;

/// Body posture of a target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Posture {
    /// Standing infantry.
    Standing,
    /// Kneeling infantry.
    Kneeling,
    /// Prone infantry.
    Prone,
    /// A gun and its crew (`F_GUN`).
    Gun,
}

/// Part of the body a projectile strikes, by hit height.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Region {
    /// Below the legs threshold.
    Legs,
    /// Between the thresholds.
    Torso,
    /// Above the torso threshold.
    Head,
}

/// Result of an outcome roll, ordered by severity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum HitOutcome {
    /// The hit does nothing.
    NoEffect = 0,
    /// Down for a number of ticks.
    Disabled = 1,
    /// Out for the battle.
    Wounded = 2,
    /// Dead.
    Killed = 3,
}

/// Body size and cover of one posture.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PostureRow {
    /// Which posture.
    pub posture: Posture,
    /// Width in millimetres.
    pub width_mm: u32,
    /// Height in millimetres.
    pub height_mm: u32,
    /// Height of the muzzle when firing, in millimetres.
    pub muzzle_height_mm: u32,
    /// Share of terrain cover this posture uses, in permille.
    pub cover_pm: u32,
    /// Row provenance.
    pub provenance: Provenance,
}

/// One (band, region) cell of the outcome table with its cumulative thresholds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutcomeCell {
    /// Permille killed.
    pub killed_pm: u32,
    /// Permille wounded.
    pub wounded_pm: u32,
    /// Permille disabled.
    pub disabled_pm: u32,
    /// Rolls below this (of 10^6) are killed.
    killed_below: u32,
    /// Rolls below this are killed or wounded.
    wounded_below: u32,
    /// Rolls below this are killed, wounded or disabled.
    disabled_below: u32,
    /// Row provenance.
    pub provenance: Provenance,
}

/// The loaded damage table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DamageTable {
    postures: [PostureRow; 4],
    legs_below_pm: u32,
    torso_below_pm: u32,
    band_min_j: [u32; 3],
    disabled_base_ticks: u32,
    disabled_span_ticks: u32,
    absorb_ball_j: u32,
    absorb_conical_j: u32,
    absorb_pellet_j: u32,
    absorb_shot_j: u32,
    cells: [[OutcomeCell; 3]; 4],
}

/// Kinetic energy `mass_g * speed^2 / 2000` in joules (mass in grams, speed in m/s) as `Fx`,
/// truncated toward zero; saturates at [`Fx::MAX`].
pub fn energy_joules(mass_g: u32, speed: Fx) -> Fx {
    let s = i128::from(speed.raw());
    let e = i128::from(mass_g)
        .checked_mul(s * s)
        .map(|n| n / (2000i128 << 32));
    match e {
        Some(raw) => Fx::from_raw(i64::try_from(raw).unwrap_or(i64::MAX)),
        None => Fx::MAX,
    }
}

fn region_index(r: Region) -> usize {
    match r {
        Region::Legs => 0,
        Region::Torso => 1,
        Region::Head => 2,
    }
}

fn take_inner(root: &mut Table, key: &str) -> Result<Table, DataError> {
    root.take_table(key)
}

fn load_postures(root: &mut Table) -> Result<[PostureRow; 4], DataError> {
    let mut found: [Option<PostureRow>; 4] = [None, None, None, None];
    for mut row in root.take_rows("posture")? {
        let (posture, slot) = match row.take_str("id")?.as_str() {
            "standing" => (Posture::Standing, 0),
            "kneeling" => (Posture::Kneeling, 1),
            "prone" => (Posture::Prone, 2),
            "gun" => (Posture::Gun, 3),
            other => {
                return Err(DataError::Parse(format!(
                    "{}.id: unknown posture {other:?}",
                    row.path()
                )))
            }
        };
        let width_mm = take_u32(&mut row, "width_mm", 1, 5000)?;
        let height_mm = take_u32(&mut row, "height_mm", 1, 5000)?;
        let muzzle_height_mm = take_u32(&mut row, "muzzle_height_mm", 0, 5000)?;
        let cover_pm = take_u32(&mut row, "cover_pm", 0, 1000)?;
        let provenance = row.take_provenance()?;
        row.finish()?;
        if found[slot].is_some() {
            return Err(DataError::Parse(format!(
                "posture {posture:?} listed twice"
            )));
        }
        found[slot] = Some(PostureRow {
            posture,
            width_mm,
            height_mm,
            muzzle_height_mm,
            cover_pm,
            provenance,
        });
    }
    let [Some(a), Some(b), Some(c), Some(d)] = found else {
        return Err(DataError::Parse(
            "posture needs standing, kneeling, prone and gun".to_owned(),
        ));
    };
    Ok([a, b, c, d])
}

fn load_cells(root: &mut Table) -> Result<[[OutcomeCell; 3]; 4], DataError> {
    let mut found: [[Option<OutcomeCell>; 3]; 4] = Default::default();
    for mut row in root.take_rows("outcome")? {
        let band = take_u32(&mut row, "band", 0, 3)? as usize;
        let region = match row.take_str("region")?.as_str() {
            "legs" => Region::Legs,
            "torso" => Region::Torso,
            "head" => Region::Head,
            other => {
                return Err(DataError::Parse(format!(
                    "{}.region: unknown region {other:?}",
                    row.path()
                )))
            }
        };
        let killed_pm = take_u32(&mut row, "killed_pm", 0, 1000)?;
        let wounded_pm = take_u32(&mut row, "wounded_pm", 0, 1000)?;
        let disabled_pm = take_u32(&mut row, "disabled_pm", 0, 1000)?;
        let provenance = row.take_provenance()?;
        let at = row.path().to_owned();
        row.finish()?;
        let sum = killed_pm + wounded_pm + disabled_pm;
        if sum > 1000 {
            return Err(DataError::Range(format!(
                "{at}: killed + wounded + disabled = {sum} permille exceeds 1000"
            )));
        }
        let slot = &mut found[band][region_index(region)];
        if slot.is_some() {
            return Err(DataError::Parse(format!(
                "{at}: band {band} {region:?} listed twice"
            )));
        }
        *slot = Some(OutcomeCell {
            killed_pm,
            wounded_pm,
            disabled_pm,
            killed_below: killed_pm * 1000,
            wounded_below: (killed_pm + wounded_pm) * 1000,
            disabled_below: sum * 1000,
            provenance,
        });
    }
    let mut out: Vec<[OutcomeCell; 3]> = Vec::with_capacity(4);
    for (band, cells) in found.into_iter().enumerate() {
        let [Some(a), Some(b), Some(c)] = cells else {
            return Err(DataError::Parse(format!(
                "outcome band {band} needs legs, torso and head"
            )));
        };
        out.push([a, b, c]);
    }
    out.try_into()
        .map_err(|_| DataError::Parse("outcome needs four bands".to_owned()))
}

impl DamageTable {
    /// Parses and validates `data/rules/damage.toml` text. A cell summing above 1000 permille is
    /// [`DataError::Range`]; every posture and all twelve cells must be present exactly once.
    pub fn from_toml_str(text: &str) -> Result<DamageTable, DataError> {
        let mut root = parse_table(text)?;
        check_schema(&mut root, "damage/1")?;
        let postures = load_postures(&mut root)?;

        let mut t = take_inner(&mut root, "regions")?;
        let legs_below_pm = take_u32(&mut t, "legs_below_pm", 1, 999)?;
        let torso_below_pm = take_u32(&mut t, "torso_below_pm", 2, 1000)?;
        t.take_provenance()?;
        t.finish()?;
        if legs_below_pm >= torso_below_pm {
            return Err(DataError::Range(
                "regions: legs_below_pm must be below torso_below_pm".to_owned(),
            ));
        }

        let mut t = take_inner(&mut root, "energy_bands")?;
        let b1 = take_u32(&mut t, "b1_min_j", 1, 10_000_000)?;
        let b2 = take_u32(&mut t, "b2_min_j", 2, 10_000_000)?;
        let b3 = take_u32(&mut t, "b3_min_j", 3, 10_000_000)?;
        t.take_provenance()?;
        t.finish()?;
        if !(b1 < b2 && b2 < b3) {
            return Err(DataError::Range(
                "energy_bands: minimums must increase".to_owned(),
            ));
        }

        let mut t = take_inner(&mut root, "disabled")?;
        let disabled_base_ticks = take_u32(&mut t, "base_ticks", 1, 1_000_000)?;
        let disabled_span_ticks = take_u32(&mut t, "span_ticks", 1, 1_000_000)?;
        t.take_provenance()?;
        t.finish()?;

        let mut t = take_inner(&mut root, "absorb")?;
        let absorb_ball_j = take_u32(&mut t, "ball_j", 0, 10_000_000)?;
        let absorb_conical_j = take_u32(&mut t, "conical_j", 0, 10_000_000)?;
        let absorb_pellet_j = take_u32(&mut t, "pellet_j", 0, 10_000_000)?;
        let absorb_shot_j = take_u32(&mut t, "shot_j", 0, 10_000_000)?;
        t.take_provenance()?;
        t.finish()?;

        let cells = load_cells(&mut root)?;
        root.finish()?;
        Ok(DamageTable {
            postures,
            legs_below_pm,
            torso_below_pm,
            band_min_j: [b1, b2, b3],
            disabled_base_ticks,
            disabled_span_ticks,
            absorb_ball_j,
            absorb_conical_j,
            absorb_pellet_j,
            absorb_shot_j,
            cells,
        })
    }

    /// Body size and cover of a posture.
    pub fn posture(&self, p: Posture) -> &PostureRow {
        &self.postures[match p {
            Posture::Standing => 0,
            Posture::Kneeling => 1,
            Posture::Prone => 2,
            Posture::Gun => 3,
        }]
    }

    /// Region for a hit at `height_pm` permille of the target's height: below the legs
    /// threshold legs, below the torso threshold torso, else head.
    pub fn region_for_height(&self, height_pm: u32) -> Region {
        if height_pm < self.legs_below_pm {
            Region::Legs
        } else if height_pm < self.torso_below_pm {
            Region::Torso
        } else {
            Region::Head
        }
    }

    /// Energy band 0 to 3 of an impact energy in joules (B0 below the first minimum).
    pub fn band_for_energy(&self, energy_j: Fx) -> u8 {
        let e = i128::from(energy_j.raw());
        let mut band = 0;
        for min in self.band_min_j {
            if e >= i128::from(min) << 32 {
                band += 1;
            }
        }
        band
    }

    /// The (band, region) cell; a band above 3 is treated as 3.
    pub fn cell(&self, band: u8, region: Region) -> &OutcomeCell {
        &self.cells[usize::from(band.min(3))][region_index(region)]
    }

    /// Outcome of a roll: `roll % 10^6` is compared against the cumulative permille times 1000
    /// (killed, then wounded, then disabled); the remainder is no effect.
    pub fn outcome(&self, band: u8, region: Region, roll: u64) -> HitOutcome {
        let c = self.cell(band, region);
        let r = u32::try_from(roll % 1_000_000).unwrap_or(0);
        if r < c.killed_below {
            HitOutcome::Killed
        } else if r < c.wounded_below {
            HitOutcome::Wounded
        } else if r < c.disabled_below {
            HitOutcome::Disabled
        } else {
            HitOutcome::NoEffect
        }
    }

    /// Ticks a disabled soldier stays down: `base + roll % span`.
    pub fn disabled_ticks(&self, roll: u64) -> u32 {
        let span = u64::from(self.disabled_span_ticks);
        self.disabled_base_ticks + u32::try_from(roll % span).unwrap_or(0)
    }

    /// Energy a body hit absorbs from a round of this kind, in joules. Rounds that burst
    /// (shell, case, canister) strike as heavy shot.
    pub fn absorb_j(&self, kind: AmmoKind) -> u32 {
        match kind {
            AmmoKind::Ball => self.absorb_ball_j,
            AmmoKind::Conical => self.absorb_conical_j,
            AmmoKind::Shot | AmmoKind::Shell | AmmoKind::Case | AmmoKind::Canister => {
                self.absorb_shot_j
            }
        }
    }

    /// Energy a body hit absorbs from a burst pellet, in joules.
    pub fn absorb_pellet_j(&self) -> u32 {
        self.absorb_pellet_j
    }
}
