//! T14-06 (polygon half): each negative fixture yields exactly its named ring error, the good
//! fixture and the synthetic survey yield none, and the exact-geometry corner cases hold.
mod common;

use common::*;
use hd_terrain::{check, run_cli, survey};

const PROV: &str = "confidence = \"unverified\"\nsources = [{ source = 1, sheet = \"s\" }]\n";

fn poly(id: u32, class: &str, outer: &str, holes: Option<&str>) -> String {
    let holes = holes.map_or(String::new(), |h| format!("holes = [{h}]\n"));
    format!("\n[[polygon]]\nid = {id}\nclass = \"{class}\"\nouter = {outer}\n{holes}{PROV}")
}

fn cover(rows: &[String]) -> String {
    format!("schema = \"survey-landcover/1\"\n{}", rows.concat())
}

/// Problems as `(name, file, id)` for a survey whose `landcover.toml` is `text`.
fn land(tag: &str, text: String) -> Vec<(&'static str, &'static str, u32)> {
    run(tag, &[("landcover.toml", text)])
}

fn run(tag: &str, replace: &[(&str, String)]) -> Vec<(&'static str, &'static str, u32)> {
    let s = survey::load(&make_survey(tag, replace)).unwrap();
    check::check(&s, None)
        .iter()
        .map(|p| (p.name, p.file, p.id))
        .collect()
}

fn names(v: &[(&'static str, &'static str, u32)]) -> Vec<&'static str> {
    v.iter().map(|p| p.0).collect()
}

const SQ: &str = "[[80000, 80000], [100000, 80000], [100000, 100000], [80000, 100000]]";

#[test]
fn each_negative_fixture_yields_exactly_its_error() {
    for want in [
        "ring_short",
        "edge_zero",
        "ring_flat",
        "ring_self_touch",
        "ring_winding",
        "hole_outside",
        "group_overlap",
    ] {
        let got = land(
            &format!("ring_neg_{want}"),
            fixture_text(&format!("negative/{want}"), "landcover.toml"),
        );
        assert_eq!(names(&got), vec![want], "fixture {want}");
        assert_eq!(got[0].1, "landcover.toml");
    }
}

#[test]
fn good_fixture_passes() {
    let got = land(
        "ring_good",
        fixture_text("positive/ring_good", "landcover.toml"),
    );
    assert!(got.is_empty(), "{got:?}");
}

#[test]
fn problems_name_the_polygon_row() {
    let text = cover(&[
        poly(7, "crop", SQ, None),
        poly(9, "woods", "[[0, 0], [10000, 0]]", None),
    ]);
    assert_eq!(
        land("ring_ids", text),
        vec![("ring_short", "landcover.toml", 9)]
    );
}

#[test]
fn self_touch_catches_vertex_on_edge_and_spikes() {
    // A vertex of one edge lying on a non-adjacent edge (a pinched ring).
    let pinched = "[[80000, 80000], [120000, 80000], [120000, 120000], [100000, 80000], [100000, 100000], [80000, 100000]]";
    let t = cover(&[poly(1, "crop", pinched, None)]);
    assert_eq!(names(&land("ring_pinch", t)), vec!["ring_self_touch"]);
    // An adjacent edge folding back over the previous one.
    let spike =
        "[[80000, 80000], [120000, 80000], [140000, 80000], [110000, 80000], [110000, 100000]]";
    let t = cover(&[poly(1, "crop", spike, None)]);
    assert_eq!(names(&land("ring_spike", t)), vec!["ring_self_touch"]);
}

#[test]
fn hole_rules() {
    let outer = "[[80000, 80000], [120000, 80000], [120000, 120000], [80000, 120000]]";
    let cw = |x0: i64, y0: i64, x1: i64, y1: i64| {
        format!("[[{x0}, {y0}], [{x0}, {y1}], [{x1}, {y1}], [{x1}, {y0}]]")
    };
    let case = |tag: &str, hole: String| {
        let t = cover(&[poly(1, "yard", outer, Some(&hole))]);
        names(&land(tag, t))
    };
    assert!(case("hole_in", cw(90000, 90000, 110000, 110000)).is_empty());
    // Entirely outside, crossing the outer ring, and an edge lying along the outer ring.
    assert_eq!(
        case("hole_far", cw(200000, 200000, 220000, 220000)),
        vec!["hole_outside"]
    );
    assert_eq!(
        case("hole_cross", cw(100000, 90000, 140000, 110000)),
        vec!["hole_outside"]
    );
    assert_eq!(
        case("hole_edge", cw(80000, 90000, 100000, 110000)),
        vec!["hole_outside"]
    );
    // A counter-clockwise hole is a winding error, not a placement error.
    let ccw = "[[90000, 90000], [110000, 90000], [110000, 110000], [90000, 110000]]".to_string();
    assert_eq!(case("hole_ccw", ccw), vec!["ring_winding"]);
}

#[test]
fn overlap_is_decided_on_interiors() {
    let sq = |x0: i64, y0: i64, x1: i64, y1: i64| {
        format!("[[{x0}, {y0}], [{x1}, {y0}], [{x1}, {y1}], [{x0}, {y1}]]")
    };
    let two = |tag: &str, a: &str, b: &str, ca: &str, cb: &str| {
        names(&land(
            tag,
            cover(&[poly(1, ca, a, None), poly(2, cb, b, None)]),
        ))
    };
    let base = sq(80000, 80000, 100000, 100000);
    // Contained, identical and partly sharing an edge in the same direction all overlap.
    assert_eq!(
        two(
            "ov_in",
            &base,
            &sq(85000, 85000, 95000, 95000),
            "crop",
            "woods"
        ),
        vec!["group_overlap"]
    );
    assert_eq!(
        two("ov_same", &base, &base, "crop", "woods"),
        vec!["group_overlap"]
    );
    assert_eq!(
        two(
            "ov_edge",
            &base,
            &sq(80000, 80000, 100000, 120000),
            "crop",
            "woods"
        ),
        vec!["group_overlap"]
    );
    // Sharing an edge, touching at a corner, or apart: no overlap.
    for (tag, b) in [
        ("ov_side", sq(100000, 80000, 120000, 100000)),
        ("ov_corner", sq(100000, 100000, 120000, 120000)),
        ("ov_apart", sq(200000, 200000, 220000, 220000)),
    ] {
        assert!(two(tag, &base, &b, "crop", "woods").is_empty(), "{tag}");
    }
    // Ground against water are different groups.
    assert!(two("ov_groups", &base, &base, "crop", "deep_water").is_empty());
    assert_eq!(
        two("ov_water", &base, &base, "ford", "deep_water"),
        vec!["group_overlap"]
    );
}

#[test]
fn overlap_respects_holes() {
    let outer = "[[80000, 80000], [120000, 80000], [120000, 120000], [80000, 120000]]";
    let hole = "[[90000, 90000], [90000, 110000], [110000, 110000], [110000, 90000]]";
    let ring = |a: i64, b: i64| format!("[[{a}, {a}], [{b}, {a}], [{b}, {b}], [{a}, {b}]]");
    let case = |tag: &str, inner: String| {
        let t = cover(&[
            poly(1, "yard", outer, Some(hole)),
            poly(2, "woods", &inner, None),
        ]);
        names(&land(tag, t))
    };
    // Inside the hole, and exactly filling it: free. Straddling the hole's edge, or in the
    // solid band: overlap.
    assert!(case("hl_in", ring(95000, 105000)).is_empty());
    assert!(case("hl_fill", ring(90000, 110000)).is_empty());
    assert_eq!(
        case("hl_straddle", ring(100000, 115000)),
        vec!["group_overlap"]
    );
    assert_eq!(case("hl_band", ring(82000, 88000)), vec!["group_overlap"]);
}

#[test]
fn buildings_are_a_group_and_their_rings_are_checked() {
    let b = |id: u32, outer: &str| {
        format!("\n[[building]]\nid = {id}\nheight_mm = 6000\nouter = {outer}\n{PROV}")
    };
    let file = |rows: &[String]| format!("schema = \"survey-buildings/1\"\n{}", rows.concat());
    let sq = |x0: i64, x1: i64| {
        format!("[[{x0}, 400000], [{x1}, 400000], [{x1}, 420000], [{x0}, 420000]]")
    };
    let got = run(
        "bld_overlap",
        &[(
            "buildings.toml",
            file(&[b(1, &sq(300000, 320000)), b(2, &sq(310000, 330000))]),
        )],
    );
    assert_eq!(got, vec![("group_overlap", "buildings.toml", 1)]);
    let cw = "[[300000, 400000], [300000, 420000], [320000, 420000], [320000, 400000]]";
    let got = run("bld_cw", &[("buildings.toml", file(&[b(3, cw)]))]);
    assert_eq!(got, vec![("ring_winding", "buildings.toml", 3)]);
    // A building over ground cover is a different group.
    let got = run(
        "bld_ground",
        &[
            ("buildings.toml", file(&[b(1, &sq(80000, 100000))])),
            (
                "landcover.toml",
                cover(&[poly(1, "crop", &sq(80000, 100000), None)]),
            ),
        ],
    );
    assert!(got.is_empty(), "{got:?}");
}

#[test]
fn synthetic_survey_still_passes() {
    assert!(run("ring_synth", &[]).is_empty());
}

#[test]
fn cli_reports_ring_errors() {
    let bad = make_survey(
        "ring_cli",
        &[(
            "landcover.toml",
            fixture_text("negative/ring_winding", "landcover.toml"),
        )],
    );
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let args = vec!["check".into(), "--survey".into(), bad.display().to_string()];
    assert_eq!(run_cli(&args, &mut out, &mut err), 1);
    let text = String::from_utf8(err).unwrap();
    assert!(
        text.contains("ring_winding") && text.contains("landcover.toml"),
        "{text}"
    );
}
