//! Shared set-up for the SPEC-009 weather tests: the environment table and small weather
//! timelines written as text.
#![allow(dead_code)]

use battle_rules::{EnvTable, TerrainTable, WeatherTable};

const ENV: &str = include_str!("../../../../data/rules/environment.toml");
const TERRAIN: &str = include_str!("../../../../data/rules/terrain.toml");

const PROV: &str =
    "confidence = \"unverified\", placeholder = true, calibration_target = \"CT-009-6\"";

/// `environment.toml` as shipped.
pub fn env_table() -> EnvTable {
    env_table_with(&[])
}

/// `environment.toml` with each `(from, to)` replaced once (the text must exist).
pub fn env_table_with(edits: &[(&str, &str)]) -> EnvTable {
    let mut text = ENV.to_owned();
    for (from, to) in edits {
        assert!(text.contains(from), "environment.toml lacks {from:?}");
        text = text.replacen(from, to, 1);
    }
    let terrain = TerrainTable::from_toml_str(TERRAIN).unwrap();
    EnvTable::from_toml_str(&text, &terrain).unwrap()
}

/// A timeline from `(t_min, temp_f10, humidity_pm, wind_x_mm_s, wind_y_mm_s, rain_pm)` rows;
/// the clear visibility and light are fixed.
pub fn weather(rows: &[(i64, i32, u32, i32, i32, u32)]) -> WeatherTable {
    let mut text = String::from("schema = \"weather/1\"\nstart_minute = 0\nkey = [\n");
    for (t, temp, hum, wx, wy, rain) in rows {
        text.push_str(&format!(
            "  {{ t_min = {t}, temp_f10 = {temp}, humidity_pm = {hum}, wind_x_mm_s = {wx}, \
             wind_y_mm_s = {wy}, rain_pm = {rain}, clear_vis_mm = 16000000, light_pm = 1000, {PROV} }},\n"
        ));
    }
    text.push_str("]\n");
    WeatherTable::from_toml_str(&text).unwrap()
}
