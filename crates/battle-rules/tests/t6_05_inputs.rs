//! T6-05, inputs: every penalty and bonus of SPEC-006 section 5.2 equals its formula, sampled
//! from the frozen state of the battle.

mod t6_support;

use battle_rules::hit::Obstacle;
use battle_rules::morale::{Exposure, Leader, MoraleInputs};
use battle_rules::{Formation, Gait, Order, RegimentId, Training};
use core_math::Angle;
use t6_support::{field, line, m, spawn};

const R0: RegimentId = RegimentId(0);

fn inputs(b: &battle_rules::BattleState, id: RegimentId) -> MoraleInputs {
    b.morale_inputs(id).unwrap()
}

fn globals(b: &battle_rules::BattleState) -> battle_rules::morale::MoraleGlobals {
    b.tables.morale.globals.clone()
}

#[test]
fn neutral_regiment_has_only_the_empty_magazine_term() {
    let mut b = field(1, '.');
    line(&mut b, 0, 40, m(10, 10));
    b.enable_morale();
    let g = globals(&b);
    let i = inputs(&b, R0);
    // No weapon is issued, so `rounds_full` is 0, the share is 0 and the full penalty applies.
    assert_eq!(i.ammo, g.ammo_pen);
    assert_eq!(
        (i.fatigue, i.supply, i.flank, i.charge, i.disorder, i.cover, i.wall, i.leader),
        (0, 0, 0, 0, 0, 0, 0, 0)
    );
    assert!(!i.pressure);
    assert_eq!((i.leader_pm, i.dominant), (0, None));
}

#[test]
fn fatigue_penalty_follows_the_mean_above_the_knee() {
    let mut b = field(1, '.');
    let r = line(&mut b, 0, 40, m(10, 10));
    b.enable_morale();
    let g = globals(&b);
    let ids = b.regiments[0].roster.clone();
    for (k, a) in ids.iter().enumerate() {
        b.loco[a.index as usize].fatigue = if k % 2 == 0 { 900_001 } else { 600_000 };
    }
    let mean = (20 * 900_001u64 + 20 * 600_000) / 40;
    let knee = u64::from(g.fatigue_knee);
    let want = u64::from(g.fatigue_pen) * (mean - knee) / (1_000_000 - knee);
    assert_eq!(u64::from(inputs(&b, r).fatigue), want);
    // At or below the knee there is no penalty.
    for a in &ids {
        b.loco[a.index as usize].fatigue = g.fatigue_knee;
    }
    assert_eq!(inputs(&b, r).fatigue, 0);
}

#[test]
fn ammunition_penalty_follows_the_share_below_the_knee() {
    let mut b = field(1, '.');
    let r = line(&mut b, 0, 40, m(10, 10));
    b.equip_regiment(r, "rifle_58", 0).unwrap();
    b.enable_morale();
    let g = globals(&b);
    // Full magazines: no penalty.
    assert_eq!(inputs(&b, r).ammo, 0);
    let ids = b.regiments[0].roster.clone();
    for a in &ids {
        b.fire[a.index as usize].rounds = 6; // 10 percent of 60
    }
    let share = 1000 * 240u64 / 2400;
    let knee = u64::from(g.ammo_knee_pm);
    let want = u64::from(g.ammo_pen) * (knee - share) / knee;
    assert_eq!(u64::from(inputs(&b, r).ammo), want);
    for a in &ids {
        b.fire[a.index as usize].rounds = 0;
    }
    assert_eq!(inputs(&b, r).ammo, g.ammo_pen);
}

#[test]
fn supply_penalty_is_linear_in_the_shortfall() {
    let mut b = field(1, '.');
    let r = line(&mut b, 0, 40, m(10, 10));
    b.enable_morale();
    let g = globals(&b);
    assert!(b.set_supply(r, 400));
    assert_eq!(inputs(&b, r).supply, g.supply_pen * 600 / 1000);
    assert!(b.set_supply(r, 0));
    assert_eq!(inputs(&b, r).supply, g.supply_pen);
    assert!(b.set_supply(r, 1000));
    assert_eq!(inputs(&b, r).supply, 0);
    assert!(!b.set_supply(RegimentId(9), 500));
}

#[test]
fn woods_give_cover_and_disorder() {
    let mut b = field(1, 'W');
    line(&mut b, 0, 40, m(10, 10));
    b.enable_morale();
    let g = globals(&b);
    let class = b.tables.terrain.class(3).unwrap();
    let i = inputs(&b, R0);
    assert_eq!(i.cover, g.cover_bonus * class.cover_pm / 1000);
    assert_eq!(i.disorder, g.disorder_pen * class.line_disorder_pm / 1000);
    assert!(i.cover > 0 && i.disorder > 0);
}

fn wall(b: &mut battle_rules::BattleState, x: i32, height_mm: u16) {
    b.obstacles.segments = vec![Obstacle {
        a: m(x, 0),
        b: m(x, 20),
        height_mm,
    }];
}

#[test]
fn a_wall_ahead_within_reach_gives_the_bonus() {
    let mut b = field(1, '.');
    line(&mut b, 0, 40, m(10, 10));
    b.enable_morale();
    let g = globals(&b);
    wall(&mut b, 12, 600);
    assert_eq!(inputs(&b, R0).wall, g.wall_bonus);
    wall(&mut b, 12, 599);
    assert_eq!(inputs(&b, R0).wall, 0, "too low");
    wall(&mut b, 8, 900);
    assert_eq!(inputs(&b, R0).wall, 0, "behind the line");
    wall(&mut b, 15, 900);
    assert_eq!(inputs(&b, R0).wall, 0, "out of reach");
    wall(&mut b, 14, 900);
    assert_eq!(inputs(&b, R0).wall, g.wall_bonus, "at the edge of reach");
    wall(&mut b, 10, 900);
    assert_eq!(inputs(&b, R0).wall, g.wall_bonus, "on the anchor line");
}

#[test]
fn exposure_share_is_weighted_by_roster_and_fire() {
    let mut b = field(1, '.');
    line(&mut b, 0, 40, m(10, 10));
    // Front, line of 40: weight 40, coefficient 0.
    line(&mut b, 1, 40, m(110, 10));
    // Along the line (enfilade): a column of 40 that cannot fire.
    spawn(
        &mut b,
        1,
        40,
        Training::Trained,
        Formation::Column,
        m(10, 110),
        Angle::ZERO,
    );
    b.enable_morale();
    let g = globals(&b);
    let i = inputs(&b, R0);
    // Column cannot fire: weight 40 / 4 = 10. Enfilade coefficient applies.
    let want = u64::from(g.flank_pen) * (10 * u64::from(g.enfilade_f_pm) / 50) / 1000;
    assert_eq!(u64::from(i.flank), want);
    assert_eq!(i.dominant, Some(Exposure::Front));
}

#[test]
fn flank_and_rear_use_their_own_coefficients() {
    // One enemy line of 40 at a time, 100 m away; (f, r) in metres relative to an east facing.
    for (dx, dy, coeff_of) in [
        (100, 0, 0usize),
        (-42, 91, 1), // bearing 115: flank
        (0, 100, 2),  // 90: enfilade
        (-100, 0, 3), // rear
    ] {
        let mut b = field(1, '.');
        line(&mut b, 0, 40, m(100, 100));
        line(&mut b, 1, 40, m(100 + dx, 100 + dy));
        b.enable_morale();
        let g = globals(&b);
        let f = [0, g.flank_f_pm, g.enfilade_f_pm, g.rear_f_pm][coeff_of];
        let class = [
            Exposure::Front,
            Exposure::Flank,
            Exposure::Enfilade,
            Exposure::Rear,
        ][coeff_of];
        let i = inputs(&b, R0);
        assert_eq!(i.flank, g.flank_pen * f / 1000, "enemy at ({dx}, {dy})");
        assert_eq!(i.dominant, Some(class));
    }
}

#[test]
fn enemies_beyond_threat_range_do_not_count() {
    let mut b = field(1, '.');
    line(&mut b, 0, 40, m(10, 10));
    line(&mut b, 1, 40, m(510, 10)); // 500 m, beyond 400 m
    b.enable_morale();
    let i = inputs(&b, R0);
    assert_eq!((i.flank, i.dominant, i.pressure), (0, None, false));
}

#[test]
fn pressure_is_any_enemy_inside_the_radius() {
    let mut b = field(1, '.');
    line(&mut b, 0, 40, m(10, 10));
    let e = line(&mut b, 1, 40, m(119, 10)); // 109 m, within 120 m
    line(&mut b, 0, 40, m(300, 300)); // a friend never presses
    b.enable_morale();
    assert!(inputs(&b, R0).pressure);
    assert!(!inputs(&b, RegimentId(2)).pressure);
    b.regiments[usize::from(e.0)].anchor = m(131, 10);
    assert!(!inputs(&b, R0).pressure);
}

fn charger(order: Order, heading: Angle, count: usize, formation: Formation) -> MoraleInputs {
    let mut b = field(1, '.');
    line(&mut b, 0, 20, m(100, 100));
    let e = spawn(
        &mut b,
        1,
        count,
        Training::Trained,
        formation,
        m(130, 100),
        heading,
    );
    assert!(b.issue_order(e, order));
    b.enable_morale();
    inputs(&b, R0)
}

#[test]
fn a_charge_scales_with_the_charger_length() {
    let go = Order::Move {
        to: m(100, 100),
        gait: Gait::Run,
    };
    let g = {
        let b = field(1, '.');
        globals(&b)
    };
    // Facing west, towards the regiment at its west: 40 against 20 is capped at full strength.
    let full = charger(go, Angle::HALF, 40, Formation::Line);
    assert_eq!(full.charge, g.charge_pen);
    let half = charger(go, Angle::HALF, 10, Formation::Column);
    assert_eq!(half.charge, g.charge_pen * 500 / 1000);
    // Facing away, holding, or in skirmish order: no charge.
    assert_eq!(charger(go, Angle::ZERO, 40, Formation::Line).charge, 0);
    assert_eq!(
        charger(Order::Hold, Angle::HALF, 40, Formation::Line).charge,
        0
    );
    assert_eq!(charger(go, Angle::HALF, 40, Formation::Skirmish).charge, 0);
}

#[test]
fn the_largest_charge_applies() {
    let mut b = field(1, '.');
    line(&mut b, 0, 40, m(100, 100));
    let go = Order::Move {
        to: m(100, 100),
        gait: Gait::Run,
    };
    for (n, at) in [(10usize, m(120, 100)), (20, m(130, 100))] {
        let e = spawn(
            &mut b,
            1,
            n,
            Training::Trained,
            Formation::Line,
            at,
            Angle::HALF,
        );
        assert!(b.issue_order(e, go));
    }
    b.enable_morale();
    let g = globals(&b);
    assert_eq!(inputs(&b, R0).charge, g.charge_pen * 500 / 1000);
}

#[test]
fn the_best_living_friendly_leader_in_reach_counts() {
    let mut b = field(1, '.');
    line(&mut b, 0, 40, m(100, 100));
    b.enable_morale();
    let g = globals(&b);
    let at = |x: i32| m(100 + x, 100);
    let leader = |id, team, x, prestige_pm, alive| Leader {
        id,
        team,
        pos: at(x),
        prestige_pm,
        alive,
    };
    assert!(b.set_leader(leader(3, 0, 10, 400, true)));
    assert!(b.set_leader(leader(1, 0, 50, 700, true)));
    assert!(b.set_leader(leader(2, 0, 5, 900, false)));
    assert!(b.set_leader(leader(4, 1, 5, 950, true)));
    assert!(b.set_leader(leader(5, 0, 61, 990, true)));
    let ids: Vec<u16> = b
        .morale
        .as_ref()
        .unwrap()
        .leaders
        .iter()
        .map(|l| l.id)
        .collect();
    assert_eq!(ids, [1, 2, 3, 4, 5], "ascending id");
    let i = inputs(&b, R0);
    assert_eq!(i.leader_pm, 700);
    assert_eq!(i.leader, g.leader_bonus * 700 / 1000);
    // Setting an id again replaces it.
    assert!(b.set_leader(leader(1, 0, 50, 100, true)));
    assert_eq!(b.morale.as_ref().unwrap().leaders.len(), 5);
    assert_eq!(inputs(&b, R0).leader_pm, 400);
}

#[test]
fn unknown_regiment_or_disabled_morale_gives_nothing() {
    let mut b = field(1, '.');
    line(&mut b, 0, 40, m(10, 10));
    assert!(b.morale_inputs(R0).is_none(), "morale not enabled");
    assert!(!b.set_supply(R0, 10));
    assert!(!b.set_leader(Leader {
        id: 0,
        team: 0,
        pos: m(0, 0),
        prestige_pm: 1,
        alive: true
    }));
    b.enable_morale();
    assert!(b.morale_inputs(RegimentId(5)).is_none());
}
