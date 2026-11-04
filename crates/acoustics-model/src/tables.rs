//! The acoustics rules data (`data/rules/acoustics.toml`, SPEC-012 section 3).

use battle_rules::{parse_table, DataError, Provenance, Table};

const SCHEMA: &str = "acoustics/1";

/// Number of source classes: musketry, artillery, burst, voice.
pub const CLASS_COUNT: usize = 4;

/// Scalar constants of the model, flattened from `[globals.air]`, `[globals.propagation]` and
/// `[globals.listening]`. Units are in the key names.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(missing_docs)]
pub struct Globals {
    pub c0_mm_s: i64,
    pub c_per_k_mm_s: i64,
    pub shear_ppm: i64,
    pub g_min: i64,
    pub shadow_sat_g: i64,
    pub duct_loss_cdb_per_km: i64,
    pub barrier_cap_cdb: i64,
    pub profile_step_mm: i64,
    pub max_samples: u32,
    pub max_range_mm: i64,
    pub cluster_cell_mm: i64,
    pub hyst_cdb: i32,
    pub drop_below_cdb: i32,
    pub class_faint_cdb: i32,
    pub class_clear_cdb: i32,
    pub class_loud_cdb: i32,
    pub camera_window_ticks: u32,
    pub hq_window_ticks: u32,
}

/// One frequency band (index 0 low, 1 mid, 2 high).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BandRow {
    /// Centre frequency in hertz.
    pub f_hz: i64,
    /// Air absorption, cdB per kilometre.
    pub abs_cdb_per_km: i64,
    /// Ambient level, cdB.
    pub ambient_cdb: i32,
    /// Share of the foliage loss that applies to this band, permille.
    pub foliage_pm: i64,
    /// Growth of the shadow loss beyond the horizon, cdB per kilometre.
    pub shadow_slope_cdb_per_km: i64,
    /// Largest shadow loss, cdB.
    pub shadow_cap_cdb: i64,
    /// Where the numbers come from.
    pub provenance: Provenance,
}

/// One sound source row; the row index equals `id`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceRow {
    /// Source id, equal to the row position.
    pub id: u8,
    /// Lower-case name.
    pub name: String,
    /// Class: 0 musketry, 1 artillery, 2 burst, 3 voice.
    pub class: u8,
    /// Sound exposure level at 1 m in the low, mid and high band, cdB.
    pub level_cdb: [i32; 3],
    /// Added to the score when choosing sources for a packet, cdB (0 when absent).
    pub priority_cdb: i32,
    /// Where the numbers come from.
    pub provenance: Provenance,
}

/// Foliage of one terrain class.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroundRow {
    /// Terrain class id (SPEC-004 section 5).
    pub class_id: u8,
    /// Height of the canopy in mm; a path below it passes through foliage.
    pub canopy_mm: i64,
    /// Foliage loss per 100 m of path, cdB.
    pub foliage_cdb_per_100m: i64,
    /// Where the numbers come from.
    pub provenance: Provenance,
}

/// A named weather state for scenes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WeatherPreset {
    /// Lower-case id, unique.
    pub id: String,
    /// Temperature, tenths of a degree C.
    pub temp_dc: i32,
    /// Wind x component, mm/s.
    pub wind_x_mm_s: i32,
    /// Wind y component, mm/s.
    pub wind_y_mm_s: i32,
    /// Temperature lapse, millikelvin per metre.
    pub lapse_mk_per_m: i32,
    /// Where the numbers come from.
    pub provenance: Provenance,
}

/// How many sources one packet may carry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Budget {
    /// Most sources in total.
    pub max_total: u32,
    /// Most sources per class (musketry, artillery, burst, voice).
    pub class_caps: [u32; CLASS_COUNT],
    /// Where the numbers come from.
    pub provenance: Provenance,
}

/// The loaded acoustics data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcousticTables {
    globals: Globals,
    air: Provenance,
    propagation: Provenance,
    listening: Provenance,
    bands: [BandRow; 3],
    sources: Vec<SourceRow>,
    ground: Vec<GroundRow>,
    weather: Vec<WeatherPreset>,
    budget: Budget,
}

/// Takes an integer in `lo..=hi` and converts it to the field type.
fn int<T: TryFrom<i64>>(t: &mut Table, key: &str, lo: i64, hi: i64) -> Result<T, DataError> {
    let v = t.take_int_in(key, lo, hi)?;
    T::try_from(v).map_err(|_| DataError::Range(format!("{}.{key} = {v}", t.path())))
}

fn name(t: &mut Table, key: &str) -> Result<String, DataError> {
    let s = t.take_str(key)?;
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_lowercase() || b == b'_') {
        return Err(DataError::Parse(format!(
            "{}.{key} must be lower-case letters and underscores",
            t.path()
        )));
    }
    Ok(s)
}

fn range_err(t: &Table, msg: &str) -> DataError {
    DataError::Range(format!("{}: {msg}", t.path()))
}

fn load_band(mut t: Table) -> Result<BandRow, DataError> {
    let row = BandRow {
        f_hz: int(&mut t, "f_hz", 1, 100_000)?,
        abs_cdb_per_km: int(&mut t, "abs_cdb_per_km", 0, 100_000)?,
        ambient_cdb: int(&mut t, "ambient_cdb", -20_000, 20_000)?,
        foliage_pm: int(&mut t, "foliage_pm", 0, 100_000)?,
        shadow_slope_cdb_per_km: int(&mut t, "shadow_slope_cdb_per_km", 0, 100_000)?,
        shadow_cap_cdb: int(&mut t, "shadow_cap_cdb", 0, 20_000)?,
        provenance: t.take_provenance()?,
    };
    t.finish()?;
    Ok(row)
}

fn load_source(mut t: Table) -> Result<SourceRow, DataError> {
    let id = int(&mut t, "id", 0, 255)?;
    let name = name(&mut t, "name")?;
    let class = int(&mut t, "class", 0, CLASS_COUNT as i64 - 1)?;
    let level_cdb = [
        int(&mut t, "level_low_cdb", -20_000, 20_000)?,
        int(&mut t, "level_mid_cdb", -20_000, 20_000)?,
        int(&mut t, "level_high_cdb", -20_000, 20_000)?,
    ];
    let priority_cdb = match t.take_int_opt("priority_cdb")? {
        None => 0,
        Some(v) if (0..=20_000).contains(&v) => v as i32,
        Some(v) => return Err(range_err(&t, &format!("priority_cdb = {v}"))),
    };
    let provenance = t.take_provenance()?;
    t.finish()?;
    Ok(SourceRow {
        id,
        name,
        class,
        level_cdb,
        priority_cdb,
        provenance,
    })
}

fn load_ground(mut t: Table) -> Result<GroundRow, DataError> {
    let row = GroundRow {
        class_id: int(&mut t, "class_id", 0, 255)?,
        canopy_mm: int(&mut t, "canopy_mm", 0, 100_000)?,
        foliage_cdb_per_100m: int(&mut t, "foliage_cdb_per_100m", 0, 100_000)?,
        provenance: t.take_provenance()?,
    };
    t.finish()?;
    Ok(row)
}

fn load_weather(mut t: Table) -> Result<WeatherPreset, DataError> {
    let row = WeatherPreset {
        id: name(&mut t, "id")?,
        temp_dc: int(&mut t, "temp_dc", -600, 600)?,
        wind_x_mm_s: int(&mut t, "wind_x_mm_s", -100_000, 100_000)?,
        wind_y_mm_s: int(&mut t, "wind_y_mm_s", -100_000, 100_000)?,
        lapse_mk_per_m: int(&mut t, "lapse_mk_per_m", -500, 500)?,
        provenance: t.take_provenance()?,
    };
    t.finish()?;
    Ok(row)
}

fn load_budget(mut t: Table) -> Result<Budget, DataError> {
    let row = Budget {
        max_total: int(&mut t, "max_total", 1, 1000)?,
        class_caps: [
            int(&mut t, "cap_musketry", 0, 1000)?,
            int(&mut t, "cap_artillery", 0, 1000)?,
            int(&mut t, "cap_burst", 0, 1000)?,
            int(&mut t, "cap_voice", 0, 1000)?,
        ],
        provenance: t.take_provenance()?,
    };
    t.finish()?;
    Ok(row)
}

/// Reads `[globals.air]`, `[globals.propagation]` and `[globals.listening]`.
fn load_globals(mut g: Table) -> Result<(Globals, [Provenance; 3]), DataError> {
    let mut a = g.take_table("air")?;
    let (c0_mm_s, c_per_k_mm_s) = (
        int(&mut a, "c0_mm_s", 100_000, 500_000)?,
        int(&mut a, "c_per_k_mm_s", 0, 5000)?,
    );
    let air = a.take_provenance()?;
    a.finish()?;

    let mut p = g.take_table("propagation")?;
    let (shear_ppm, g_min, shadow_sat_g) = (
        int(&mut p, "shear_ppm", 0, 1_000_000)?,
        int(&mut p, "g_min", 1, 1000)?,
        int(&mut p, "shadow_sat_g", 1, 100_000)?,
    );
    let (duct_loss_cdb_per_km, barrier_cap_cdb) = (
        int(&mut p, "duct_loss_cdb_per_km", 0, 100_000)?,
        int(&mut p, "barrier_cap_cdb", 0, 20_000)?,
    );
    let (profile_step_mm, max_samples, max_range_mm) = (
        int(&mut p, "profile_step_mm", 1, 10_000_000)?,
        int(&mut p, "max_samples", 2, 4096)?,
        int(&mut p, "max_range_mm", 1000, 10_000_000_000)?,
    );
    let propagation = p.take_provenance()?;
    p.finish()?;

    let mut l = g.take_table("listening")?;
    let cluster_cell_mm = int(&mut l, "cluster_cell_mm", 1, 10_000_000)?;
    let (hyst_cdb, drop_below_cdb) = (
        int(&mut l, "hyst_cdb", 0, 10_000)?,
        int(&mut l, "drop_below_cdb", -20_000, 20_000)?,
    );
    let (class_faint_cdb, class_clear_cdb, class_loud_cdb) = (
        int(&mut l, "class_faint_cdb", -20_000, 20_000)?,
        int(&mut l, "class_clear_cdb", -20_000, 20_000)?,
        int(&mut l, "class_loud_cdb", -20_000, 20_000)?,
    );
    let (camera_window_ticks, hq_window_ticks) = (
        int(&mut l, "camera_window_ticks", 1, 1000)?,
        int(&mut l, "hq_window_ticks", 1, 1000)?,
    );
    if !(class_faint_cdb < class_clear_cdb && class_clear_cdb < class_loud_cdb) {
        return Err(range_err(&l, "class thresholds must ascend"));
    }
    let listening = l.take_provenance()?;
    l.finish()?;
    g.finish()?;

    let globals = Globals {
        c0_mm_s,
        c_per_k_mm_s,
        shear_ppm,
        g_min,
        shadow_sat_g,
        duct_loss_cdb_per_km,
        barrier_cap_cdb,
        profile_step_mm,
        max_samples,
        max_range_mm,
        cluster_cell_mm,
        hyst_cdb,
        drop_below_cdb,
        class_faint_cdb,
        class_clear_cdb,
        class_loud_cdb,
        camera_window_ticks,
        hq_window_ticks,
    };
    Ok((globals, [air, propagation, listening]))
}

impl AcousticTables {
    /// Parses and validates `acoustics.toml` text.
    ///
    /// Errors: the shared reader's [`DataError::Float`], [`DataError::UnknownKey`] and
    /// [`DataError::Provenance`]; [`DataError::Parse`] for a wrong schema tag, a missing key, a
    /// bad name, a wrong row count or a duplicate weather id; [`DataError::Range`] for
    /// out-of-range numbers, bands not ascending in frequency, source ids that differ from
    /// their position, ground classes not strictly ascending or thresholds out of order.
    pub fn from_toml_str(text: &str) -> Result<AcousticTables, DataError> {
        let mut root = parse_table(text)?;
        let schema = root.take_str("schema")?;
        if schema != SCHEMA {
            return Err(DataError::Parse(format!(
                "schema must be {SCHEMA:?}, found {schema:?}"
            )));
        }
        let (globals, [air, propagation, listening]) = load_globals(root.take_table("globals")?)?;

        let bands: Vec<BandRow> = rows(&mut root, "band", load_band)?;
        let bands: [BandRow; 3] = bands
            .try_into()
            .map_err(|_| DataError::Parse("band: exactly 3 rows are required".to_owned()))?;
        if !(bands[0].f_hz < bands[1].f_hz && bands[1].f_hz < bands[2].f_hz) {
            return Err(DataError::Range("band: f_hz must ascend".to_owned()));
        }

        let sources = rows(&mut root, "source", load_source)?;
        if sources.len() != 9 {
            return Err(DataError::Parse(
                "source: exactly 9 rows are required".to_owned(),
            ));
        }
        if let Some((i, s)) = sources
            .iter()
            .enumerate()
            .find(|(i, s)| usize::from(s.id) != *i)
        {
            return Err(DataError::Range(format!(
                "source[{i}].id = {}, expected {i}",
                s.id
            )));
        }

        let ground = rows(&mut root, "ground", load_ground)?;
        if ground.windows(2).any(|w| w[1].class_id <= w[0].class_id) {
            return Err(DataError::Range(
                "ground: class_id must strictly ascend".to_owned(),
            ));
        }

        let weather = rows(&mut root, "weather", load_weather)?;
        if let Some(i) =
            (1..weather.len()).find(|&i| weather[..i].iter().any(|w| w.id == weather[i].id))
        {
            return Err(DataError::Parse(format!(
                "duplicate weather id {}",
                weather[i].id
            )));
        }

        let mut budget = rows(&mut root, "budget", load_budget)?;
        if budget.len() != 1 {
            return Err(DataError::Parse(
                "budget: exactly 1 row is required".to_owned(),
            ));
        }
        root.finish()?;
        Ok(AcousticTables {
            globals,
            air,
            propagation,
            listening,
            bands,
            sources,
            ground,
            weather,
            budget: budget.remove(0),
        })
    }

    /// The scalar constants.
    pub fn globals(&self) -> &Globals {
        &self.globals
    }

    /// Provenance of the sound-speed law (`c0_mm_s`, `c_per_k_mm_s`).
    pub fn air_provenance(&self) -> &Provenance {
        &self.air
    }

    /// Provenance of the propagation constants.
    pub fn propagation_provenance(&self) -> &Provenance {
        &self.propagation
    }

    /// Provenance of the listening constants (windows, thresholds, hysteresis).
    pub fn listening_provenance(&self) -> &Provenance {
        &self.listening
    }

    /// The three bands, low to high.
    pub fn bands(&self) -> &[BandRow; 3] {
        &self.bands
    }

    /// The nine sources, indexed by id.
    pub fn sources(&self) -> &[SourceRow] {
        &self.sources
    }

    /// The ground row for a terrain class; a class without a row has no foliage.
    pub fn ground(&self, class_id: u8) -> Option<&GroundRow> {
        self.ground
            .binary_search_by_key(&class_id, |g| g.class_id)
            .ok()
            .map(|i| &self.ground[i])
    }

    /// The weather preset with this id.
    pub fn weather(&self, id: &str) -> Option<&WeatherPreset> {
        self.weather.iter().find(|w| w.id == id)
    }

    /// All weather presets in file order.
    pub fn weather_presets(&self) -> &[WeatherPreset] {
        &self.weather
    }

    /// The packet budget.
    pub fn budget(&self) -> &Budget {
        &self.budget
    }
}

fn rows<T>(
    root: &mut Table,
    key: &str,
    load: fn(Table) -> Result<T, DataError>,
) -> Result<Vec<T>, DataError> {
    root.take_rows(key)?.into_iter().map(load).collect()
}
