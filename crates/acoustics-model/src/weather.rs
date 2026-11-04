//! Weather as the propagation model sees it (SPEC-012 section 4).

use crate::tables::WeatherPreset;
use core_math::{Fx, Vec2Fx};

/// Uniform, constant weather: temperature, wind and temperature lapse.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Weather {
    /// Air temperature, tenths of a degree C.
    pub temp_dc: i16,
    /// Wind vector, mm/s, x then y.
    pub wind_mm_s: [i32; 2],
    /// Temperature change with height, millikelvin per metre (negative: cooler aloft).
    pub lapse_mk_per_m: i16,
}

/// Metres to millimetres: `floor(raw * 1000 / 2^32)`.
pub fn to_mm(x: Fx) -> i64 {
    // The arithmetic shift of the 128-bit product floors toward minus infinity.
    ((i128::from(x.raw()) * 1000) >> 32) as i64
}

/// Narrows to the field type, saturating at its limits.
fn narrow<T: TryFrom<i64>>(v: i64, lo: T, hi: T) -> T {
    T::try_from(v).unwrap_or(if v < 0 { lo } else { hi })
}

impl Weather {
    /// Converts simulation units: the wind in m/s becomes mm/s through [`to_mm`].
    pub fn from_sim(temp_dc: i16, wind: Vec2Fx, lapse_mk_per_m: i16) -> Weather {
        let mm = |v: Fx| narrow(to_mm(v), i32::MIN, i32::MAX);
        Weather {
            temp_dc,
            wind_mm_s: [mm(wind.x), mm(wind.y)],
            lapse_mk_per_m,
        }
    }

    /// The weather of a named preset in the rules data.
    pub fn from_preset(p: &WeatherPreset) -> Weather {
        Weather {
            temp_dc: narrow(i64::from(p.temp_dc), i16::MIN, i16::MAX),
            wind_mm_s: [p.wind_x_mm_s, p.wind_y_mm_s],
            lapse_mk_per_m: narrow(i64::from(p.lapse_mk_per_m), i16::MIN, i16::MAX),
        }
    }
}
