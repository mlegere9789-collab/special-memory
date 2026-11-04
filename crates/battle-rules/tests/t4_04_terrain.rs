//! T4-04: in `terrain_strip` the anchor spends `strip_length / (speed * 0.6) * 20` ticks inside
//! the 40 m woods strip, within 2 percent (SPEC-004 section 13 and Amendment A2).

mod t019_support;

use battle_rules::{scenarios, Gait, Order};
use core_math::Fx;
use t019_support::{ref_rows, tables, within_percent};

#[test]
fn ticks_inside_the_strip_follow_the_woods_speed() {
    let t = tables();
    let mut b = scenarios::terrain_strip(&t, 1);
    let (lo, hi) = (Fx::from_int(80), Fx::from_int(120));
    let mut inside = 0u64;
    while b.regiments[0].order != Order::Hold {
        assert!(b.world.tick < 20_000, "never arrives");
        let x = b.regiments[0].anchor.x;
        inside += u64::from(x >= lo && x < hi);
        b.step_movement();
    }
    let speed = u64::from(t.locomotion.gait(Gait::Quick).speed_mm_s);
    let woods = u64::from(t.terrain.class(3).unwrap().speed_pm);
    assert_eq!(t.terrain.class(3).unwrap().name, "woods");
    let want = (40_000u64 * 20 * 1000).div_ceil(speed * woods);
    let row = ref_rows().into_iter().find(|r| r.0 == "strip").unwrap();
    println!("strip: {inside} ticks, formula {want}, model {}", row.4);
    assert_eq!(row.3, want);
    assert!(within_percent(inside, want, 2), "{inside} vs {want}");
    assert!(
        within_percent(inside, row.4, 2),
        "{inside} vs model {}",
        row.4
    );
}
