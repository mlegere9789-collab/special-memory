//! T10-07: with a belief equal to truth, `forecast` over N ticks ends on the played run's hash.

mod scene;

use battle_ai::{
    build_belief, forecast, ContactId, EnemyModel, Frame, Script, ScriptItem, UnitRef,
    EST_LATENCY_TICKS,
};
use battle_rules::{BattleState, Gait, Order, RegimentId};
use core_math::{Rng, SplitMix64};
use scene::*;

/// The master seed `build_belief` takes from `rng`.
fn master_seed(rng: &Rng) -> u64 {
    SplitMix64::new(rng.clone().next_u64()).next_u64()
}

fn truth(rng: &Rng) -> (BattleState, battle_ai::MapView) {
    let t = tables();
    let mp = map(&t);
    let mut b = world(master_seed(rng), &t, &mp);
    let a = spawn(&mut b, 0, 100, m(100, 100), battle_rules_zero());
    spawn(&mut b, 0, 80, m(100, 140), battle_rules_zero());
    spawn(
        &mut b,
        1,
        90,
        m(260, 100),
        toward(m(260, 100), area().center),
    );
    spawn(
        &mut b,
        1,
        70,
        m(260, 140),
        toward(m(260, 140), area().center),
    );
    assert!(b.set_fatigue(a, 100_000));
    assert!(b.set_rounds_pm(a, 700));
    (b, mp)
}

fn battle_rules_zero() -> core_math::Angle {
    core_math::Angle::ZERO
}

#[test]
fn a_belief_equal_to_truth_hashes_as_truth() {
    for seed in 1..=5u64 {
        let rng = Rng::from_seed(seed);
        let (b, mp) = truth(&rng);
        let t = tables();
        let pic = picture(&b, 0, &[2, 3]);
        let v = view(0, b.world.tick, (&pic.0, &pic.1), &[], &mp, &t);
        let belief = build_belief(&v, &mut rng.clone(), 0);
        assert_eq!(belief.hash(), b.hash(), "seed {seed}");
    }
}

fn script() -> Script {
    Script {
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
                target: Some(ContactId(502)),
            },
            ScriptItem::Fire {
                unit: UnitRef(1),
                target: Some(ContactId(503)),
            },
        ],
    }
}

/// The played run: the same orders through the rules' own entry points.
fn played(mut b: BattleState, model: EnemyModel, horizon: u32) -> BattleState {
    for foe in [RegimentId(2), RegimentId(3)] {
        let near = nearest_foe(&b, foe).expect("an own regiment");
        match model {
            EnemyModel::Hold => assert!(b.issue_order(foe, Order::Hold)),
            EnemyModel::Advance => {
                let to = b.regiments[usize::from(near.0)].anchor;
                assert!(b.issue_order(
                    foe,
                    Order::Move {
                        to,
                        gait: Gait::Quick
                    }
                ));
            }
            EnemyModel::Continue => unreachable!("no track in this scene"),
        }
        at_will(&mut b, foe, near);
    }
    for tick in 0..u64::from(horizon) {
        if tick == EST_LATENCY_TICKS {
            assert!(b.issue_order(
                RegimentId(0),
                Order::Move {
                    to: m(200, 100),
                    gait: Gait::Quick
                }
            ));
            at_will(&mut b, RegimentId(0), RegimentId(2));
            at_will(&mut b, RegimentId(1), RegimentId(3));
        }
        b.step();
    }
    b
}

#[test]
fn forecast_hash_equals_the_played_run() {
    for (seed, model) in [
        (1u64, EnemyModel::Hold),
        (2, EnemyModel::Advance),
        (3, EnemyModel::Hold),
    ] {
        let rng = Rng::from_seed(seed);
        let (b, mp) = truth(&rng);
        let t = tables();
        let pic = picture(&b, 0, &[2, 3]);
        let v = view(0, b.world.tick, (&pic.0, &pic.1), &[], &mp, &t);
        let frame = Frame::of(&v);
        let belief = build_belief(&v, &mut rng.clone(), 0);
        let horizon = 900;
        let r = forecast(&belief, &frame, &script(), horizon, model, None);
        let run = played(b, model, horizon);
        assert_eq!(r.final_hash, run.hash(), "seed {seed} {model:?}");
        assert!(
            r.own_lost + r.enemy_lost > 0,
            "the scene must fight, or the check proves little"
        );
        let again = forecast(&belief, &frame, &script(), horizon, model, None);
        assert_eq!(again, r, "a forecast repeats");
    }
}

#[test]
fn forecast_leaves_the_belief_alone() {
    let rng = Rng::from_seed(9);
    let (b, mp) = truth(&rng);
    let t = tables();
    let pic = picture(&b, 0, &[2, 3]);
    let v = view(0, 0, (&pic.0, &pic.1), &[], &mp, &t);
    let belief = build_belief(&v, &mut rng.clone(), 0);
    let before = belief.hash();
    let _ = forecast(
        &belief,
        &Frame::of(&v),
        &script(),
        150,
        EnemyModel::Advance,
        None,
    );
    assert_eq!(belief.hash(), before);
}

#[test]
fn a_reseeded_forecast_differs_and_repeats() {
    let rng = Rng::from_seed(4);
    let (b, mp) = truth(&rng);
    let t = tables();
    let pic = picture(&b, 0, &[2, 3]);
    let v = view(0, 0, (&pic.0, &pic.1), &[], &mp, &t);
    let (frame, belief) = (Frame::of(&v), build_belief(&v, &mut rng.clone(), 0));
    let go = |seed| forecast(&belief, &frame, &script(), 300, EnemyModel::Advance, seed);
    assert_eq!(go(Some(7)), go(Some(7)));
    assert_ne!(go(Some(7)).final_hash, go(Some(8)).final_hash);
    assert_ne!(go(Some(7)).final_hash, go(None).final_hash);
}

#[test]
fn continue_follows_the_track_and_without_one_is_hold() {
    let rng = Rng::from_seed(6);
    let (b, mp) = truth(&rng);
    let t = tables();
    let pic = picture(&b, 0, &[2, 3]);
    let previous: Vec<_> = pic
        .1
        .iter()
        .map(|c| {
            let mut p = *c;
            p.pos = m(c.pos.x.to_int_floor() + 30, c.pos.y.to_int_floor());
            p.last_seen = 0;
            p
        })
        .collect();
    let mut now = pic.1.clone();
    for c in &mut now {
        c.last_seen = 100;
    }
    let go = |prev: &[battle_ai::Contact], model| {
        let v = view(0, 0, (&pic.0, &now), prev, &mp, &t);
        let belief = build_belief(&v, &mut rng.clone(), 0);
        forecast(
            &belief,
            &Frame::of(&v),
            &Script::default(),
            200,
            model,
            None,
        )
        .final_hash
    };
    assert_eq!(go(&[], EnemyModel::Continue), go(&[], EnemyModel::Hold));
    assert_ne!(
        go(&previous, EnemyModel::Continue),
        go(&[], EnemyModel::Hold)
    );
    assert_eq!(EnemyModel::of_sample(0), EnemyModel::Continue);
    assert_eq!(EnemyModel::of_sample(4), EnemyModel::Advance);
    assert_eq!(EnemyModel::of_sample(5), EnemyModel::Hold);
}
