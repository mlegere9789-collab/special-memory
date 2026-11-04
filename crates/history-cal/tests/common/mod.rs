//! Shared fixtures: the toy units, rules tables and script text builders.
#![allow(dead_code)]

use battle_rules::Tables;
use history_cal::{compile_script, units_from_toml_str, ReplayScript, ScriptInput, UnitRecord};

pub const RULES_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/rules");
pub const UNITS: &str = include_str!("../../../../data/scenarios/toy_replay/units.toml");
pub const PROV: &str =
    "confidence = \"unverified\"\nplaceholder = true\ncalibration_target = \"CT-011-1101\"\n";

pub fn tables() -> Tables {
    Tables::load_dir(RULES_DIR).unwrap()
}

pub fn units() -> Vec<UnitRecord> {
    units_from_toml_str(UNITS).unwrap()
}

/// A script with the given clock start and row text.
pub fn script(clock_start_s: i64, rows: &str) -> ReplayScript {
    let head = format!(
        "schema = \"replay-script/1\"\n[meta]\nscenario = \"toy_replay\"\nfidelity_class = 0\nclock_start_s = {clock_start_s}\nend_s = {}\n",
        clock_start_s + 1000
    );
    ReplayScript::from_toml_str(&format!("{head}{rows}")).unwrap()
}

pub fn order(id: &str, unit: &str, at_s: i64, kind: &str) -> String {
    let extra = match kind {
        "move" => "x_m = 300\ny_m = 0\ngait = \"quick\"\n",
        "form" => "formation = \"line\"\n",
        "face" => "heading_deg = 90\n",
        _ => "",
    };
    format!(
        "[[order]]\nid = \"{id}\"\nunit = \"{unit}\"\nat_s = {at_s}\nkind = \"{kind}\"\n{extra}{PROV}"
    )
}

pub fn trigger(id: &str, at_s: i64, extra: &str) -> String {
    format!("[[trigger]]\nid = \"{id}\"\nat_s = {at_s}\n{extra}{PROV}")
}

pub fn compile(s: &ReplayScript) -> Vec<ScriptInput> {
    compile_script(s, &units()).unwrap()
}
