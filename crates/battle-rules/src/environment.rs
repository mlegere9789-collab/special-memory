//! Environment data (SPEC-009 sections 3 and 5): the `environment.toml` constants, the weather
//! timeline with its linear interpolation, and the optical-depth table.
//!
//! Everything here is integer data read through the shared loader; nothing touches the
//! simulation state. [`EnvTable`] holds the smoke, sight, heat and rain constants and the smoke
//! retention per terrain class; [`WeatherTable`] holds the timeline and answers
//! [`WeatherNow`] at a battle tick; [`exp_table`] turns an optical depth into a transmission.

use crate::{
    fx_ratio, parse_table, DataError, ElevationGrid, Provenance, Table, TerrainTable, Weather,
    TICK_HZ,
};
use core_math::Vec2Fx;

const ENV_SCHEMA: &str = "environment/1";
const WEATHER_SCHEMA: &str = "weather/1";
const TICKS_PER_MINUTE: i128 = 1200;
/// Largest wind speed a weather key may hold, in mm per second (SPEC-009 section 3.1).
pub const WIND_MAX_MM_S: i64 = 20_000;
/// Smoke leaving a cell in one update may never exceed the whole cell, in ppm.
const FLOW_LIMIT_PPM: u64 = 1_000_000;
/// The square root of 2 rounded up, in permille: bounds `|wx| + |wy|` by the wind speed.
const SQRT2_UP_PM: u64 = 1415;

fn uint(t: &mut Table, key: &str, lo: u32, hi: u32) -> Result<u32, DataError> {
    let v = t.take_int_in(key, i64::from(lo), i64::from(hi))?;
    u32::try_from(v).map_err(|_| DataError::Range(format!("{}.{key} = {v}", t.path())))
}

fn int32(t: &mut Table, key: &str, lo: i32, hi: i32) -> Result<i32, DataError> {
    let v = t.take_int_in(key, i64::from(lo), i64::from(hi))?;
    i32::try_from(v).map_err(|_| DataError::Range(format!("{}.{key} = {v}", t.path())))
}

fn schema(root: &mut Table, want: &str) -> Result<(), DataError> {
    let found = root.take_str("schema")?;
    if found != want {
        return Err(DataError::Parse(format!(
            "schema must be {want:?}, found {found:?}"
        )));
    }
    Ok(())
}

/// Smoke grid constants (`[smoke]`). One density unit is 1 mg of smoke in a cell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SmokeParams {
    /// Ticks between grid updates.
    pub period_ticks: u32,
    /// Cell edge in mm; equals the terrain cell.
    pub cell_mm: u32,
    /// Height of the air column in mm.
    pub height_mm: u32,
    /// Smoke mass per powder mass, permille.
    pub yield_pm: u32,
    /// Share of the yield a misfire makes, permille.
    pub misfire_pm: u32,
    /// Share sent to each of four neighbours per update, ppm.
    pub diffuse_ppm: u32,
    /// Ceiling decay per update, ppm.
    pub decay_ppm: u32,
    /// Extra decay at full rain, permille of the base.
    pub rain_decay_gain_pm: u32,
    /// Density a cell may hold.
    pub cell_cap: u32,
    /// Extinction constant of the optical depth.
    pub ext_k: u32,
    /// Density that counts as full concentration.
    pub ref_units: u32,
    /// Where the numbers come from.
    pub provenance: Provenance,
}

/// Line-of-sight, gate, peek and observer constants (`[sight]`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SightParams {
    /// Ticks per peek and observer period.
    pub period_ticks: u32,
    /// Sample step of infantry queries, mm.
    pub step_mm: u32,
    /// Sample step of observer queries, mm.
    pub far_step_mm: u32,
    /// Clearance below which a ray is blocked, mm.
    pub clear_margin_mm: u32,
    /// Transmission below which a target is not visible, permille.
    pub min_transmit_pm: u32,
    /// Extra targets a blind shooter tries.
    pub retries: u32,
    /// Range-error growth with smoke, permille.
    pub range_smoke_gain_pm: u32,
    /// Forward step of a blind soldier per period, mm.
    pub peek_step_mm: u32,
    /// Largest peek offset, mm.
    pub peek_max_mm: u32,
    /// Step back per quiet period, mm.
    pub peek_back_mm: u32,
    /// Share of blind men that sets the regiment's blocked view, permille.
    pub blind_report_pm: u32,
    /// Soldiers an observer tests per regiment.
    pub observer_samples: u32,
    /// Where the numbers come from.
    pub provenance: Provenance,
}

/// Apparent-temperature parameters (`[heat]`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeatParams {
    /// Temperature above which humidity adds heat, tenths of a degree F.
    pub floor_f10: i32,
    /// Humidity below which none is added, permille.
    pub humid_floor_pm: u32,
    /// Divisor of the humidity term.
    pub div: u32,
    /// Where the numbers come from.
    pub provenance: Provenance,
}

/// One point of the heat curve.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeatPoint {
    /// Apparent temperature, tenths of a degree F; strictly increasing along the curve.
    pub apparent_f10: i32,
    /// Fatigue drain multiplier, permille; non-decreasing along the curve.
    pub drain_pm: u32,
    /// Rest recovery multiplier, permille; non-increasing along the curve.
    pub rest_pm: u32,
    /// Where the numbers come from.
    pub provenance: Provenance,
}

/// Rain, powder and ground rates (`[rain]`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RainParams {
    /// Powder damp rise per period at full rain, ppm.
    pub powder_rise_ppm: u32,
    /// Powder damp fall per dry period, ppm.
    pub powder_fall_ppm: u32,
    /// Ground wetness rise per period at full rain, ppm.
    pub ground_rise_ppm: u32,
    /// Ground wetness fall per dry period, ppm.
    pub ground_fall_ppm: u32,
    /// Misfire factor added at full damp, permille.
    pub wet_extra_pm: u32,
    /// Ground factor at full wetness, permille.
    pub ground_min_pm: u32,
    /// Where the numbers come from.
    pub provenance: Provenance,
}

/// Smoke retention of one terrain class.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RetainRow {
    /// Terrain class id.
    pub class: u8,
    /// Share of a cell's smoke that stays put each update, permille.
    pub retain_pm: u32,
    /// Where the number comes from.
    pub provenance: Provenance,
}

/// The loaded `environment.toml`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnvTable {
    /// Smoke grid constants.
    pub smoke: SmokeParams,
    /// Sight constants.
    pub sight: SightParams,
    /// Apparent-temperature parameters.
    pub heat: HeatParams,
    /// Heat curve, ascending in apparent temperature.
    pub heat_curve: Vec<HeatPoint>,
    /// Rain rates.
    pub rain: RainParams,
    retain: Vec<RetainRow>,
}

fn load_retain(rows: Vec<Table>, terrain: &TerrainTable) -> Result<Vec<RetainRow>, DataError> {
    let mut out: Vec<RetainRow> = Vec::new();
    for mut t in rows {
        let class = u8::try_from(t.take_int_in("class", 0, 255)?).unwrap_or(0);
        let retain_pm = uint(&mut t, "retain_pm", 0, 1000)?;
        let provenance = t.take_provenance()?;
        t.finish()?;
        if terrain.class(class).is_none() {
            return Err(DataError::Range(format!(
                "retain class {class} is not in terrain.toml"
            )));
        }
        if out.last().is_some_and(|p| class <= p.class) {
            return Err(DataError::Range(format!(
                "retain class {class} is not strictly ascending"
            )));
        }
        out.push(RetainRow {
            class,
            retain_pm,
            provenance,
        });
    }
    if let Some(c) = terrain
        .classes()
        .iter()
        .find(|c| out.iter().all(|r| r.class != c.id))
    {
        return Err(DataError::Range(format!(
            "terrain class {} has no retain row",
            c.id
        )));
    }
    Ok(out)
}

fn load_curve(rows: Vec<Table>) -> Result<Vec<HeatPoint>, DataError> {
    let mut out: Vec<HeatPoint> = Vec::new();
    for mut t in rows {
        let apparent_f10 = int32(&mut t, "apparent_f10", -1000, 3000)?;
        let drain_pm = uint(&mut t, "drain_pm", 0, 10_000)?;
        let rest_pm = uint(&mut t, "rest_pm", 0, 10_000)?;
        let provenance = t.take_provenance()?;
        t.finish()?;
        if let Some(p) = out.last() {
            let order = [
                (
                    apparent_f10 <= p.apparent_f10,
                    "apparent_f10 must be strictly increasing",
                ),
                (drain_pm < p.drain_pm, "drain_pm must be non-decreasing"),
                (rest_pm > p.rest_pm, "rest_pm must be non-increasing"),
            ];
            if let Some((_, m)) = order.iter().find(|(bad, _)| *bad) {
                return Err(DataError::Range(format!("heat_curve: {m}")));
            }
        }
        out.push(HeatPoint {
            apparent_f10,
            drain_pm,
            rest_pm,
            provenance,
        });
    }
    if out.len() < 2 {
        return Err(DataError::Parse(
            "heat_curve: needs at least two rows".to_owned(),
        ));
    }
    Ok(out)
}

fn load_smoke(mut t: Table) -> Result<SmokeParams, DataError> {
    let s = SmokeParams {
        period_ticks: uint(&mut t, "period_ticks", 1, 1200)?,
        cell_mm: uint(&mut t, "cell_mm", 1, 1_000_000)?,
        height_mm: uint(&mut t, "height_mm", 1, 1_000_000)?,
        yield_pm: uint(&mut t, "yield_pm", 0, 10_000)?,
        misfire_pm: uint(&mut t, "misfire_pm", 0, 1000)?,
        diffuse_ppm: uint(&mut t, "diffuse_ppm", 0, 250_000)?,
        decay_ppm: uint(&mut t, "decay_ppm", 0, 1_000_000)?,
        rain_decay_gain_pm: uint(&mut t, "rain_decay_gain_pm", 0, 10_000)?,
        cell_cap: uint(&mut t, "cell_cap", 1, u32::MAX)?,
        ext_k: uint(&mut t, "ext_k", 0, 10_000)?,
        ref_units: uint(&mut t, "ref_units", 1, u32::MAX)?,
        provenance: t.take_provenance()?,
    };
    t.finish()?;
    // Wind cells per update at the speed limit, and the most the two axes can add up to.
    let cells = (WIND_MAX_MM_S as u64) * u64::from(s.period_ticks) * 1_000_000
        / (u64::from(TICK_HZ) * u64::from(s.cell_mm));
    let flow = 4 * u64::from(s.diffuse_ppm) + (cells * SQRT2_UP_PM).div_ceil(1000);
    if flow > FLOW_LIMIT_PPM {
        return Err(DataError::Range(format!(
            "smoke flow sum {flow} ppm exceeds {FLOW_LIMIT_PPM}"
        )));
    }
    Ok(s)
}

fn load_sight(mut t: Table) -> Result<SightParams, DataError> {
    let s = SightParams {
        period_ticks: uint(&mut t, "period_ticks", 1, 1200)?,
        step_mm: uint(&mut t, "step_mm", 1, 100_000)?,
        far_step_mm: uint(&mut t, "far_step_mm", 1, 100_000)?,
        clear_margin_mm: uint(&mut t, "clear_margin_mm", 0, 10_000)?,
        min_transmit_pm: uint(&mut t, "min_transmit_pm", 0, 1000)?,
        retries: uint(&mut t, "retries", 0, 64)?,
        range_smoke_gain_pm: uint(&mut t, "range_smoke_gain_pm", 0, 100_000)?,
        peek_step_mm: uint(&mut t, "peek_step_mm", 1, 60_000)?,
        peek_max_mm: uint(&mut t, "peek_max_mm", 0, 60_000)?,
        peek_back_mm: uint(&mut t, "peek_back_mm", 0, 60_000)?,
        blind_report_pm: uint(&mut t, "blind_report_pm", 0, 1000)?,
        observer_samples: uint(&mut t, "observer_samples", 1, 64)?,
        provenance: t.take_provenance()?,
    };
    t.finish()?;
    Ok(s)
}

fn load_heat(mut t: Table) -> Result<HeatParams, DataError> {
    let h = HeatParams {
        floor_f10: int32(&mut t, "floor_f10", -1000, 3000)?,
        humid_floor_pm: uint(&mut t, "humid_floor_pm", 0, 1000)?,
        div: uint(&mut t, "div", 1, 1_000_000)?,
        provenance: t.take_provenance()?,
    };
    t.finish()?;
    Ok(h)
}

fn load_rain(mut t: Table) -> Result<RainParams, DataError> {
    let r = RainParams {
        powder_rise_ppm: uint(&mut t, "powder_rise_ppm", 0, 1_000_000)?,
        powder_fall_ppm: uint(&mut t, "powder_fall_ppm", 0, 1_000_000)?,
        ground_rise_ppm: uint(&mut t, "ground_rise_ppm", 0, 1_000_000)?,
        ground_fall_ppm: uint(&mut t, "ground_fall_ppm", 0, 1_000_000)?,
        wet_extra_pm: uint(&mut t, "wet_extra_pm", 0, 100_000)?,
        ground_min_pm: uint(&mut t, "ground_min_pm", 0, 1000)?,
        provenance: t.take_provenance()?,
    };
    t.finish()?;
    Ok(r)
}

impl EnvTable {
    /// Parses and validates `environment.toml` text against the loaded terrain classes.
    ///
    /// Errors: the shared reader's [`DataError::Float`], [`DataError::UnknownKey`] and
    /// [`DataError::Provenance`]; [`DataError::Parse`] for a wrong schema tag, a missing key or
    /// a short heat curve; [`DataError::Range`] for out-of-range numbers, a heat curve that is
    /// not strictly increasing in `apparent_f10`, non-decreasing in `drain_pm` and
    /// non-increasing in `rest_pm`, a `retain` class that is not in `terrain` or not strictly
    /// ascending, a terrain class without a retain row, and a smoke flow sum above 10^6 ppm
    /// (`4 * diffuse_ppm` plus the largest wind share, `|wx| + |wy|` bounded by the 20000 mm/s
    /// wind limit).
    pub fn from_toml_str(text: &str, terrain: &TerrainTable) -> Result<EnvTable, DataError> {
        let mut root = parse_table(text)?;
        schema(&mut root, ENV_SCHEMA)?;
        let retain = load_retain(root.take_rows("retain")?, terrain)?;
        let heat_curve = load_curve(root.take_rows("heat_curve")?)?;
        let smoke = load_smoke(root.take_table("smoke")?)?;
        let sight = load_sight(root.take_table("sight")?)?;
        let heat = load_heat(root.take_table("heat")?)?;
        let rain = load_rain(root.take_table("rain")?)?;
        root.finish()?;
        Ok(EnvTable {
            smoke,
            sight,
            heat,
            heat_curve,
            rain,
            retain,
        })
    }

    /// Smoke retention rows, ascending by terrain class.
    pub fn retain(&self) -> &[RetainRow] {
        &self.retain
    }

    /// Smoke retention of a terrain class in permille (0 for a class without a row).
    pub fn retain_pm(&self, class: u8) -> u32 {
        self.retain
            .binary_search_by_key(&class, |r| r.class)
            .map_or(0, |i| self.retain[i].retain_pm)
    }

    /// Refuses a terrain grid whose cell is not the smoke cell ([`DataError::Range`]).
    pub fn check_cell_mm(&self, grid_cell_mm: i64) -> Result<(), DataError> {
        if grid_cell_mm != i64::from(self.smoke.cell_mm) {
            return Err(DataError::Range(format!(
                "smoke.cell_mm {} differs from the terrain cell {grid_cell_mm}",
                self.smoke.cell_mm
            )));
        }
        Ok(())
    }
}

/// One row of the weather timeline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WeatherKey {
    /// Minutes after 00:00 on 1863-07-01.
    pub t_min: i64,
    /// Temperature, tenths of a degree F.
    pub temp_f10: i32,
    /// Relative humidity, permille.
    pub humidity_pm: u16,
    /// Eastward air motion, mm per second.
    pub wind_x_mm_s: i32,
    /// Northward air motion, mm per second.
    pub wind_y_mm_s: i32,
    /// Rain intensity, permille.
    pub rain_pm: u16,
    /// Clear-air visibility, mm.
    pub clear_vis_mm: i32,
    /// Daylight, permille.
    pub light_pm: u16,
    /// Where the numbers come from.
    pub provenance: Provenance,
}

/// The weather at one battle tick: every key field interpolated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WeatherNow {
    /// Temperature, tenths of a degree F.
    pub temp_f10: i32,
    /// Relative humidity, permille.
    pub humidity_pm: i32,
    /// Eastward air motion, mm per second.
    pub wind_x_mm_s: i64,
    /// Northward air motion, mm per second.
    pub wind_y_mm_s: i64,
    /// Rain intensity, permille.
    pub rain_pm: i32,
    /// Clear-air visibility, mm.
    pub clear_vis_mm: i32,
    /// Daylight, permille.
    pub light_pm: i32,
}

impl WeatherKey {
    fn now(&self) -> WeatherNow {
        WeatherNow {
            temp_f10: self.temp_f10,
            humidity_pm: self.humidity_pm.into(),
            wind_x_mm_s: self.wind_x_mm_s.into(),
            wind_y_mm_s: self.wind_y_mm_s.into(),
            rain_pm: self.rain_pm.into(),
            clear_vis_mm: self.clear_vis_mm,
            light_pm: self.light_pm.into(),
        }
    }
}

/// The weather timeline of a scenario family (`weather.toml`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WeatherTable {
    start_minute: i64,
    keys: Vec<WeatherKey>,
}

fn load_key(mut t: Table) -> Result<WeatherKey, DataError> {
    let at = t.path().to_owned();
    let narrow = |v: u32| u16::try_from(v).unwrap_or(u16::MAX);
    let key = WeatherKey {
        t_min: t.take_int_in("t_min", -10_000_000, 10_000_000)?,
        temp_f10: int32(&mut t, "temp_f10", -1000, 1500)?,
        humidity_pm: narrow(uint(&mut t, "humidity_pm", 0, 1000)?),
        wind_x_mm_s: int32(&mut t, "wind_x_mm_s", -20_000, 20_000)?,
        wind_y_mm_s: int32(&mut t, "wind_y_mm_s", -20_000, 20_000)?,
        rain_pm: narrow(uint(&mut t, "rain_pm", 0, 1000)?),
        clear_vis_mm: int32(&mut t, "clear_vis_mm", 1, 100_000_000)?,
        light_pm: narrow(uint(&mut t, "light_pm", 0, 1000)?),
        provenance: t.take_provenance()?,
    };
    t.finish()?;
    let (wx, wy) = (i64::from(key.wind_x_mm_s), i64::from(key.wind_y_mm_s));
    if wx * wx + wy * wy > WIND_MAX_MM_S * WIND_MAX_MM_S {
        return Err(DataError::Range(format!(
            "{at}: wind speed exceeds {WIND_MAX_MM_S} mm/s"
        )));
    }
    Ok(key)
}

impl WeatherTable {
    /// Parses and validates a `weather.toml` sidecar.
    ///
    /// Errors: the shared reader's [`DataError::Float`], [`DataError::UnknownKey`] and
    /// [`DataError::Provenance`]; [`DataError::Parse`] for a wrong schema tag, a missing key or
    /// no keys; [`DataError::Range`] for out-of-range numbers, keys whose `t_min` is not
    /// strictly ascending or a wind speed above [`WIND_MAX_MM_S`].
    pub fn from_toml_str(text: &str) -> Result<WeatherTable, DataError> {
        let mut root = parse_table(text)?;
        schema(&mut root, WEATHER_SCHEMA)?;
        let start_minute = root.take_int_in("start_minute", -10_000_000, 10_000_000)?;
        let mut keys: Vec<WeatherKey> = Vec::new();
        for row in root.take_rows("key")? {
            let key = load_key(row)?;
            if keys.last().is_some_and(|p| key.t_min <= p.t_min) {
                return Err(DataError::Range(format!(
                    "key t_min {} is not strictly ascending",
                    key.t_min
                )));
            }
            keys.push(key);
        }
        root.finish()?;
        if keys.is_empty() {
            return Err(DataError::Parse("key: no rows".to_owned()));
        }
        Ok(WeatherTable { start_minute, keys })
    }

    /// Minutes after 00:00 on 1863-07-01 at battle tick 0.
    pub fn start_minute(&self) -> i64 {
        self.start_minute
    }

    /// The keys, ascending in `t_min`.
    pub fn keys(&self) -> &[WeatherKey] {
        &self.keys
    }

    /// The weather at battle tick `tick`: linear interpolation of every field between the
    /// bracketing keys, weighted in ticks (1200 per minute) and truncated toward zero; before
    /// the first key and after the last, that end key.
    pub fn at(&self, tick: u64) -> WeatherNow {
        let t = i128::from(self.start_minute) * TICKS_PER_MINUTE + i128::from(tick);
        let at_key = |k: &WeatherKey| i128::from(k.t_min) * TICKS_PER_MINUTE;
        let b = self.keys.partition_point(|k| at_key(k) <= t);
        if b == 0 {
            return self.keys[0].now();
        }
        if b == self.keys.len() {
            return self.keys[b - 1].now();
        }
        let (ka, kb) = (&self.keys[b - 1], &self.keys[b]);
        let (num, den) = (t - at_key(ka), at_key(kb) - at_key(ka));
        let lerp = |a: i64, b: i64| a + i64::try_from(i128::from(b - a) * num / den).unwrap_or(0);
        let (a, z) = (ka.now(), kb.now());
        let narrow = |v: i64| i32::try_from(v).unwrap_or(0);
        WeatherNow {
            temp_f10: narrow(lerp(a.temp_f10.into(), z.temp_f10.into())),
            humidity_pm: narrow(lerp(a.humidity_pm.into(), z.humidity_pm.into())),
            wind_x_mm_s: lerp(a.wind_x_mm_s, z.wind_x_mm_s),
            wind_y_mm_s: lerp(a.wind_y_mm_s, z.wind_y_mm_s),
            rain_pm: narrow(lerp(a.rain_pm.into(), z.rain_pm.into())),
            clear_vis_mm: narrow(lerp(a.clear_vis_mm.into(), z.clear_vis_mm.into())),
            light_pm: narrow(lerp(a.light_pm.into(), z.light_pm.into())),
        }
    }

    /// The weather as the simulation holds it: re-evaluated when `tick % period_ticks == 0`
    /// and held until the next period (a period of 0 is read as 1).
    pub fn at_period(&self, tick: u64, period_ticks: u64) -> WeatherNow {
        self.at(tick - tick % period_ticks.max(1))
    }
}

/// `round(10^6 * exp(-i/8))` for `i = 0 ..= 128`, generated by `qc/fixtures/ref_exp.py`
/// (Python `decimal`, 50 digits); the test T9-03 compares it with that script's output.
pub const EXP_TABLE: [u32; 129] = [
    1000000, 882497, 778801, 687289, 606531, 535261, 472367, 416862, 367879, 324652, 286505,
    252840, 223130, 196912, 173774, 153355, 135335, 119433, 105399, 93014, 82085, 72440, 63928,
    56416, 49787, 43937, 38774, 34218, 30197, 26649, 23518, 20754, 18316, 16163, 14264, 12588,
    11109, 9804, 8652, 7635, 6738, 5946, 5248, 4631, 4087, 3607, 3183, 2809, 2479, 2187, 1930,
    1704, 1503, 1327, 1171, 1033, 912, 805, 710, 627, 553, 488, 431, 380, 335, 296, 261, 231, 203,
    180, 158, 140, 123, 109, 96, 85, 75, 66, 58, 51, 45, 40, 35, 31, 28, 24, 21, 19, 17, 15, 13,
    11, 10, 9, 8, 7, 6, 5, 5, 4, 4, 3, 3, 3, 2, 2, 2, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0,
];

/// Optical-depth step of one table entry (tau = 1/8) in micro-units.
const EXP_STEP: u64 = 125_000;

/// Transmission in ppm of an optical depth `tau_micro` (tau times 10^6): linear interpolation
/// of [`EXP_TABLE`] at `tau_micro / 125000`; 0 from tau = 16.
pub fn exp_table(tau_micro: u64) -> u32 {
    let i = tau_micro / EXP_STEP;
    if i >= 128 {
        return 0;
    }
    let (hi, lo) = (EXP_TABLE[i as usize], EXP_TABLE[i as usize + 1]);
    let drop = u64::from(hi - lo) * (tau_micro % EXP_STEP) / EXP_STEP;
    hi - u32::try_from(drop).unwrap_or(0)
}

/// Apparent temperature in tenths of a degree F (SPEC-009 section 7):
/// `temp + max(0, temp - floor) * max(0, humidity - humid_floor) / div`, integer floor.
pub fn apparent_f10(heat: &HeatParams, temp_f10: i32, humidity_pm: i32) -> i32 {
    let over = (i64::from(temp_f10) - i64::from(heat.floor_f10)).max(0);
    let humid = (i64::from(humidity_pm) - i64::from(heat.humid_floor_pm)).max(0);
    let v = i64::from(temp_f10) + over * humid / i64::from(heat.div.max(1));
    i32::try_from(v).unwrap_or(i32::MAX)
}

impl EnvTable {
    /// `(drain_pm, rest_pm)` at an apparent temperature: linear interpolation between the two
    /// bracketing heat-curve rows, truncating toward zero like `fatigue_speed_pm`, and clamped at
    /// the ends of the curve.
    pub fn heat_multipliers(&self, apparent_f10: i32) -> (u32, u32) {
        let curve = &self.heat_curve;
        let upper = curve.partition_point(|p| p.apparent_f10 < apparent_f10);
        let pair = |p: &HeatPoint| (p.drain_pm, p.rest_pm);
        let (Some(p1), Some(p0)) = (curve.get(upper), upper.checked_sub(1).map(|i| &curve[i]))
        else {
            return curve.get(upper).or(curve.last()).map_or((1000, 1000), pair);
        };
        if p1.apparent_f10 == apparent_f10 {
            return pair(p1);
        }
        let run = i64::from(p1.apparent_f10) - i64::from(p0.apparent_f10);
        let at = i64::from(apparent_f10) - i64::from(p0.apparent_f10);
        let lerp = |a: u32, b: u32| {
            let v = i64::from(a) + (i64::from(b) - i64::from(a)) * at / run;
            u32::try_from(v).unwrap_or(0)
        };
        (lerp(p0.drain_pm, p1.drain_pm), lerp(p0.rest_pm, p1.rest_pm))
    }
}

/// The heat state of the battle: the apparent temperature and the fatigue multipliers read from
/// the heat curve (permille; 1000 is neutral).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeatNow {
    /// Apparent temperature, tenths of a degree F.
    pub apparent_f10: i32,
    /// Multiplier of fatigue drain, permille.
    pub drain_pm: u32,
    /// Multiplier of rest recovery, permille.
    pub rest_pm: u32,
}

/// What the morale layer reads of a regiment's surroundings, refreshed each sight period
/// (SPEC-009 section 8).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EnvInputs {
    /// Smoke concentration at the regiment's anchor, permille.
    pub smoke_pm: u16,
    /// Share of the regiment's soldiers that found no visible target this period, permille.
    pub blind_pm: u16,
    /// Enough soldiers stand at the peek limit and still cannot see.
    pub view_blocked: bool,
    /// Heat multiplier of fatigue drain, permille.
    pub heat_drain_pm: u16,
}

/// The environment layer of a battle (`BattleState.env`): the rules table, the weather timeline
/// and the state they drive each period: the weather now, powder damp, ground wetness and heat
/// (SPEC-009 sections 3.2 and 7). Damp and ground are ppm in `0..=1_000_000`.
#[derive(Clone, Debug)]
pub struct Environment {
    table: EnvTable,
    weather: WeatherTable,
    now: WeatherNow,
    damp: u32,
    ground: u32,
    heat: HeatNow,
    relief: ElevationGrid,
    inputs: Vec<EnvInputs>,
}

/// One rain step: `v + rain_pm * rise / 1000` while it rains, else `v - fall`, in `0..=10^6`.
fn rain_step(v: u32, rain_pm: i32, rise_ppm: u32, fall_ppm: u32) -> u32 {
    let v = i64::from(v);
    let next = if rain_pm > 0 {
        v + i64::from(rain_pm) * i64::from(rise_ppm) / 1000
    } else {
        v - i64::from(fall_ppm)
    };
    u32::try_from(next.clamp(0, 1_000_000)).unwrap_or(0)
}

impl Environment {
    /// An environment at battle tick `tick`: dry powder and ground, the weather and heat of the
    /// period that tick lies in.
    pub fn new(table: EnvTable, weather: WeatherTable, tick: u64) -> Environment {
        let period = u64::from(table.smoke.period_ticks);
        let now = weather.at_period(tick, period);
        let mut env = Environment {
            table,
            weather,
            now,
            damp: 0,
            ground: 0,
            heat: HeatNow {
                apparent_f10: 0,
                drain_pm: 1000,
                rest_pm: 1000,
            },
            relief: ElevationGrid::flat(),
            inputs: Vec::new(),
        };
        env.heat = env.heat_of(&now);
        env
    }

    fn heat_of(&self, now: &WeatherNow) -> HeatNow {
        let apparent_f10 = apparent_f10(&self.table.heat, now.temp_f10, now.humidity_pm);
        let (drain_pm, rest_pm) = self.table.heat_multipliers(apparent_f10);
        HeatNow {
            apparent_f10,
            drain_pm,
            rest_pm,
        }
    }

    /// Step TK-02: when `tick % smoke.period_ticks == 0`, re-reads the weather, takes one rain
    /// step and refreshes the heat; returns whether it did.
    pub fn refresh(&mut self, tick: u64) -> bool {
        if !tick.is_multiple_of(u64::from(self.table.smoke.period_ticks.max(1))) {
            return false;
        }
        self.now = self.weather.at(tick);
        let r = &self.table.rain;
        self.damp = rain_step(
            self.damp,
            self.now.rain_pm,
            r.powder_rise_ppm,
            r.powder_fall_ppm,
        );
        self.ground = rain_step(
            self.ground,
            self.now.rain_pm,
            r.ground_rise_ppm,
            r.ground_fall_ppm,
        );
        self.heat = self.heat_of(&self.now);
        true
    }

    /// The `Weather` the fire and flight rules read: wind from the timeline in m/s,
    /// `wet_pm = 1000 + damp * wet_extra_pm / 10^6`,
    /// `ground_pm = 1000 - ground * (1000 - ground_min_pm) / 10^6`.
    pub fn weather_state(&self) -> Weather {
        let r = &self.table.rain;
        let wet = 1000 + u64::from(self.damp) * u64::from(r.wet_extra_pm) / 1_000_000;
        let drop = u64::from(self.ground) * u64::from(1000 - r.ground_min_pm) / 1_000_000;
        Weather {
            wind: Vec2Fx::new(
                fx_ratio(self.now.wind_x_mm_s, 1000),
                fx_ratio(self.now.wind_y_mm_s, 1000),
            ),
            wet_pm: u32::try_from(wet).unwrap_or(u32::MAX),
            ground_pm: u32::try_from(1000 - drop).unwrap_or(0),
        }
    }

    /// The rules table.
    pub fn table(&self) -> &EnvTable {
        &self.table
    }

    /// The weather timeline.
    pub fn timeline(&self) -> &WeatherTable {
        &self.weather
    }

    /// The weather as of the last refresh.
    pub fn now(&self) -> &WeatherNow {
        &self.now
    }

    /// Powder dampness in ppm.
    pub fn powder_damp_ppm(&self) -> u32 {
        self.damp
    }

    /// Ground wetness in ppm.
    pub fn ground_wet_ppm(&self) -> u32 {
        self.ground
    }

    /// The heat state as of the last refresh.
    pub fn heat(&self) -> HeatNow {
        self.heat
    }

    /// The ground heights sight reads; flat until [`Environment::set_relief`].
    pub fn relief(&self) -> &ElevationGrid {
        &self.relief
    }

    /// Sets the ground heights (decision D-02: relief changes sight only).
    pub fn set_relief(&mut self, relief: ElevationGrid) {
        self.relief = relief;
    }

    /// The inputs of each regiment as of the last sight period, indexed by regiment id (shorter
    /// than the regiment list until the first period ends).
    pub fn inputs(&self) -> &[EnvInputs] {
        &self.inputs
    }

    /// Replaces the inputs (written by the period step).
    pub(crate) fn set_inputs(&mut self, inputs: Vec<EnvInputs>) {
        self.inputs = inputs;
    }

    /// Mutable inputs, for tests of the hash.
    #[doc(hidden)]
    pub fn inputs_mut(&mut self) -> &mut Vec<EnvInputs> {
        &mut self.inputs
    }
}
