//! Ammunition table (SPEC-005 section 1): `data/rules/ammo.toml`.
//!
//! Rows are kept in file order; an ammunition's index in [`AmmoTable::rows`] is its `ammo`
//! number in a projectile. Two numbers are derived at load, in `i128` integers only: the drag
//! constant `k = rho * cd * A / (2 m)` of every projectile and burst pellet ([`drag_k`]), and the
//! mass of a burst pellet that gives no mass of its own ([`sphere_mass_g`], iron density times
//! sphere volume).

use crate::{fx_ratio, parse_table, DataError, Provenance, Table};
use core_math::Fx;

/// `pi` as the rational `PI_NUM / PI_DEN` (sixteen digits, far below the 1e-6 requirement).
const PI_NUM: i128 = 3_141_592_653_589_793;
const PI_DEN: i128 = 1_000_000_000_000_000;

/// What a round is and how it behaves on impact.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AmmoKind {
    /// Round lead ball.
    Ball,
    /// Conical rifle bullet.
    Conical,
    /// Solid cannon shot; ricochets (needs `restitution_pm`).
    Shot,
    /// Explosive shell; bursts into fragments (needs a burst).
    Shell,
    /// Case shot; bursts into pellets (needs a burst).
    Case,
    /// Canister; bursts into pellets at the muzzle (needs a burst).
    Canister,
}

/// Direction rule of a burst's children.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BurstShape {
    /// Parent direction plus a Gaussian error in azimuth and elevation.
    Cone,
    /// Uniform azimuth, elevation within 45 degrees, fixed speed.
    Sphere,
}

/// How a shell, case or canister breaks up.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Burst {
    /// Number of children.
    pub count: u32,
    /// Mass of one child in grams; derived from the iron density when the row gives none.
    pub pellet_mass_g: u32,
    /// Diameter of one child in millimetres.
    pub pellet_diameter_mm: u32,
    /// Drag coefficient of a child, times 1000.
    pub pellet_cd_pm: u32,
    /// Direction rule.
    pub shape: BurstShape,
    /// Standard deviation of the cone error, in micro-radians.
    pub spread_sigma_urad: u32,
    /// Child speed in permille. A cone takes it of the parent's speed at the burst; a sphere
    /// takes it of the ammunition's muzzle speed, so the speed is fixed.
    pub pellet_speed_pm: u32,
    /// Standard deviation of the fuse error in sub-steps.
    pub fuse_sigma_substeps: u32,
    /// Chance in parts per million that the round fails to burst and flies on as shot.
    pub dud_ppm: u32,
    /// Drag constant of a child in 1/m, derived at load.
    pub pellet_drag_k: Fx,
}

/// One ammunition row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AmmoRow {
    /// Unique id.
    pub id: String,
    /// Kind.
    pub kind: AmmoKind,
    /// Mass in grams.
    pub mass_g: u32,
    /// Diameter in millimetres.
    pub diameter_mm: u32,
    /// Drag coefficient times 1000.
    pub cd_pm: u32,
    /// Muzzle speed in millimetres per second.
    pub muzzle_mm_s: u32,
    /// Ground restitution in permille (shot only).
    pub restitution_pm: Option<u32>,
    /// Burst description (shell, case, canister only).
    pub burst: Option<Burst>,
    /// Drag constant `k` in 1/m, derived at load.
    pub drag_k: Fx,
    /// Row provenance.
    pub provenance: Provenance,
}

/// Physical constants shared by every round.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Constants {
    /// Air density in grams per cubic metre.
    pub air_density_g_m3: u32,
    /// Iron density in kilograms per cubic metre.
    pub iron_density_kg_m3: u32,
    /// Gravity in m/s^2 (from micrometres per second squared through [`fx_ratio`]).
    pub gravity: Fx,
    /// Provenance of the constants.
    pub provenance: Provenance,
}

/// The loaded ammunition table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AmmoTable {
    constants: Constants,
    rows: Vec<AmmoRow>,
}

/// Drag constant `k = rho * cd * A / (2 m)` in 1/m with `A = pi d^2 / 4`, as `Fx`, computed in
/// `i128` and rounded to nearest. Units: air density g/m^3, `cd_pm` drag coefficient times 1000,
/// diameter mm, mass g. A zero mass or an overflow gives [`Fx::MAX`].
pub fn drag_k(air_density_g_m3: u32, cd_pm: u32, diameter_mm: u32, mass_g: u32) -> Fx {
    // k = rho_g * cd_pm * pi * d_mm^2 / (8e9 * m_g).
    let num = i128::from(air_density_g_m3)
        .checked_mul(i128::from(cd_pm))
        .and_then(|n| n.checked_mul(i128::from(diameter_mm)))
        .and_then(|n| n.checked_mul(i128::from(diameter_mm)))
        .and_then(|n| n.checked_mul(PI_NUM))
        .and_then(|n| n.checked_mul(1 << 32));
    let den = 8_000_000_000i128 * i128::from(mass_g) * PI_DEN;
    match (num, den) {
        (Some(n), d) if d > 0 => {
            let raw = (n + d / 2) / d;
            Fx::from_raw(i64::try_from(raw).unwrap_or(i64::MAX))
        }
        _ => Fx::MAX,
    }
}

/// Mass in whole grams (nearest) of a solid sphere: `density * pi/6 * d^3`. Units: density kg/m^3,
/// diameter mm. Saturates at `u32::MAX`.
pub fn sphere_mass_g(density_kg_m3: u32, diameter_mm: u32) -> u32 {
    let d = i128::from(diameter_mm);
    let num = i128::from(density_kg_m3)
        .checked_mul(d * d * d)
        .and_then(|n| n.checked_mul(PI_NUM));
    let den = 6_000_000i128 * PI_DEN;
    match num {
        Some(n) => u32::try_from((n + den / 2) / den).unwrap_or(u32::MAX),
        None => u32::MAX,
    }
}

/// Takes an integer in `lo..=hi` as `u32`.
pub(crate) fn take_u32(t: &mut Table, key: &str, lo: u32, hi: u32) -> Result<u32, DataError> {
    let v = t.take_int_in(key, i64::from(lo), i64::from(hi))?;
    Ok(u32::try_from(v).unwrap_or(lo))
}

/// Checks that `id` is a non-empty lowercase identifier (`a-z`, `0-9`, `_`).
pub(crate) fn check_id(path: &str, id: &str) -> Result<(), DataError> {
    if !id.is_empty()
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
    {
        Ok(())
    } else {
        Err(DataError::Parse(format!("{path}: bad id {id:?}")))
    }
}

/// Reads the `schema` key and checks it equals `want`.
pub(crate) fn check_schema(root: &mut Table, want: &str) -> Result<(), DataError> {
    let got = root.take_str("schema")?;
    if got == want {
        Ok(())
    } else {
        Err(DataError::Parse(format!(
            "schema must be \"{want}\", found \"{got}\""
        )))
    }
}

fn load_constants(root: &mut Table) -> Result<Constants, DataError> {
    let mut t = root.take_table("constants")?;
    let air_density_g_m3 = take_u32(&mut t, "air_density_g_m3", 1, 5000)?;
    let iron_density_kg_m3 = take_u32(&mut t, "iron_density_kg_m3", 1000, 20000)?;
    let gravity_um_s2 = t.take_int_in("gravity_um_s2", 1_000_000, 30_000_000)?;
    let provenance = t.take_provenance()?;
    t.finish()?;
    Ok(Constants {
        air_density_g_m3,
        iron_density_kg_m3,
        gravity: fx_ratio(gravity_um_s2, 1_000_000),
        provenance,
    })
}

fn load_burst(row: &mut Table, c: &Constants) -> Result<Burst, DataError> {
    let mut t = row.take_table("burst")?;
    let count = take_u32(&mut t, "count", 1, 255)?;
    let pellet_diameter_mm = take_u32(&mut t, "pellet_diameter_mm", 1, 500)?;
    let pellet_mass_g = match t.take_int_opt("pellet_mass_g")? {
        Some(v) if (1..=100_000).contains(&v) => u32::try_from(v).unwrap_or(1),
        Some(v) => {
            return Err(DataError::Range(format!(
                "{}.pellet_mass_g = {v} outside 1..=100000",
                t.path()
            )))
        }
        None => sphere_mass_g(c.iron_density_kg_m3, pellet_diameter_mm).max(1),
    };
    let pellet_cd_pm = take_u32(&mut t, "pellet_cd_pm", 0, 2000)?;
    let shape = match t.take_str("shape")?.as_str() {
        "cone" => BurstShape::Cone,
        "sphere" => BurstShape::Sphere,
        other => {
            return Err(DataError::Parse(format!(
                "{}.shape: unknown shape {other:?}",
                t.path()
            )))
        }
    };
    let spread_sigma_urad = take_u32(&mut t, "spread_sigma_urad", 0, 1_000_000)?;
    let pellet_speed_pm = take_u32(&mut t, "pellet_speed_pm", 1, 2000)?;
    let fuse_sigma_substeps = take_u32(&mut t, "fuse_sigma_substeps", 0, 1000)?;
    let dud_ppm = take_u32(&mut t, "dud_ppm", 0, 1_000_000)?;
    t.finish()?;
    Ok(Burst {
        count,
        pellet_mass_g,
        pellet_diameter_mm,
        pellet_cd_pm,
        shape,
        spread_sigma_urad,
        pellet_speed_pm,
        fuse_sigma_substeps,
        dud_ppm,
        pellet_drag_k: drag_k(
            c.air_density_g_m3,
            pellet_cd_pm,
            pellet_diameter_mm,
            pellet_mass_g,
        ),
    })
}

fn load_row(mut row: Table, c: &Constants) -> Result<AmmoRow, DataError> {
    let id = row.take_str("id")?;
    check_id(row.path(), &id)?;
    let kind = match row.take_str("kind")?.as_str() {
        "ball" => AmmoKind::Ball,
        "conical" => AmmoKind::Conical,
        "shot" => AmmoKind::Shot,
        "shell" => AmmoKind::Shell,
        "case" => AmmoKind::Case,
        "canister" => AmmoKind::Canister,
        other => {
            return Err(DataError::Parse(format!(
                "{}.kind: unknown kind {other:?}",
                row.path()
            )))
        }
    };
    let mass_g = take_u32(&mut row, "mass_g", 1, 100_000)?;
    let diameter_mm = take_u32(&mut row, "diameter_mm", 1, 500)?;
    let cd_pm = take_u32(&mut row, "cd_pm", 0, 2000)?;
    let muzzle_mm_s = take_u32(&mut row, "muzzle_mm_s", 1, 2_000_000)?;
    // A key that does not belong to the kind is left in the row and refused by `finish`.
    let restitution_pm = match kind {
        AmmoKind::Shot => Some(take_u32(&mut row, "restitution_pm", 0, 1000)?),
        _ => None,
    };
    let burst = match kind {
        AmmoKind::Shell | AmmoKind::Case | AmmoKind::Canister => Some(load_burst(&mut row, c)?),
        _ => None,
    };
    let provenance = row.take_provenance()?;
    row.finish()?;
    Ok(AmmoRow {
        drag_k: drag_k(c.air_density_g_m3, cd_pm, diameter_mm, mass_g),
        id,
        kind,
        mass_g,
        diameter_mm,
        cd_pm,
        muzzle_mm_s,
        restitution_pm,
        burst,
        provenance,
    })
}

impl AmmoTable {
    /// Parses and validates `data/rules/ammo.toml` text.
    ///
    /// Errors: float, unknown key, provenance, range and structure problems as [`DataError`];
    /// duplicate ids and more than 255 rows are refused.
    pub fn from_toml_str(text: &str) -> Result<AmmoTable, DataError> {
        let mut root = parse_table(text)?;
        check_schema(&mut root, "ammo/1")?;
        let constants = load_constants(&mut root)?;
        let mut rows: Vec<AmmoRow> = Vec::new();
        for row in root.take_rows("ammo")? {
            let r = load_row(row, &constants)?;
            if rows.iter().any(|x| x.id == r.id) {
                return Err(DataError::Parse(format!("duplicate ammo id {}", r.id)));
            }
            rows.push(r);
        }
        root.finish()?;
        if rows.is_empty() || rows.len() > 255 {
            return Err(DataError::Range(format!(
                "ammo needs 1..=255 rows, found {}",
                rows.len()
            )));
        }
        Ok(AmmoTable { constants, rows })
    }

    /// The shared constants.
    pub fn constants(&self) -> &Constants {
        &self.constants
    }

    /// All rows in file order.
    pub fn rows(&self) -> &[AmmoRow] {
        &self.rows
    }

    /// The row with this id.
    pub fn get(&self, id: &str) -> Option<&AmmoRow> {
        self.rows.iter().find(|r| r.id == id)
    }

    /// The index of the row with this id.
    pub fn index_of(&self, id: &str) -> Option<usize> {
        self.rows.iter().position(|r| r.id == id)
    }
}
