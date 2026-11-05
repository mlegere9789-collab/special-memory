//! T12-11 (SPEC-012 sections 11 and 12): with the shipped data the bands CB1 to CB6 hold. Every
//! point is printed with its measured value next to the limits. A band is never edited to turn
//! this test green.

mod common;
use acoustics_model::propagation::{delay_ms, sound_speed_mm_s};
use acoustics_model::{scenes, AcousticTables};
use battle_rules::{parse_table, DataError, Provenance};
use common::*;

const BANDS: &str = include_str!("../../../qc/calibration/acoustics-bands.toml");

struct Point {
    scene: String,
    weather: String,
    at_mm: i64,
    metric: String,
    low: Option<i64>,
    high: Option<i64>,
}

struct Band {
    id: String,
    points: Vec<Point>,
    provenance: Provenance,
}

fn parse(text: &str) -> Result<Vec<Band>, DataError> {
    let mut root = parse_table(text)?;
    if root.take_str("schema")? != "bands/1" {
        return Err(DataError::Parse("unknown bands schema".into()));
    }
    let mut out = Vec::new();
    for mut row in root.take_rows("band")? {
        let id = row.take_str("id")?;
        let scene = row.take_str_opt("scene")?.unwrap_or_default();
        row.take_str("note")?;
        let mut points = Vec::new();
        for mut p in row.take_rows("points")? {
            points.push(Point {
                scene: scene.clone(),
                weather: p.take_str("weather")?,
                at_mm: p.take_int("at_mm")?,
                metric: p.take_str("metric")?,
                low: p.take_int_opt("low")?,
                high: p.take_int_opt("high")?,
            });
            p.finish()?;
        }
        let provenance = row.take_provenance()?;
        row.finish()?;
        out.push(Band {
            id,
            points,
            provenance,
        });
    }
    root.finish()?;
    Ok(out)
}

fn measure(t: &AcousticTables, p: &Point) -> i64 {
    let w = preset(t, &p.weather);
    if p.metric == "delay_ms" {
        return delay_ms(p.at_mm, sound_speed_mm_s(t, i32::from(w.temp_dc)), 0) as i64;
    }
    let scene = scenes::by_name(&p.scene).expect("scene exists");
    let r = scene.run(t, &p.weather, p.at_mm).report;
    match p.metric.as_str() {
        "margin_cdb" => i64::from(r.margin_cdb),
        "class" => r.class as i64,
        other => panic!("unknown metric {other}"),
    }
}

/// Measures every point; returns `(id, line, within)` per point.
fn run_bands(t: &AcousticTables, text: &str) -> Result<Vec<(String, String, bool)>, DataError> {
    let mut rows = Vec::new();
    for b in parse(text)? {
        for p in &b.points {
            let v = measure(t, p);
            let ok = p.low.is_none_or(|l| v >= l) && p.high.is_none_or(|h| v <= h);
            let line = format!(
                "{} {} {} {}@{}: {v} in [{:?}, {:?}]",
                b.id, p.scene, p.metric, p.weather, p.at_mm, p.low, p.high
            );
            rows.push((b.id.clone(), line, ok));
        }
    }
    Ok(rows)
}

#[test]
fn the_file_covers_cb1_to_cb6_with_provenance() {
    let bands = parse(BANDS).unwrap();
    let ids: Vec<&str> = bands.iter().map(|b| b.id.as_str()).collect();
    assert_eq!(ids, ["CB1", "CB2", "CB3", "CB4", "CB5", "CB6"]);
    assert!(bands
        .iter()
        .all(|b| b.provenance.placeholder || !b.provenance.sources.is_empty()));
    assert!(bands.iter().all(|b| !b.points.is_empty()));
    for ct in ["CT-012-1", "CT-012-2"] {
        assert!(BANDS.contains(&format!("calibration_target = \"{ct}\"")));
    }
    assert!(!BANDS.contains("confidence = \"verified\""));
}

#[test]
fn a_band_file_with_a_float_is_refused() {
    let float = BANDS.replacen("high = 2930", "high = 2930.5", 1);
    assert!(parse(&float).is_err());
}

#[test]
fn an_impossible_band_is_reported_as_a_miss() {
    let t = tables();
    let bad = BANDS.replacen("high = 2930", "high = 2900", 1);
    let rows = run_bands(&t, &bad).unwrap();
    assert!(rows.iter().any(|(id, _, ok)| id == "CB1" && !ok));
}

#[test]
fn every_band_holds_with_the_shipped_data() {
    let t = tables();
    let rows = run_bands(&t, BANDS).unwrap();
    assert_eq!(rows.len(), 9);
    for (_, line, ok) in &rows {
        println!("{} {line}", if *ok { "ok  " } else { "MISS" });
    }
    let misses: Vec<&str> = rows.iter().filter(|r| !r.2).map(|r| r.1.as_str()).collect();
    assert!(misses.is_empty(), "bands missed: {misses:?}");
}
