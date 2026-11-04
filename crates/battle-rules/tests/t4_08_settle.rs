//! T4-08: in `deploy_pair`, by tick `T + settle_budget_ticks` every soldier is within
//! `settle_tol_mm` of its slot, for both regiments (SPEC-004 section 13).

mod t019_support;

use battle_rules::{scenarios, transition_ticks, Formation, RegimentState};
use t019_support::{max_slot_error, mm, tables};

#[test]
fn both_regiments_settle_within_the_budget() {
    let t = tables();
    let g = &t.drill.globals;
    let (tol, budget) = (mm(g.settle_tol_mm), u64::from(g.settle_budget_ticks));
    let mut b = scenarios::deploy_pair(&t, 2);
    let ends: Vec<u64> = b
        .regiments
        .iter()
        .map(|r| {
            u64::from(transition_ticks(
                Formation::Column,
                Formation::Line,
                r.training,
                &t.drill,
            ))
        })
        .collect();
    let last = ends.iter().max().unwrap() + budget;
    let mut first_settled = [None; 2];
    while b.world.tick < last {
        b.step_movement();
        for r in 0..2 {
            let done = matches!(b.regiments[r].state, RegimentState::Steady(Formation::Line));
            if first_settled[r].is_none() && done && max_slot_error(&b, r) <= tol {
                first_settled[r] = Some(b.world.tick);
            }
            if b.world.tick == ends[r] + budget {
                assert!(
                    max_slot_error(&b, r) <= tol,
                    "regiment {r} unsettled at T + budget"
                );
            }
        }
    }
    println!("transition ticks {ends:?}, first settled {first_settled:?}");
    for r in 0..2 {
        assert!(first_settled[r].is_some_and(|s| s <= ends[r] + budget));
        assert!(max_slot_error(&b, r) <= tol);
    }
}
