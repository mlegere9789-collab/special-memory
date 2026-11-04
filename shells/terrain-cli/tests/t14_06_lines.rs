//! T14-06 (line half): each negative fixture yields exactly its named error; the synthetic survey
//! yields none. Also the survey-level checks of SPEC-014 section 3 (voids, coverage, sources) and
//! the strict reader (floats and unknown keys refused).
mod common;

use common::*;
use hd_terrain::{check, run_cli, survey};
use std::fs;

fn names_for(tag: &str, linear: String) -> Vec<&'static str> {
    let dir = make_survey(tag, &[("linear.toml", linear)]);
    let s = survey::load(&dir).unwrap();
    check::check(&s, None).iter().map(|p| p.name).collect()
}

#[test]
fn each_negative_fixture_yields_exactly_its_error() {
    for (dir, want) in [
        ("line_one_point", "line_short"),
        ("line_zero_segment", "line_short"),
        ("road_dangling", "road_dangling"),
        ("creek_uphill", "creek_uphill"),
    ] {
        let got = names_for(
            &format!("neg_{dir}"),
            fixture_text(&format!("negative/{dir}"), "linear.toml"),
        );
        assert_eq!(got, vec![want], "fixture {dir}");
    }
}

#[test]
fn synthetic_survey_passes() {
    let dir = make_survey("pos_synthetic", &[]);
    let s = survey::load(&dir).unwrap();
    assert!(check::check(&s, None).is_empty());
}

#[test]
fn road_end_near_another_road_is_not_dangling() {
    let row = |id: u32, pts: &str, dead: &str| {
        format!(
            "[[line]]\nid = {id}\nkind = \"road\"\nhalf_width_mm = 3000\n{dead}points = {pts}\n\
             confidence = \"unverified\"\nsources = [{{ source = 1, sheet = \"s\" }}]\n\n"
        )
    };
    // Road 2 ends 900 mm from road 1; road 1 runs from edge to edge.
    let mut t = String::from("schema = \"survey-linear/1\"\n\n");
    t += &row(1, "[[0, 100000], [511000, 100000]]", "");
    t += &row(
        2,
        "[[200000, 100900], [200000, 300000]]",
        "dead_end = true\n",
    );
    assert!(names_for("road_near", t.clone()).is_empty());
    // 1100 mm away and without dead_end: the south end dangles.
    let t = t
        .replace("100900", "101100")
        .replace("dead_end = true\n", "");
    assert_eq!(
        names_for("road_far", t),
        vec!["road_dangling", "road_dangling"]
    );
}

#[test]
fn floats_and_unknown_keys_are_refused() {
    let lin = fixture_text("synthetic", "linear.toml");
    let dir = make_survey(
        "neg_float",
        &[(
            "linear.toml",
            lin.replacen("half_width_mm = 3000", "half_width_mm = 3000.0", 1),
        )],
    );
    assert!(survey::load(&dir).is_err());
    let dir = make_survey(
        "neg_key",
        &[(
            "linear.toml",
            lin.replacen("kind = \"road\"", "kind = \"road\"\nwidth = 3", 1),
        )],
    );
    assert!(survey::load(&dir).is_err());
}

#[test]
fn voids_above_1000_ppm_and_uncovered_slab_posts_fail() {
    let dir = make_survey("neg_voids", &[]);
    // 300 voids of 263,169 posts is 1,140 ppm; coverage stays 1 so only the void rule fires.
    let dem = dem_bytes(|i, j| {
        if j == 7 && i < 300 {
            i32::MIN
        } else {
            plane(i, j)
        }
    });
    fs::write(dir.join("dem_cm.i32"), dem).unwrap();
    let s = survey::load(&dir).unwrap();
    let names: Vec<_> = check::check(&s, None).iter().map(|p| p.name).collect();
    assert_eq!(names, vec!["voids_high"]);

    let dir = make_survey("neg_cover", &[]);
    let mut cov = vec![1u8; POSTS * POSTS];
    cov[100 * POSTS + 100] = 0;
    fs::write(dir.join("coverage.u8"), cov).unwrap();
    let s = survey::load(&dir).unwrap();
    let names: Vec<_> = check::check(&s, None).iter().map(|p| p.name).collect();
    assert_eq!(names, vec!["slab_uncovered"]);
}

#[test]
fn uncleared_sources_fail_and_cleared_ones_pass() {
    let dir = make_survey("src_check", &[]);
    let s = survey::load(&dir).unwrap();
    let path = dir.join("sources.toml");
    let src = |status: &str| {
        format!(
            "schema = \"survey-sources/1\"\nsurvey = \"synthetic\"\n\n[[source]]\nid = 1\nkind = \"dem\"\n\
             provider = \"p\"\nproduct = \"q\"\nconsulted = true\ncoverage_status = \"passed\"\n\
             licence_status = \"{status}\"\n"
        )
    };
    fs::write(&path, src("pending")).unwrap();
    let names: Vec<_> = check::check(&s, Some(&path))
        .iter()
        .map(|p| p.name)
        .collect();
    assert_eq!(names, vec!["source_uncleared"]);
    fs::write(&path, src("cleared")).unwrap();
    assert!(check::check(&s, Some(&path)).is_empty());
}

#[test]
fn cli_check_exit_codes_and_messages() {
    let good = make_survey("cli_good", &[]);
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let args =
        |d: &std::path::Path| vec!["check".into(), "--survey".into(), d.display().to_string()];
    assert_eq!(run_cli(&args(&good), &mut out, &mut err), 0);
    let bad = make_survey(
        "cli_bad",
        &[(
            "linear.toml",
            fixture_text("negative/creek_uphill", "linear.toml"),
        )],
    );
    let (mut out, mut err) = (Vec::new(), Vec::new());
    assert_eq!(run_cli(&args(&bad), &mut out, &mut err), 1);
    let text = String::from_utf8(err).unwrap();
    assert!(
        text.contains("creek_uphill") && text.contains("linear.toml"),
        "{text}"
    );
    assert_eq!(
        run_cli(&["frobnicate".into()], &mut out, &mut Vec::new()),
        2
    );
}
