//! T7-05: loss share against the formula over seeds 1 to 2,000, with and without 50 enemies on the
//! path; captures only with enemies, and a capture gives the enemy picture an intercepted contact.

mod t73_support;

use battle_rules::command::{Medium, OfficerId, Truth};
use t73_support::*;

/// Loss probability in ppm of a 3 km courier leg over open ground with `n` enemies counted.
fn expected_ppm(n: i64) -> i64 {
    let t = table();
    let g = &t.globals;
    let path_mm = 3_000_000 * i64::from(g.path_factor_pm) / 1000;
    let p = i64::from(t.media[0].loss_base_ppm)
        + i64::from(g.loss_ppm_per_km) * path_mm / 1_000_000
        + i64::from(g.exposure_ppm_per_enemy) * n.min(i64::from(g.exposure_cap));
    p.min(i64::from(g.loss_cap_ppm))
}

fn trial(enemies: usize) -> (u32, u32, u32) {
    let t = table();
    let mut base = field(1, enemies, metres(1500, 0));
    place(&mut base, 1, 0, 0);
    place(&mut base, 2, 3000, 0);
    let (mut lost, mut captured, mut intercepted) = (0, 0, 0);
    for seed in 1..=2000u64 {
        let mut b = base.clone();
        b.world.rng = core_math::RngStreams::new(seed);
        let id = b
            .send_message(
                &t,
                OfficerId(1),
                OfficerId(2),
                Some(Medium::Courier),
                hold(),
            )
            .unwrap();
        let truth = b
            .cmd
            .as_ref()
            .unwrap()
            .records
            .iter()
            .find(|r| r.id == id)
            .unwrap()
            .truth;
        match truth {
            Truth::Lost => lost += 1,
            Truth::Captured => {
                captured += 1;
                // The contact appears after one leg's time in the nearest enemy officer's picture.
                run_until(&mut b, &t, 40_000);
                let cmd = b.cmd.as_ref().unwrap();
                let i = cmd.index_of(OfficerId(10)).unwrap();
                let hit = cmd
                    .pics
                    .iter()
                    .flat_map(|p| &p.contacts)
                    .filter(|c| c.intercepted)
                    .count();
                assert_eq!(hit, 1, "seed {seed}");
                assert!(cmd.pics[i]
                    .contacts
                    .iter()
                    .all(|c| c.quality_pm == t.globals.capture_quality_pm));
                intercepted += 1;
            }
            _ => {}
        }
    }
    (lost, captured, intercepted)
}

fn within_3_sigma(count: u32, p_ppm: i64) {
    let n = 2000f64;
    let p = p_ppm as f64 / 1e6;
    let (mean, sd) = (n * p, (n * p * (1.0 - p)).sqrt());
    assert!(
        (f64::from(count) - mean).abs() <= 3.0 * sd,
        "{count} vs {mean} +- {sd}"
    );
}

#[test]
fn loss_share_without_enemies() {
    let (lost, captured, _) = trial(0);
    assert_eq!(captured, 0, "no enemies, no captures");
    within_3_sigma(lost, expected_ppm(0));
}

#[test]
fn loss_share_with_fifty_enemies_and_capture() {
    let (lost, captured, intercepted) = trial(50);
    within_3_sigma(lost + captured, expected_ppm(50));
    assert!(captured > 0 && intercepted == captured);
    // Captures are 60 percent of the losses (capture_share_pm), within 3 sigma.
    let total = f64::from(lost + captured);
    let (mean, sd) = (total * 0.6, (total * 0.6 * 0.4).sqrt());
    assert!(
        (f64::from(captured) - mean).abs() <= 3.0 * sd,
        "{captured} of {total}"
    );
}
