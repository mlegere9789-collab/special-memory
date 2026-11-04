//! T4-10: closing up. After 40 men leave a 400-man line, the next close-up tick gives 360 slots and
//! every remaining soldier a distinct slot (SPEC-004 section 8).

mod t018_support;

use battle_rules::{slot_layout, Formation, Order, RegimentState, Training, F_DISABLED, F_WOUNDED};
use sim_state::AgentId;
use std::collections::BTreeSet;
use t018_support::{battle_with, step};

fn assigned(b: &battle_rules::BattleState) -> Vec<(i64, i64)> {
    b.regiments[0]
        .roster
        .iter()
        .map(|a| {
            let s = b.loco[a.index as usize].slot;
            (s.u.raw(), s.v.raw())
        })
        .collect()
}

#[test]
fn forty_despawned_from_a_line_close_up_to_360_distinct_slots() {
    let (mut b, _) = battle_with(400, Training::Trained, Formation::Line);
    step(&mut b); // tick 0 has nothing to do; the world is now at tick 1
    let victims: Vec<AgentId> = b.regiments[0].roster.iter().copied().step_by(10).collect();
    assert_eq!(victims.len(), 40);
    for v in &victims {
        assert!(b.world.agents.despawn(*v));
    }
    let interval = u64::from(b.tables.drill.globals.closeup_interval_ticks);
    // Off the interval the roster shrinks and the regiment is dirty, but the layout stands.
    while b.world.tick % interval != 0 {
        step(&mut b);
        assert_eq!(b.regiments[0].roster.len(), 360);
        assert!(b.regiments[0].dirty);
        assert_eq!(b.regiments[0].slots.len(), 400);
    }
    // The close-up tick.
    step(&mut b);
    let r = &b.regiments[0];
    assert!(!r.dirty);
    assert_eq!(r.slots.len(), 360);
    assert_eq!(r.slots, slot_layout(Formation::Line, 360, &b.tables.drill));
    let got = assigned(&b);
    assert_eq!(got.len(), 360);
    let distinct: BTreeSet<(i64, i64)> = got.iter().copied().collect();
    assert_eq!(distinct.len(), 360, "every soldier has a distinct slot");
    let layout: BTreeSet<(i64, i64)> = r.slots.iter().map(|s| (s.u.raw(), s.v.raw())).collect();
    assert_eq!(distinct, layout, "the assignment covers the whole layout");
    // 360 men in two ranks of 180.
    assert_eq!(r.slots.iter().filter(|s| s.v.raw() == 0).count(), 180);
}

#[test]
fn closeup_happens_on_the_first_tick_divisible_by_the_interval() {
    let (mut b, _) = battle_with(400, Training::Trained, Formation::Line);
    b.world.tick = 41;
    let victim = b.regiments[0].roster[5];
    b.world.agents.despawn(victim);
    let mut closed_at = None;
    for _ in 0..80 {
        let tick = b.world.tick;
        step(&mut b);
        if closed_at.is_none() && b.regiments[0].slots.len() == 399 {
            closed_at = Some(tick);
        }
    }
    assert_eq!(closed_at, Some(80));
}

#[test]
fn disabled_and_wounded_soldiers_leave_the_roster() {
    let (mut b, _) = battle_with(100, Training::Trained, Formation::Column);
    b.world.tick = 1;
    let [a, w, ok]: [AgentId; 3] = [10, 20, 30].map(|i| b.regiments[0].roster[i]);
    b.world.agents.set_flags(a, F_DISABLED);
    b.world.agents.set_flags(w, F_WOUNDED);
    step(&mut b);
    let roster = &b.regiments[0].roster;
    assert_eq!(roster.len(), 98);
    assert!(!roster.contains(&a) && !roster.contains(&w) && roster.contains(&ok));
    assert!(roster.windows(2).all(|p| p[0] < p[1]), "ascending");
    assert!(b.regiments[0].dirty);
    while b.world.tick % 40 != 0 {
        step(&mut b);
    }
    step(&mut b);
    assert_eq!(b.regiments[0].slots.len(), 98);
    assert_eq!(assigned(&b).into_iter().collect::<BTreeSet<_>>().len(), 98);
}

#[test]
fn losses_while_forming_close_up_into_the_target_formation() {
    let (mut b, id) = battle_with(400, Training::Raw, Formation::Column);
    b.issue_order(id, Order::Form(Formation::Line));
    let victims: Vec<AgentId> = b.regiments[0].roster.iter().copied().step_by(10).collect();
    for v in victims {
        b.world.agents.despawn(v);
    }
    step(&mut b); // tick 0 is a close-up tick
    let r = &b.regiments[0];
    assert!(matches!(r.state, RegimentState::Forming { .. }));
    assert_eq!(r.slots, slot_layout(Formation::Line, 360, &b.tables.drill));
    let distinct: BTreeSet<(i64, i64)> = assigned(&b).into_iter().collect();
    assert_eq!(distinct.len(), 360);
}
