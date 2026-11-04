//! T7-06: garbled share, kind shares, displacement deviation, and consistency of the record with
//! what the reader saw.

mod t73_support;

use battle_rules::command::{read_order, GarbleKind, Medium, OfficerId, OrderBody};
use battle_rules::{Formation, Order};
use core_math::Rng;
use t73_support::*;

const N: u32 = 20_000;

fn sigma3(count: u32, n: u32, p: f64) {
    let (mean, sd) = (f64::from(n) * p, (f64::from(n) * p * (1.0 - p)).sqrt());
    assert!(
        (f64::from(count) - mean).abs() <= 3.0 * sd,
        "{count} vs {mean} +- {sd}"
    );
}

#[test]
fn garbled_share_and_kind_shares_of_moves() {
    let t = table();
    let g = &t.globals;
    let (mut garbled, mut disp, mut gait, mut stall) = (0u32, 0u32, 0u32, 0u32);
    let (mut sum_sq, mut sum_n) = (0i128, 0i128);
    let sent = walk_to(100, 100);
    for seed in 0..u64::from(N) {
        let mut rng = Rng::from_seed(seed);
        let (got, kind) = read_order(&t, Medium::Courier, &mut rng, sent);
        match kind {
            None => assert_eq!(got, sent),
            Some(k) => {
                garbled += 1;
                match k {
                    GarbleKind::Displaced => {
                        disp += 1;
                        let (Order::Move { to, .. }, Order::Move { to: was, .. }) =
                            (got.order, sent.order)
                        else {
                            panic!()
                        };
                        for d in [to.x - was.x, to.y - was.y] {
                            let mm = (i128::from(d.raw()) * 1000) >> 32;
                            sum_sq += mm * mm;
                            sum_n += 1;
                        }
                    }
                    GarbleKind::GaitShifted => gait += 1,
                    GarbleKind::Stalled => {
                        stall += 1;
                        assert_eq!(got.order, Order::Hold);
                    }
                    other => panic!("{other:?} cannot garble a move"),
                }
            }
        }
    }
    let p = f64::from(t.media[0].garble_ppm) / 1e6;
    sigma3(garbled, N, p);
    let (pd, pg) = (
        f64::from(g.garble_move_displace_pm) / 1000.0,
        f64::from(g.garble_move_gait_pm) / 1000.0,
    );
    sigma3(disp, garbled, pd);
    sigma3(gait, garbled, pg - pd);
    sigma3(stall, garbled, 1.0 - pg);
    let dev = ((sum_sq / sum_n) as f64).sqrt();
    let sigma = g.garble_sigma_mm as f64;
    assert!(
        (dev - sigma).abs() <= 0.1 * sigma,
        "deviation {dev} vs {sigma}"
    );
}

#[test]
fn hold_is_never_garbled_and_forms_and_faces_follow_their_thresholds() {
    let mut t = table();
    t.media[0].garble_ppm = 1_000_000;
    for seed in 0..2000u64 {
        let mut rng = Rng::from_seed(seed);
        assert_eq!(
            read_order(&t, Medium::Courier, &mut rng, hold()),
            (hold(), None)
        );
    }
    let form = OrderBody {
        order: Order::Form(Formation::Line),
        ..hold()
    };
    let (mut swapped, mut stalled) = (0u32, 0u32);
    for seed in 0..2000u64 {
        let mut rng = Rng::from_seed(seed);
        let (got, kind) = read_order(&t, Medium::Courier, &mut rng, form);
        match kind {
            Some(GarbleKind::FormSwapped) => {
                swapped += 1;
                assert!(matches!(got.order, Order::Form(f) if f != Formation::Line));
            }
            Some(GarbleKind::Stalled) => stalled += 1,
            other => panic!("{other:?}"),
        }
    }
    sigma3(
        swapped,
        2000,
        f64::from(t.globals.garble_form_swap_pm) / 1000.0,
    );
    assert_eq!(swapped + stalled, 2000);
    let mut turned = 0u32;
    for seed in 0..2000u64 {
        let mut rng = Rng::from_seed(seed);
        match read_order(&t, Medium::Courier, &mut rng, face(10)).1 {
            Some(GarbleKind::Turned) => turned += 1,
            Some(GarbleKind::Stalled) => {}
            other => panic!("{other:?}"),
        }
    }
    sigma3(
        turned,
        2000,
        f64::from(t.globals.garble_face_turn_pm) / 1000.0,
    );
}

#[test]
fn reading_at_arrival_matches_the_record() {
    let mut t = table();
    t.media[0].loss_base_ppm = 0;
    t.globals.loss_ppm_per_km = 0;
    t.media[0].garble_ppm = 500_000;
    let mut base = field(1, 1, metres(5000, 5000));
    place(&mut base, 1, 0, 0);
    place(&mut base, 2, 1000, 0);
    let mut garbled = 0;
    for seed in 1..=200u64 {
        let mut b = base.clone();
        b.world.rng = core_math::RngStreams::new(seed);
        let id = b
            .send_message(
                &t,
                OfficerId(1),
                OfficerId(2),
                Some(Medium::Courier),
                walk_to(50, 50),
            )
            .unwrap();
        let got = run_until(&mut b, &t, 20_000);
        assert_eq!(got.len(), 1);
        let reading = got[0].1.reading.unwrap();
        let rec = b
            .cmd
            .as_ref()
            .unwrap()
            .records
            .iter()
            .find(|r| r.id == id)
            .unwrap();
        assert_eq!(rec.garble, reading.garbled);
        assert_eq!(
            reading.garbled.is_some(),
            reading.received != walk_to(50, 50) || reading.garbled == Some(GarbleKind::GaitShifted)
        );
        garbled += u32::from(reading.garbled.is_some());
    }
    assert!(garbled > 60 && garbled < 140);
}
