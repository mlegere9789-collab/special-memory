//! T15-06 morale (checklist item 12): the three largest causes with the tie rule and their bands,
//! the warning rule, a distinct icon and symptom set per level, and the parameter files.

use std::collections::BTreeSet;
use ui_model::{morale_view, FactorBand, MoraleReport, MoraleViewParams};

const MORALE: &str = include_str!("../../../data/ui/morale.toml");
const SYMPTOMS: &str = include_str!("../../../data/ui/symptoms.toml");

fn params() -> MoraleViewParams {
    MoraleViewParams::from_toml_str(MORALE, SYMPTOMS).expect("morale data load")
}

fn splitmix(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

fn report(level: u8, margin: i32, factors: &[(u8, i32)]) -> MoraleReport {
    MoraleReport {
        level,
        margin,
        trend: 0,
        factors: factors.to_vec(),
    }
}

/// The reference: repeated selection of the next largest `|c|`, lowest id first on a tie.
fn reference_top(factors: &[(u8, i32)]) -> Vec<u8> {
    let mut left: Vec<(u8, i32)> = factors.to_vec();
    let mut out = Vec::new();
    while out.len() < 3 && !left.is_empty() {
        let mut best = 0;
        for i in 1..left.len() {
            let (a, b) = (left[i], left[best]);
            if a.1.unsigned_abs() > b.1.unsigned_abs()
                || (a.1.unsigned_abs() == b.1.unsigned_abs() && a.0 < b.0)
            {
                best = i;
            }
        }
        out.push(left.remove(best).0);
    }
    out
}

fn band_of(c: i32, p: &MoraleViewParams) -> FactorBand {
    let m = c.unsigned_abs();
    if m >= p.large_pm {
        FactorBand::Large
    } else if m >= p.moderate_pm {
        FactorBand::Moderate
    } else {
        FactorBand::Slight
    }
}

#[test]
fn over_1000_random_reports() {
    let p = params();
    let mut s = 15;
    for _ in 0..1000 {
        let level = (splitmix(&mut s) % 5) as u8;
        let margin = (splitmix(&mut s) % 41) as i32 * 50 - 100;
        let mut ids: Vec<u8> = (0..9).collect();
        // shuffle so the order of the list never decides a tie
        for i in (1..ids.len()).rev() {
            ids.swap(i, (splitmix(&mut s) % (i as u64 + 1)) as usize);
        }
        let factors: Vec<(u8, i32)> = ids
            .iter()
            .map(|&id| (id, (splitmix(&mut s) % 13) as i32 * 25 - 150))
            .collect();
        let total: i32 = factors.iter().map(|f| f.1).sum();
        let r = report(level, margin, &factors);
        let v = morale_view(&r, &p);
        let shown: Vec<u8> = v.top.iter().map(|t| t.0).collect();
        assert_eq!(shown, reference_top(&factors));
        let value = |id: u8| factors.iter().find(|f| f.0 == id).unwrap().1;
        for (id, band) in &v.top {
            assert_eq!(*band, band_of(value(*id), &p));
        }
        let shown_sum: i32 = shown.iter().map(|&id| value(id)).sum();
        let rest: i32 = factors
            .iter()
            .filter(|f| !shown.contains(&f.0))
            .map(|f| f.1)
            .sum();
        assert_eq!(shown_sum + rest, total);
        assert_eq!(v.warn, level == 2 && margin < p.warn_margin_pm as i32);
        assert_eq!((v.level, v.trend), (level, r.trend));
    }
}

#[test]
fn ties_go_to_the_lower_factor_id_and_signs_do_not_matter() {
    let p = params();
    let r = report(0, 0, &[(5, -100), (2, 100), (7, 100), (1, 40), (0, -40)]);
    let top: Vec<u8> = morale_view(&r, &p).top.iter().map(|t| t.0).collect();
    assert_eq!(top, vec![2, 5, 7]);
    let r = report(0, 0, &[(4, 10), (3, -10), (9, 10), (1, 10)]);
    let top: Vec<u8> = morale_view(&r, &p).top.iter().map(|t| t.0).collect();
    assert_eq!(top, vec![1, 3, 4]);
}

#[test]
fn short_and_extreme_reports_do_not_fail() {
    let p = params();
    assert!(morale_view(&report(0, 0, &[]), &p).top.is_empty());
    assert_eq!(morale_view(&report(1, 0, &[(3, 5)]), &p).top.len(), 1);
    let v = morale_view(&report(2, 0, &[(0, i32::MIN), (1, i32::MAX)]), &p);
    assert_eq!(v.top, vec![(0, FactorBand::Large), (1, FactorBand::Large)]);
}

#[test]
fn band_thresholds_are_inclusive() {
    let p = params();
    let (l, m) = (p.large_pm as i32, p.moderate_pm as i32);
    let r = report(0, 0, &[(0, l), (1, -m), (2, m - 1)]);
    assert_eq!(
        morale_view(&r, &p).top,
        vec![
            (0, FactorBand::Large),
            (1, FactorBand::Moderate),
            (2, FactorBand::Slight)
        ]
    );
    let r = report(0, 0, &[(0, l - 1), (1, -(m - 1))]);
    assert_eq!(
        morale_view(&r, &p).top,
        vec![(0, FactorBand::Moderate), (1, FactorBand::Slight)]
    );
}

#[test]
fn the_warning_follows_the_rule() {
    let p = params();
    let w = p.warn_margin_pm as i32;
    for level in 0..5u8 {
        for margin in [-50, 0, w - 50, w - 1, w, w + 50, 1000] {
            let v = morale_view(&report(level, margin, &[(0, 1)]), &p);
            assert_eq!(
                v.warn,
                level == 2 && margin < w,
                "level {level} margin {margin}"
            );
        }
    }
}

#[test]
fn every_level_has_a_distinct_icon_and_symptom_set() {
    let p = params();
    let mut icons = BTreeSet::new();
    let mut sets = BTreeSet::new();
    let mut words = BTreeSet::new();
    for level in 0..5u8 {
        let info = p.level_info(level).expect("five levels");
        icons.insert(info.icon);
        sets.insert((info.fire_slowed, info.looking_back, info.ranks_thinning));
        words.insert(info.word.clone());
    }
    assert_eq!((icons.len(), sets.len(), words.len()), (5, 5, 5));
    assert!(p.level_info(5).is_none());
    // a steady regiment shows no symptom; a broken one shows all three
    let (first, last) = (p.level_info(0).unwrap(), p.level_info(4).unwrap());
    assert!(!first.fire_slowed && !first.looking_back && !first.ranks_thinning);
    assert!(last.fire_slowed && last.looking_back && last.ranks_thinning);
}

#[test]
fn the_values_are_placeholders_with_their_calibration_target() {
    let p = params();
    assert!(p.large_pm > p.moderate_pm && p.moderate_pm > 0);
    assert!(p.provenance.placeholder);
    assert_eq!(
        (
            p.provenance.calibration_spec,
            p.provenance.calibration_target
        ),
        (Some(15), Some(3))
    );
}

#[test]
fn loader_refuses_bad_files() {
    let load = |m: &str, s: &str| MoraleViewParams::from_toml_str(m, s);
    assert!(load(
        &MORALE.replace("large_pm = 200", "large_pm = 20.5"),
        SYMPTOMS
    )
    .is_err());
    assert!(load(&MORALE.replace("large_pm = 200", "large_pm = 50"), SYMPTOMS).is_err());
    assert!(load(
        &MORALE.replace("calibration_target = \"CT-015-3\"\n", ""),
        SYMPTOMS
    )
    .is_err());
    assert!(load(&MORALE.replace("ui-morale/1", "ui-morale/0"), SYMPTOMS).is_err());
    assert!(load(&format!("{MORALE}\nextra = 1\n"), SYMPTOMS).is_err());
    assert!(load(MORALE, &SYMPTOMS.replace("id = 4", "id = 3")).is_err());
    assert!(load(MORALE, &SYMPTOMS.replace("id = 4", "id = 5")).is_err());
    assert!(load(MORALE, &SYMPTOMS.replace("icon = 4", "icon = 3")).is_err());
    assert!(load(
        MORALE,
        &SYMPTOMS.replace("fire_slowed = true", "fire_slowed = 1")
    )
    .is_err());
    let one_less = SYMPTOMS
        .split("[[level]]")
        .take(5)
        .collect::<Vec<_>>()
        .join("[[level]]");
    assert!(load(MORALE, &one_less).is_err());
    assert!(load(MORALE, &SYMPTOMS.replace("ui-symptoms/1", "x")).is_err());
}
