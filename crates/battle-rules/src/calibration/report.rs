//! Text and JSON renderings of a fire band suite: the table of the cycle report and the
//! reference curves of `qc/calibration/f0-curves.json` (all integers, no floating point).

use crate::calibration::fire::{BandRow, LineRun, SuiteReport, SAMPLE_EVERY};

fn fmt_scaled(v: i64, scale: i64) -> String {
    if scale <= 1 {
        return v.to_string();
    }
    let digits = scale.ilog10() as usize;
    format!("{}.{:0w$}", v / scale, (v % scale).abs(), w = digits)
}

fn limit(r: &BandRow, v: Option<i64>) -> String {
    v.map_or("none".to_string(), |x| fmt_scaled(x, r.scale))
}

/// One line per band point: id, point, mean, min, max, limits and the verdict.
pub fn render_text(report: &SuiteReport) -> String {
    let mut s = String::new();
    for r in &report.rows {
        s.push_str(&format!(
            "{:<4} {:<10} mean {:>10} min {:>10} max {:>10} band [{} .. {}] {}\n",
            r.id,
            r.label,
            fmt_scaled(r.mean, r.scale),
            fmt_scaled(r.min, r.scale),
            fmt_scaled(r.max, r.scale),
            limit(r, r.low),
            limit(r, r.high),
            if r.pass { "PASS" } else { "MISS" }
        ));
    }
    s
}

fn list(v: impl Iterator<Item = i64>) -> String {
    v.map(|x| x.to_string()).collect::<Vec<_>>().join(", ")
}

/// The reference curves as JSON: casualties per 100 men (times 1000) against time and range for
/// every `cal_line` set, the canister counts, and every band row.
pub fn render_curves_json(report: &SuiteReport) -> String {
    let mut s = String::from("{\n  \"format\": 1,\n  \"scale\": 1000,\n  \"sample_every_ticks\": ");
    s.push_str(&format!("{SAMPLE_EVERY},\n  \"lines\": [\n"));
    for (i, l) in report.lines.iter().enumerate() {
        let n = l.runs.len().max(1) as i64;
        let cols = l.runs.first().map_or(0, |r| r.by_tick.len());
        let curve = (0..cols).map(|c| {
            l.runs.iter().map(|r| i64::from(r.by_tick[c])).sum::<i64>() * 100_000 / (400 * n)
        });
        let per: Vec<i64> = l.runs.iter().map(LineRun::per_100_milli).collect();
        s.push_str(&format!(
            "    {{\"weapon\": \"{}\", \"range_mm\": {}, \"wall\": {}, \"casualties_per_100_by_tick\": [{}], \"final_min\": {}, \"final_max\": {}}}",
            l.weapon,
            l.range_mm,
            l.wall,
            list(curve),
            per.iter().min().copied().unwrap_or(0),
            per.iter().max().copied().unwrap_or(0)
        ));
        s.push_str(if i + 1 < report.lines.len() {
            ",\n"
        } else {
            "\n"
        });
    }
    s.push_str("  ],\n  \"canister\": [\n");
    for (i, c) in report.canister.iter().enumerate() {
        s.push_str(&format!(
            "    {{\"range_mm\": {}, \"rounds\": {}, \"casualties\": {}}}",
            c.0, c.1, c.2
        ));
        s.push_str(if i + 1 < report.canister.len() {
            ",\n"
        } else {
            "\n"
        });
    }
    s.push_str("  ],\n  \"bands\": [\n");
    for (i, r) in report.rows.iter().enumerate() {
        s.push_str(&format!(
            "    {{\"id\": \"{}\", \"point\": \"{}\", \"scale\": {}, \"mean\": {}, \"min\": {}, \"max\": {}}}",
            r.id, r.label, r.scale, r.mean, r.min, r.max
        ));
        s.push_str(if i + 1 < report.rows.len() {
            ",\n"
        } else {
            "\n"
        });
    }
    s.push_str("  ]\n}\n");
    s
}
