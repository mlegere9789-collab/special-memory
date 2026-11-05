//! T10-08: the horizon follows the SPEC-010 section 7 formula, checked against the independent
//! reference `qc/fixtures/ref_budget.py` (its committed output), and repeats on rerun.

use battle_ai::{plan_budget, tactical_budget, BudgetPlan, HORIZON_MAX, HORIZON_MIN};

const REF: &str = include_str!("../../../qc/fixtures/ref_budget.csv");

fn rows() -> Vec<(String, [u64; 7])> {
    REF.lines()
        .map(|l| {
            let mut it = l.split(',');
            let kind = it.next().expect("kind").to_owned();
            let n: Vec<u64> = it.map(|x| x.parse().expect("integer")).collect();
            (kind, n.try_into().expect("seven numbers"))
        })
        .collect()
}

#[test]
fn horizon_matches_the_reference() {
    let rows = rows();
    assert!(rows.iter().filter(|r| r.0 == "plan").count() >= 6);
    for (kind, [b, c, s, n, h, cu, su]) in rows {
        let (c, s, n) = (c as u32, s as u32, n as u32);
        let got = match kind.as_str() {
            "plan" => plan_budget(b, c, s, n),
            "tactical" => tactical_budget(b, c, s, n),
            other => panic!("unknown row kind {other}"),
        };
        let want = BudgetPlan {
            candidates: cu as u32,
            samples: su as u32,
            horizon: h as u32,
        };
        assert_eq!(got, want, "{kind} {b} {c} {s} {n}");
        assert_eq!(got, plan_budget_again(&kind, b, c, s, n), "repeats");
    }
}

fn plan_budget_again(kind: &str, b: u64, c: u32, s: u32, n: u32) -> BudgetPlan {
    if kind == "plan" {
        plan_budget(b, c, s, n)
    } else {
        tactical_budget(b, c, s, n)
    }
}

#[test]
fn the_horizon_is_always_inside_its_bounds_and_the_cost_fits_or_cannot() {
    assert_eq!((HORIZON_MIN, HORIZON_MAX), (100, 1200));
    for budget in [0u64, 1, 999_999, 2_000_000, 12_000_000, u64::MAX] {
        for c in 1..=4u32 {
            for s in 1..=3u32 {
                for n in [0u32, 1, 500, 3000, 60_000] {
                    let p = plan_budget(budget, c, s, n);
                    assert!((HORIZON_MIN..=HORIZON_MAX).contains(&p.horizon));
                    assert!(p.candidates >= 1 && p.candidates <= c);
                    assert!(p.samples >= 1 && p.samples <= s);
                    let cost = u128::from(p.horizon)
                        * u128::from(p.candidates)
                        * u128::from(p.samples)
                        * u128::from(n.max(1));
                    let fits = cost <= u128::from(budget);
                    let floor_case =
                        p.horizon == HORIZON_MIN && p.candidates == 1 && p.samples == 1;
                    assert!(fits || floor_case, "{budget} {c} {s} {n} -> {p:?}");
                }
            }
        }
    }
}
