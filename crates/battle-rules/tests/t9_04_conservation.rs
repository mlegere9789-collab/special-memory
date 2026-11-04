//! T9-04: smoke mass is conserved exactly by the update, every cell reaches zero, and the
//! source adds `powder_mg * yield_pm / 1000` with the saturated excess counted (SPEC-009 section 4).

mod t93_support;

use battle_rules::SmokeGrid;
use core_math::Vec2Fx;
use t93_support::{at, balance, centre, grid, params, put, Lcg};

#[test]
fn add_smoke_scales_by_yield_and_misfire_and_saturates_at_the_cap() {
    let p = params(&[("cell_cap = 1000000", "cell_cap = 3000")]);
    let mut g = grid(4, 4);
    g.add_smoke(centre(1, 1), 3900, false, &p);
    assert_eq!(at(&g, 1, 1), 2145); // 3900 * 550 / 1000
    g.add_smoke(centre(2, 2), 3900, true, &p);
    assert_eq!(at(&g, 2, 2), 536); // 2145 * 250 / 1000, floors in order
    g.add_smoke(centre(1, 1), 3900, false, &p); // 4290 asked, 855 fit
    assert_eq!(at(&g, 1, 1), 3000);
    assert_eq!(g.lost_cap, 2145 - 855);
    assert_eq!(g.injected, 2145 + 536 + 2145);
    assert_eq!(balance(&g).0, balance(&g).1);
    g.add_smoke(centre(1, 1), 0, false, &p);
    assert_eq!(at(&g, 1, 1), 3000);
    assert_eq!(SmokeGrid::empty().total(), 0);
    let mut e = SmokeGrid::empty();
    e.add_smoke(Vec2Fx::ZERO, 3900, false, &p);
    assert_eq!((e.injected, e.total()), (0, 0));
}

#[test]
fn injected_equals_density_plus_the_three_losses_for_2000_updates() {
    let p = params(&[("cell_cap = 1000000", "cell_cap = 20000")]);
    let mut g = grid(24, 20);
    let mut rng = Lcg(7);
    let mut asked = 0u64;
    for step in 0..2000u64 {
        // Sources at a few cells every few updates, some of them over the cap.
        if step % 3 == 0 {
            for _ in 0..3 {
                let (x, y) = (rng.next(24) as u32, rng.next(20) as u32);
                let mg = 2000 + rng.next(60_000) as u32;
                g.add_smoke(centre(x, y), mg, rng.next(4) == 0, &p);
            }
        }
        let (wx, wy) = (
            (rng.next(9001) as i64) - 4500,
            (rng.next(9001) as i64) - 4500,
        );
        let rain = if step % 400 < 150 { 700 } else { 0 };
        // Woods in the middle columns hold smoke back.
        let retain = |pos: Vec2Fx| {
            if pos.x.to_int_floor() / 8 % 6 == 0 {
                400
            } else {
                0
            }
        };
        g.update(&p, wx, wy, rain, &retain);
        assert_eq!(balance(&g).0, balance(&g).1, "update {step}");
        asked = g.injected;
    }
    assert!(asked > 0 && g.lost_edge > 0 && g.lost_decay > 0 && g.lost_cap > 0);
}

#[test]
fn a_lone_cell_under_decay_only_reaches_zero_after_the_reference_count() {
    let p = params(&[("diffuse_ppm = 10000", "diffuse_ppm = 0")]);
    let mut g = grid(5, 5);
    put(&mut g, 2, 2, 100_000);
    let mut want = 100_000u64;
    let mut updates = 0;
    while want > 0 {
        want -= (want * 4000).div_ceil(1_000_000);
        g.update(&p, 0, 0, 0, &|_| 0);
        updates += 1;
        assert_eq!(u64::from(at(&g, 2, 2)), want, "update {updates}");
        assert_eq!(g.total(), want);
    }
    assert!(updates > 100);
    assert_eq!(g.total(), 0);
    assert_eq!(g.lost_decay, 100_000);
    assert_eq!((g.lost_edge, g.lost_cap), (0, 0));
}

#[test]
fn rain_raises_the_decay_and_every_cell_reaches_zero() {
    let p = params(&[]);
    let count = |rain: i32| {
        let mut g = grid(7, 7);
        put(&mut g, 3, 3, 900_000);
        let mut n = 0;
        while g.total() > 0 {
            g.update(&p, 0, 0, rain, &|_| 0);
            n += 1;
            assert!(n < 20_000);
        }
        n
    };
    assert!(count(1000) < count(0));
}

#[test]
fn smoke_pm_is_the_share_of_the_reference_units_capped_at_1000() {
    let mut g = grid(3, 3);
    put(&mut g, 1, 1, 5000);
    put(&mut g, 2, 2, 500_000);
    assert_eq!(g.smoke_pm(centre(1, 1), 20_000), 250);
    assert_eq!(g.smoke_pm(centre(2, 2), 20_000), 1000);
    assert_eq!(g.smoke_pm(centre(0, 0), 20_000), 0);
    assert_eq!(SmokeGrid::empty().smoke_pm(Vec2Fx::ZERO, 20_000), 0);
    assert_eq!(g.total(), 505_000);
}
