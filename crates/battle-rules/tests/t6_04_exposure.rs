//! T6-04: exposure classes at the listed bearings, checked against `qc/fixtures/ref_morale.py`,
//! whose classes come from the boundary angles alone (SPEC-006 section 5.2).

use battle_rules::morale::{classify_exposure, Exposure};

const CSV: &str = include_str!("../../../qc/fixtures/ref_morale.csv");

fn name(e: Exposure) -> &'static str {
    match e {
        Exposure::Front => "front",
        Exposure::Flank => "flank",
        Exposure::Enfilade => "enfilade",
        Exposure::Rear => "rear",
    }
}

fn reference() -> Vec<(i32, i128, i128, &'static str)> {
    CSV.lines()
        .filter_map(|l| {
            let c: Vec<&str> = l.split(',').collect();
            (c[0] == "exposure").then(|| {
                (
                    c[1].parse().unwrap(),
                    c[2].parse().unwrap(),
                    c[3].parse().unwrap(),
                    c[4],
                )
            })
        })
        .collect()
}

#[test]
fn listed_bearings() {
    let want = [
        (0, "front"),
        (59, "front"),
        (61, "flank"),
        (90, "enfilade"),
        (100, "enfilade"),
        (105, "enfilade"),
        (115, "flank"),
        (134, "flank"),
        (136, "rear"),
        (180, "rear"),
    ];
    let table = reference();
    for (bearing, class) in want {
        let (_, f, r, _) = *table.iter().find(|row| row.0 == bearing).unwrap();
        assert_eq!(name(classify_exposure(f, r)), class, "bearing {bearing}");
    }
}

#[test]
fn every_reference_bearing_agrees() {
    let table = reference();
    assert_eq!(table.len(), 20);
    for (bearing, f, r, class) in table {
        assert_eq!(name(classify_exposure(f, r)), class, "bearing {bearing}");
    }
}

#[test]
fn bearing_to_the_left_matches_the_right() {
    for (_, f, r, class) in reference() {
        assert_eq!(name(classify_exposure(f, -r)), class);
    }
}

#[test]
fn boundaries_and_extremes() {
    // r * 1000 == 1732 * f is not front; |f| * 1000 == 364 * r is enfilade; r == -f is not rear.
    assert_eq!(classify_exposure(1000, 1732), Exposure::Flank);
    assert_eq!(classify_exposure(1000, 1731), Exposure::Front);
    assert_eq!(classify_exposure(364, 1000), Exposure::Enfilade);
    assert_eq!(classify_exposure(-1000, 1000), Exposure::Flank);
    assert_eq!(classify_exposure(-1000, 999), Exposure::Rear);
    assert_eq!(classify_exposure(0, 0), Exposure::Enfilade, "same position");
    assert_eq!(classify_exposure(i128::MAX, 0), Exposure::Front);
    // Inputs beyond any real dot product saturate instead of overflowing.
    for (f, r) in [
        (i128::MIN, i128::MAX),
        (i128::MIN, i128::MIN),
        (i128::MAX, i128::MIN),
    ] {
        let _ = classify_exposure(f, r);
    }
}
