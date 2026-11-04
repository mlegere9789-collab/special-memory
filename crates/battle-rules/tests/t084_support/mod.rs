//! Shared set-up for the T-084 tests: scenario text for small maps and a loader.
#![allow(dead_code)]

use battle_rules::{Battle, Scenario, Tables};

pub const RULES_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/rules");

const PROVENANCE: &str =
    "confidence = \"partial\"\nsources = [{ document = \"CWV\", page = 80, note = \"test\" }]\n";

/// One unit of a test scenario and its arrival.
#[derive(Clone)]
pub struct U {
    pub id: &'static str,
    pub side: &'static str,
    pub arm: &'static str,
    pub men: u32,
    pub guns: u32,
    pub weapon: &'static str,
    pub rounds: u32,
    pub x_mm: i64,
    pub y_mm: i64,
    pub heading: u16,
    pub at_s: u32,
}

impl U {
    /// 120 riflemen of the Union facing east at `x` metres, arriving at `at_s`.
    pub fn blue(id: &'static str, x_m: i64, at_s: u32) -> U {
        U {
            id,
            side: "union",
            arm: "infantry",
            men: 120,
            guns: 0,
            weapon: "rifle_58",
            rounds: 40,
            x_mm: x_m * 1000,
            y_mm: 40_000,
            heading: 0,
            at_s,
        }
    }

    /// 120 riflemen of the Confederacy facing west.
    pub fn grey(id: &'static str, x_m: i64, at_s: u32) -> U {
        U {
            side: "confederate",
            heading: 180,
            ..U::blue(id, x_m, at_s)
        }
    }
}

/// Scenario text on a 160 m by 80 m map: units, one arrival each, and `extra` rows appended.
pub fn scenario_text(fc: u8, duration_s: u32, units: &[U], extra: &str) -> String {
    let mut s = format!(
        "schema = \"scenario/1\"\nid = \"t084\"\ntitle = \"Test\"\ndate = \"1863-07-01\"\n\
         clock_start_s = 26400\nduration_s = {duration_s}\ntick_hz = 20\nfidelity_class = {fc}\n\
         f0_cap = 15000\nagent_cap = 35000\n\n[map]\nwidth_mm = 160000\nheight_mm = 80000\n\
         origin_x_mm = 0\norigin_y_mm = 0\n\n[terrain]\ncell_mm = 40000\n\
         legend = [{{ ch = \".\", class = 0 }}]\nrows = \"\"\"\n....\n....\n\"\"\"\n\n"
    );
    for u in units {
        s += &format!(
            "[[unit]]\nid = \"{}\"\nside = \"{}\"\nparent = \"\"\narm = \"{}\"\nmen = {}\nguns = {}\n\
             training = \"trained\"\nweapon = \"{}\"\nrounds_per_man = {}\ncommander = \"\"\n{PROVENANCE}\n",
            u.id, u.side, u.arm, u.men, u.guns, u.weapon, u.rounds
        );
    }
    for u in units {
        s += &format!(
            "[[arrival]]\nunit = \"{}\"\nat_s = {}\nx_mm = {}\ny_mm = {}\nheading_deg = {}\n\
             formation = \"line\"\nlow_s = {}\nhigh_s = {}\n{PROVENANCE}\n",
            u.id, u.at_s, u.x_mm, u.y_mm, u.heading, u.at_s, u.at_s
        );
    }
    s += "[[weather]]\nfrom_s = 0\nwind_x_mm_s = 0\nwind_y_mm_s = 0\nwet_pm = 0\nground_pm = 0\n\
          visibility_m = 5000\nconfidence = \"unverified\"\nplaceholder = true\n\
          calibration_target = \"CT-008-88\"\nsources = [{ document = \"CWV\", page = 79, note = \"test\" }]\n\n";
    s += extra;
    s
}

/// A trigger row at `at_s` on `unit`; `body` holds the kind-specific keys.
pub fn trigger(id: &str, at_s: u32, unit: &str, kind: &str, body: &str) -> String {
    format!(
        "[[trigger]]\nid = \"{id}\"\nat_s = {at_s}\nkind = \"{kind}\"\nunit = \"{unit}\"\n{body}\n{PROVENANCE}\n"
    )
}

/// A departure row.
pub fn departure(unit: &str, at_s: u32) -> String {
    format!("[[departure]]\nunit = \"{unit}\"\nat_s = {at_s}\n{PROVENANCE}\n")
}

/// An objective row centred at `(x, y)` metres.
pub fn objective(id: &str, x_m: i64, y_m: i64, radius_m: i64) -> String {
    format!(
        "[[objective]]\nid = \"{id}\"\nside = \"union\"\nx_mm = {}\ny_mm = {}\nradius_mm = {}\npoints = 1\n{PROVENANCE}\n",
        x_m * 1000,
        y_m * 1000,
        radius_m * 1000
    )
}

pub fn tables() -> Tables {
    Tables::load_dir(RULES_DIR).unwrap()
}

pub fn load(text: &str) -> Scenario {
    Scenario::from_toml_str(text, &tables(), None).unwrap_or_else(|e| panic!("{e}\n{text}"))
}

pub fn battle(text: &str, seed: u64) -> Battle {
    Battle::from_scenario(&load(text), seed).unwrap()
}

/// Two blue and two grey regiments 60 m apart on the small map, all present at tick 0.
pub fn four() -> Vec<U> {
    vec![
        U::blue("b1", 50, 0),
        U {
            y_mm: 60_000,
            ..U::blue("b2", 50, 0)
        },
        U::grey("g1", 110, 0),
        U {
            y_mm: 60_000,
            ..U::grey("g2", 110, 0)
        },
    ]
}
