//! T5-14: per-soldier streams. Adding or removing an unrelated soldier leaves another soldier's
//! first ten shots (seeds and launch vectors) unchanged (SPEC-005 section 4).

mod t022_support;

use battle_rules::{BattleState, Training};
use core_math::Angle;
use t022_support::{duel, line, metres, no_misfire, tick};

/// `(seed, pos, vel, vz, fuse)` of the first ten projectiles of soldier `index`.
fn first_ten(b: &mut BattleState, index: u32) -> Vec<String> {
    for _ in 0..8000 {
        tick(b);
        if b.pool.iter().filter(|p| p.owner == index).count() >= 10 {
            break;
        }
    }
    let v: Vec<String> = b
        .pool
        .iter()
        .filter(|p| p.owner == index)
        .take(10)
        .map(|p| format!("{} {:?} {:?} {:?} {}", p.seed, p.pos, p.vel, p.vz, p.fuse))
        .collect();
    assert_eq!(v.len(), 10);
    v
}

fn base() -> BattleState {
    let (mut b, _, _) = duel(21, "rifle_58", 0, Training::Trained, 5, 120);
    no_misfire(&mut b);
    b
}

#[test]
fn an_unrelated_soldier_does_not_change_another_soldiers_shots() {
    let reference = first_ten(&mut base(), 2);

    // Added: a third regiment, equipped, far from everything.
    let mut b = base();
    let far = b.spawn_regiment(line(0, 7, Training::Raw, metres(1500, 50), Angle::HALF));
    b.equip_regiment(far, "smoothbore_69", 0).unwrap();
    assert_eq!(first_ten(&mut b, 2), reference);

    // Removed: one of those soldiers leaves again.
    let mut b = base();
    let far = b.spawn_regiment(line(0, 7, Training::Raw, metres(1500, 50), Angle::HALF));
    b.equip_regiment(far, "smoothbore_69", 0).unwrap();
    let gone = *b.regiments[usize::from(far.0)].roster.last().unwrap();
    assert!(b.world.agents.despawn(gone));
    assert_eq!(first_ten(&mut b, 2), reference);
}

#[test]
fn different_soldiers_draw_different_shots() {
    let mut b = base();
    let a = first_ten(&mut b, 1);
    let c = first_ten(&mut b, 3);
    assert_ne!(a, c);
}
