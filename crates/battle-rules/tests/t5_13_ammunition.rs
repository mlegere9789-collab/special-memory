//! T5-13: ammunition. `n` rounds give exactly `n` shots or misfires, then the soldier stops;
//! `resupply` restarts; nothing underflows (SPEC-005 section 4).

mod t022_support;

use battle_rules::Training;
use t022_support::{duel, tick};

#[test]
fn n_rounds_give_n_shots_or_misfires_then_stop_and_resupply_restarts() {
    let (mut b, shooters, _) = duel(9, "smoothbore_69", 0, Training::Veteran, 20, 40);
    for f in b.fire.iter_mut().take(20) {
        f.rounds = 3;
    }
    for _ in 0..3000 {
        tick(&mut b);
    }
    let c = b.counters();
    assert_eq!(c.shots_fired, 60);
    assert_eq!(c.projectiles_spawned + c.misfires, 60);
    assert_eq!(b.pool.len() as u64, c.projectiles_spawned);
    assert!(b.fire.iter().take(20).all(|f| f.rounds == 0));
    for _ in 0..1000 {
        tick(&mut b);
    }
    assert_eq!(b.counters().shots_fired, 60, "no shot without rounds");
    assert!(
        b.fire.iter().take(20).all(|f| f.rounds == 0),
        "no underflow"
    );

    assert!(b.resupply(shooters, 2));
    assert!(b.fire.iter().take(20).all(|f| f.rounds == 2));
    for _ in 0..3000 {
        tick(&mut b);
    }
    assert_eq!(b.counters().shots_fired, 60 + 40);
    assert!(b.fire.iter().take(20).all(|f| f.rounds == 0));
}

#[test]
fn equipping_gives_the_weapon_table_rounds_for_the_selected_ammunition() {
    let (b, _, _) = duel(1, "napoleon_12", 3, Training::Trained, 4, 100);
    assert!(b
        .fire
        .iter()
        .take(4)
        .all(|f| f.rounds == 6 && f.ammo_sel == 3));
    let (b, _, _) = duel(1, "rifle_58", 0, Training::Trained, 4, 100);
    assert!(b.fire.iter().take(4).all(|f| f.rounds == 60));
}

#[test]
fn unknown_weapon_or_ammunition_is_refused() {
    let (mut b, shooters, _) = duel(1, "rifle_58", 0, Training::Trained, 2, 100);
    assert!(b.equip_regiment(shooters, "no_such_gun", 0).is_err());
    assert!(b.equip_regiment(shooters, "rifle_58", 9).is_err());
}
