//! T10-03, first version: replacing every enemy unit that has no contact by a randomly placed
//! one leaves the belief and every forecast byte-identical, on a fixed two-regiment scene.

mod scene;

use battle_ai::{build_belief, forecast, EnemyModel, Frame, Script, ScriptItem, UnitRef};
use battle_rules::{Gait, Order};
use core_math::Rng;
use scene::*;

/// Truth: two own regiments, one seen enemy (regiment 2) and one unseen (regiment 3) at `hidden`.
fn outputs(seed: u64, hidden: core_math::Vec2Fx, seen_at: core_math::Vec2Fx) -> Vec<u64> {
    let t = tables();
    let mp = map(&t);
    let mut b = world(1, &t, &mp);
    spawn(&mut b, 0, 100, m(100, 100), core_math::Angle::ZERO);
    spawn(&mut b, 0, 80, m(100, 140), core_math::Angle::ZERO);
    spawn(&mut b, 1, 90, seen_at, toward(seen_at, area().center));
    spawn(&mut b, 1, 120, hidden, core_math::Angle::HALF);
    let pic = picture(&b, 0, &[2]);
    let v = view(0, 0, (&pic.0, &pic.1), &[], &mp, &t);
    let frame = Frame::of(&v);
    let script = Script {
        items: vec![
            ScriptItem::Order {
                unit: UnitRef(0),
                order: Order::Move {
                    to: m(200, 100),
                    gait: Gait::Quick,
                },
            },
            ScriptItem::Fire {
                unit: UnitRef(0),
                target: Some(battle_ai::ContactId(502)),
            },
        ],
    };
    let mut rng = Rng::from_seed(seed);
    let belief = build_belief(&v, &mut rng, 150);
    let mut out = vec![belief.hash(), rng.next_u64()];
    for model in [EnemyModel::Continue, EnemyModel::Advance, EnemyModel::Hold] {
        let r = forecast(&belief, &frame, &script, 250, model, None);
        out.extend([r.final_hash, r.own_lost, r.enemy_lost, u64::from(r.ctrl_pm)]);
    }
    out
}

#[test]
fn unseen_enemies_do_not_reach_the_belief_or_the_forecast() {
    let seen = m(260, 100);
    for seed in 1..=5u64 {
        let reference = outputs(seed, m(250, 140), seen);
        let mut place = Rng::from_seed(1000 + seed);
        for _ in 0..8 {
            let hidden = m(place.range_i32(0, 2000), place.range_i32(0, 500));
            assert_eq!(outputs(seed, hidden, seen), reference, "seed {seed}");
        }
    }
}

#[test]
fn the_check_is_sensitive_to_a_seen_enemy() {
    let a = outputs(1, m(250, 140), m(260, 100));
    let b = outputs(1, m(250, 140), m(240, 100));
    assert_ne!(a, b);
}
