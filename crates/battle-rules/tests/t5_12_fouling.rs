//! T5-12: fouling and misfire (SPEC-005 section 4).

mod t022_support;

use battle_rules::aim::draw_shot;
use battle_rules::fire::misfire_ppm;
use battle_rules::Training;
use core_math::Rng;
use t022_support::{duel, tables, tick};

#[test]
fn misfire_rate_at_full_fouling_is_fifty_thousand_ppm_scaled_by_wetness() {
    let t = tables();
    let w = t.weapons.get("smoothbore_69").unwrap();
    for wet_pm in [1000u32, 2000] {
        let want = 50_000 * wet_pm / 1000;
        assert_eq!(misfire_ppm(w, 1000, wet_pm), want);
        let n = 200_000u32;
        let mut rng = Rng::from_seed(77 + u64::from(wet_pm));
        let mut hits = 0u32;
        for _ in 0..n {
            let d = draw_shot(&mut rng);
            if d.misfire_roll % 1_000_000 < want {
                hits += 1;
            }
        }
        let p = f64::from(want) / 1e6;
        let mean = f64::from(n) * p;
        let sigma = (f64::from(n) * p * (1.0 - p)).sqrt();
        assert!(
            (f64::from(hits) - mean).abs() <= 3.0 * sigma,
            "wet {wet_pm}: {hits} misfires, expected {mean} +- {sigma}"
        );
    }
}

#[test]
fn clean_barrel_and_dry_powder_use_the_base_rate() {
    let t = tables();
    let w = t.weapons.get("smoothbore_69").unwrap();
    assert_eq!(misfire_ppm(w, 0, 1000), 20_000);
    assert_eq!(misfire_ppm(w, 500, 1000), 35_000);
    assert_eq!(misfire_ppm(w, 0, 0), 0);
}

#[test]
fn ram_step_lengthens_by_exactly_the_penalty() {
    for (training, fouling, want) in [
        (Training::Trained, 0u16, 140u16),
        (Training::Trained, 1000, 210),
        (Training::Trained, 500, 175),
        (Training::Raw, 1000, 252),
    ] {
        let (mut b, _, _) = duel(1, "smoothbore_69", 0, training, 1, 40);
        // Step 1 (seat) has one tick left; the next tick enters step 2 (ram).
        b.fire[0].step = 1;
        b.fire[0].left = 1;
        b.fire[0].fouling = fouling;
        b.step_fire();
        assert_eq!(b.fire[0].step, 2);
        assert_eq!(b.fire[0].left, want, "{training:?} fouling {fouling}");
    }
}

#[test]
fn a_misfire_spends_the_round_and_restarts_at_prime() {
    let (mut b, _, _) = duel(1, "smoothbore_69", 0, Training::Trained, 1, 40);
    b.weather.wet_pm = 100_000; // every roll misfires
    let rounds = b.fire[0].rounds;
    b.fire[0].step = 5; // Ready
    b.step_fire();
    assert_eq!(b.fire[0].rounds, rounds - 1);
    assert!(b.pool.is_empty());
    assert_eq!(b.counters().misfires, 1);
    assert_eq!(b.counters().shots_fired, 1);
    assert_eq!(b.counters().projectiles_spawned, 0);
    assert_eq!(b.fire[0].step, 3, "prime is the fourth step");
    assert_eq!(b.fire[0].left, 100);
    // The loading carries on from there and the soldier fires again after prime and ready.
    for _ in 0..160 {
        tick(&mut b);
    }
    assert_eq!(b.counters().shots_fired, 2);
}
