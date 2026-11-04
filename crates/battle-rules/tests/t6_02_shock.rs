//! T6-02: shock arithmetic against `qc/fixtures/ref_morale.py` (exact `Fraction` reference,
//! output committed in `ref_morale.csv`), plus the `lerp` helper against the same file.

use battle_rules::morale::{lerp, shock_unit, MoraleTable};
use battle_rules::Training;

const CSV: &str = include_str!("../../../qc/fixtures/ref_morale.csv");
const MORALE: &str = include_str!("../../../data/rules/morale.toml");

/// Rows of one case, fields after the case name.
fn rows(case: &str) -> Vec<Vec<&'static str>> {
    CSV.lines()
        .filter(|l| l.split(',').next() == Some(case))
        .map(|l| l.split(',').skip(1).collect())
        .collect()
}

fn num<T: std::str::FromStr>(s: &str) -> T
where
    T::Err: std::fmt::Debug,
{
    s.parse().unwrap()
}

#[test]
fn the_documented_hand_check() {
    assert_eq!(shock_unit(400, 455), 5495);
    let g = MoraleTable::from_toml_str(MORALE).unwrap().globals;
    let unit = shock_unit(400, 455);
    assert_eq!(unit * g.w_killed_pm / 1000, 5495);
    assert_eq!(unit * g.w_disabled_pm / 1000, 3846);
    assert_eq!(unit * g.w_colonel_pm / 1000, 54950);
}

#[test]
fn shock_matches_the_reference_for_every_row() {
    let table = MoraleTable::from_toml_str(MORALE).unwrap();
    let g = &table.globals;
    let found = rows("shock");
    assert_eq!(found.len(), 15);
    for r in found {
        let level = match r[0] {
            "raw" => Training::Raw,
            "trained" => Training::Trained,
            _ => Training::Veteran,
        };
        let (s0, cap): (u32, u32) = (num(r[1]), num(r[2]));
        assert_eq!(cap, table.training(level).cap_pm);
        let unit = shock_unit(s0, cap);
        assert_eq!(unit, num::<u32>(r[3]), "{r:?}");
        let weights = [
            g.w_killed_pm,
            g.w_wounded_pm,
            g.w_disabled_pm,
            g.w_officer_pm,
            g.w_colonel_pm,
        ];
        for (w, want) in weights.iter().zip(&r[4..]) {
            assert_eq!(
                u64::from(unit) * u64::from(*w) / 1000,
                num::<u64>(want),
                "{r:?}"
            );
        }
    }
}

#[test]
fn shock_unit_is_defined_for_degenerate_input() {
    assert_eq!(shock_unit(0, 455), shock_unit(1, 455));
    assert_eq!(shock_unit(400, 0), shock_unit(400, 1));
    assert_eq!(shock_unit(1, 1), 1_000_000_000);
    assert_eq!(shock_unit(u32::MAX, u32::MAX), 1);
}

#[test]
fn lerp_matches_the_reference() {
    let found = rows("lerp");
    assert!(found.len() >= 20);
    for r in found {
        let v: Vec<i64> = r.iter().map(|s| num(s)).collect();
        assert_eq!(lerp(v[0], v[1], v[2], v[3], v[4]), v[5], "{r:?}");
    }
}

#[test]
fn lerp_edges() {
    assert_eq!(lerp(0, 10, 10, 7, 9), 7, "empty span gives v_hi");
    assert_eq!(lerp(0, 5, 10, 7, 9), 7, "inverted span gives v_hi");
    assert_eq!(lerp(i64::MIN, 10, 0, 100, 0), 0);
    assert_eq!(lerp(i64::MAX, 10, 0, 100, 0), 100);
    assert_eq!(lerp(7, 10, 0, 1, -1), 1, "the quotient -0.6 truncates to 0");
}
