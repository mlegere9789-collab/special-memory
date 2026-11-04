//! T9-07: updating only the active rectangle equals updating the full grid, bit for bit.

mod t93_support;

use core_math::Vec2Fx;
use proptest::prelude::*;
use t93_support::{grid, params, put};

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn active_rectangle_update_equals_the_full_grid_update(
        w in 2u32..16,
        h in 2u32..16,
        cells in proptest::collection::vec((0u32..16, 0u32..16, 1u32..1_000_000), 0..8),
        wind in (-9000i64..9000, -9000i64..9000),
        rain in 0i32..=1000,
        wood in 0u32..=500,
        steps in 1usize..12,
    ) {
        let p = params(&[]);
        let mut a = grid(w, h);
        for (x, y, d) in cells {
            put(&mut a, x % w, y % h, d);
        }
        let mut b = a.clone();
        let retain = move |pos: Vec2Fx| if (pos.x.to_int_floor() / 8 + pos.y.to_int_floor() / 8) % 3 == 0 { wood } else { 0 };
        for _ in 0..steps {
            a.update_with(&p, wind.0, wind.1, rain, &retain, false, false);
            b.update_with(&p, wind.0, wind.1, rain, &retain, false, true);
            prop_assert_eq!(&a, &b);
        }
    }
}

#[test]
fn an_empty_grid_stays_empty_and_a_corner_puff_loses_mass_over_the_edge() {
    let p = params(&[]);
    let mut g = grid(6, 6);
    g.update(&p, 3000, 3000, 0, &|_| 0);
    assert_eq!((g.total(), g.lost_edge), (0, 0));
    put(&mut g, 0, 0, 800_000);
    for _ in 0..5 {
        g.update(&p, -4000, -4000, 0, &|_| 0);
    }
    assert!(g.lost_edge > 0);
    assert_eq!(
        g.injected,
        g.total() + g.lost_edge + g.lost_decay + g.lost_cap
    );
}
