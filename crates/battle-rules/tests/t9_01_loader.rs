//! T9-01: `environment.toml` and the Gettysburg weather load; float, unknown key, provenance,
//! monotonicity, retain-class and flow-sum violations return the named `DataError` (SPEC-009
//! section 3).

use battle_rules::{Confidence, DataError, EnvTable, TerrainTable, WeatherTable, DEFAULT_CELL_MM};

const ENV: &str = include_str!("../../../data/rules/environment.toml");
const WEATHER: &str = include_str!("../../../data/scenarios/gettysburg/weather.toml");
const TERRAIN: &str = include_str!("../../../data/rules/terrain.toml");

fn terrain() -> TerrainTable {
    TerrainTable::from_toml_str(TERRAIN).unwrap()
}

/// Replaces the first occurrence of `from` in the environment file, which must exist.
fn env_with(from: &str, to: &str) -> Result<EnvTable, DataError> {
    assert!(ENV.contains(from), "fixture text lacks {from:?}");
    EnvTable::from_toml_str(&ENV.replacen(from, to, 1), &terrain())
}

fn wx_with(from: &str, to: &str) -> Result<WeatherTable, DataError> {
    assert!(WEATHER.contains(from), "fixture text lacks {from:?}");
    WeatherTable::from_toml_str(&WEATHER.replacen(from, to, 1))
}

#[test]
fn environment_loads_with_the_spec_values() {
    let e = EnvTable::from_toml_str(ENV, &terrain()).unwrap();
    let s = &e.smoke;
    assert_eq!(
        [
            s.period_ticks,
            s.cell_mm,
            s.height_mm,
            s.yield_pm,
            s.misfire_pm
        ],
        [4, 8000, 3000, 550, 250]
    );
    assert_eq!(
        [s.diffuse_ppm, s.decay_ppm, s.rain_decay_gain_pm, s.cell_cap],
        [10_000, 4000, 1000, 1_000_000]
    );
    assert_eq!((s.ext_k, s.ref_units), (20, 20_000));
    assert_eq!(s.provenance.calibration_target, Some(1));
    let g = &e.sight;
    assert_eq!(
        [g.period_ticks, g.step_mm, g.far_step_mm, g.clear_margin_mm],
        [20, 2000, 4000, 100]
    );
    assert_eq!(
        [
            g.min_transmit_pm,
            g.retries,
            g.range_smoke_gain_pm,
            g.peek_step_mm
        ],
        [50, 4, 1500, 500]
    );
    assert_eq!(
        [
            g.peek_max_mm,
            g.peek_back_mm,
            g.blind_report_pm,
            g.observer_samples
        ],
        [15_000, 250, 600, 8]
    );
    assert_eq!(
        (e.heat.floor_f10, e.heat.humid_floor_pm, e.heat.div),
        (700, 400, 1500)
    );
    let curve: Vec<_> = e
        .heat_curve
        .iter()
        .map(|p| (p.apparent_f10, p.drain_pm, p.rest_pm))
        .collect();
    assert_eq!(
        curve,
        [
            (700, 1000, 1000),
            (800, 1100, 950),
            (900, 1300, 850),
            (1000, 1700, 700)
        ]
    );
    let r = &e.rain;
    assert_eq!(
        [
            r.powder_rise_ppm,
            r.powder_fall_ppm,
            r.ground_rise_ppm,
            r.ground_fall_ppm
        ],
        [600, 150, 400, 40]
    );
    assert_eq!((r.wet_extra_pm, r.ground_min_pm), (2000, 500));
    let retain: Vec<u32> = (0..12).map(|c| e.retain_pm(c)).collect();
    assert_eq!(retain, [0, 0, 100, 400, 500, 0, 0, 0, 0, 0, 0, 0]);
    assert!(e.check_cell_mm(DEFAULT_CELL_MM).is_ok());
    assert!(matches!(e.check_cell_mm(4000), Err(DataError::Range(_))));
}

#[test]
fn gettysburg_weather_loads_and_the_880_keys_cite_p079() {
    let w = WeatherTable::from_toml_str(WEATHER).unwrap();
    assert_eq!(w.start_minute(), 0);
    assert_eq!(w.keys().len(), 24);
    let hot: Vec<_> = w.keys().iter().filter(|k| k.temp_f10 == 880).collect();
    assert_eq!(hot.len(), 3);
    for k in hot {
        assert_eq!(k.t_min % 1440, 840, "the peak is the 14:00 key");
        assert_eq!(k.provenance.confidence, Confidence::Partial);
        assert!(k
            .provenance
            .sources
            .iter()
            .any(|s| s.document == "CWV" && s.page == 79));
    }
    let rain: Vec<_> = w
        .keys()
        .iter()
        .map(|k| (k.t_min, k.rain_pm))
        .filter(|r| r.1 > 0)
        .collect();
    assert_eq!(rain, [(4290, 900), (4470, 900), (4560, 300), (4800, 300)]);
    assert!(w.keys().iter().all(|k| k.provenance.placeholder));
}

#[test]
fn environment_rejections() {
    let float = env_with("yield_pm = 550", "yield_pm = 550.5");
    assert!(matches!(float, Err(DataError::Float(p)) if p == "smoke.yield_pm"));
    let unknown = env_with("[sight]\n", "[sight]\nbogus = 1\n");
    assert!(matches!(unknown, Err(DataError::UnknownKey(p)) if p == "sight.bogus"));
    let no_target = env_with("calibration_target = \"CT-009-5\"\n", "");
    assert!(matches!(no_target, Err(DataError::Provenance(_))));
    let bare = env_with("\"CT-009-1\"", "\"CT-1\"");
    assert!(matches!(bare, Err(DataError::Provenance(_))));
    let range = env_with("misfire_pm = 250", "misfire_pm = 2000");
    assert!(matches!(range, Err(DataError::Range(_))));
    let schema = env_with("environment/1", "environment/2");
    assert!(matches!(schema, Err(DataError::Parse(_))));
}

#[test]
fn heat_curve_must_be_monotone() {
    let drain = env_with("drain_pm = 1100", "drain_pm = 900");
    assert!(matches!(drain, Err(DataError::Range(m)) if m.contains("drain_pm")));
    let rest = env_with("rest_pm = 950", "rest_pm = 1100");
    assert!(matches!(rest, Err(DataError::Range(m)) if m.contains("rest_pm")));
    let apparent = env_with("apparent_f10 = 800", "apparent_f10 = 700");
    assert!(matches!(apparent, Err(DataError::Range(m)) if m.contains("apparent_f10")));
}

#[test]
fn retain_classes_must_match_the_terrain_table() {
    let unknown = env_with("{ class = 7,", "{ class = 99,");
    assert!(matches!(unknown, Err(DataError::Range(m)) if m.contains("99")));
    let dup = env_with("{ class = 7,", "{ class = 6,");
    assert!(matches!(dup, Err(DataError::Range(_))));
    let no_row: String = ENV
        .lines()
        .filter(|l| !l.starts_with("  { class = 11,"))
        .map(|l| format!("{l}\n"))
        .collect();
    let missing = EnvTable::from_toml_str(&no_row, &terrain());
    assert!(matches!(missing, Err(DataError::Range(m)) if m.contains("11")));
    let short = TERRAIN.split("[[class]]\nid = 8").next().unwrap();
    let narrow = TerrainTable::from_toml_str(short).unwrap();
    assert!(matches!(
        EnvTable::from_toml_str(ENV, &narrow),
        Err(DataError::Range(_))
    ));
}

#[test]
fn flow_sum_limit_is_enforced() {
    // 4 * 10000 plus the wind share of 707500 passes; 4 * 70000 does not.
    assert!(env_with("diffuse_ppm = 10000", "diffuse_ppm = 70000").is_ok());
    let over = env_with("diffuse_ppm = 10000", "diffuse_ppm = 80000");
    assert!(matches!(over, Err(DataError::Range(m)) if m.contains("flow")));
    let fast = env_with(
        "period_ticks = 4\ncell_mm = 8000",
        "period_ticks = 8\ncell_mm = 8000",
    );
    assert!(matches!(fast, Err(DataError::Range(m)) if m.contains("flow")));
}

#[test]
fn weather_rejections() {
    let float = wx_with("temp_f10 = 680", "temp_f10 = 680.0");
    assert!(matches!(float, Err(DataError::Float(p)) if p.starts_with("key[0]")));
    let unknown = wx_with("light_pm = 50,", "light_pm = 50, dew = 3,");
    assert!(matches!(unknown, Err(DataError::UnknownKey(p)) if p == "key[0].dew"));
    let provenance = wx_with("calibration_target = \"CT-009-6\"", "note = \"x\"");
    assert!(matches!(provenance, Err(DataError::Provenance(_))));
    let order = wx_with("t_min = 270", "t_min = 0");
    assert!(matches!(order, Err(DataError::Range(m)) if m.contains("ascending")));
    let rain = wx_with("rain_pm = 0", "rain_pm = 1001");
    assert!(matches!(rain, Err(DataError::Range(_))));
    let light = wx_with("light_pm = 50,", "light_pm = 1001,");
    assert!(matches!(light, Err(DataError::Range(_))));
    let wind = wx_with(
        "wind_x_mm_s = 400, wind_y_mm_s = 200",
        "wind_x_mm_s = 15000, wind_y_mm_s = 15000",
    );
    assert!(matches!(wind, Err(DataError::Range(m)) if m.contains("wind")));
    assert!(wx_with(
        "wind_x_mm_s = 400, wind_y_mm_s = 200",
        "wind_x_mm_s = 14000, wind_y_mm_s = 14000"
    )
    .is_ok());
    let schema = wx_with("weather/1", "weather/2");
    assert!(matches!(schema, Err(DataError::Parse(_))));
    let empty = WeatherTable::from_toml_str("schema = \"weather/1\"\nstart_minute = 0\nkey = []\n");
    assert!(matches!(empty, Err(DataError::Parse(_))));
}
