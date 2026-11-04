//! T5-08: hit geometry, obstacles, own-unit clearance and down soldiers (SPEC-005 section 5).

mod t023_support;

use battle_rules::hit::{Obstacle, QueuedHit};
use battle_rules::{F_DISABLED, F_KNEEL, F_PRONE, F_WOUNDED};
use core_math::{Fx, Vec2Fx};
use t023_support::{bullet, field_of, fire_through, m, man};

/// Whether a ball flying at `z_mm` through a lone target of `flags` strikes it.
fn struck(flags: u16, z_mm: i64) -> bool {
    let mut b = field_of(1, '.');
    let t = man(&mut b, 1, m(100, 200), flags);
    let at = b.world.agents.pos(t).unwrap();
    let mut p = bullet(&b, "minie_58", 999, 77, 5);
    let mut out: Vec<QueuedHit> = Vec::new();
    fire_through(&b, &mut p, at, z_mm, 50_000, 290, &mut out).struck == 1
}

#[test]
fn standing_target_is_hit_at_1_2_m_and_missed_at_1_9_m() {
    assert!(struck(0, 1200));
    assert!(!struck(0, 1900));
    assert!(!struck(0, 1701));
}

#[test]
fn kneeling_target_is_missed_at_1_4_m() {
    assert!(struck(F_KNEEL, 1100));
    assert!(!struck(F_KNEEL, 1400));
}

#[test]
fn prone_target_is_hit_only_below_450_mm() {
    assert!(struck(F_PRONE, 400));
    assert!(!struck(F_PRONE, 500));
    assert!(!struck(F_PRONE, 1200));
}

#[test]
fn a_ball_below_ground_level_misses() {
    assert!(!struck(0, -100));
}

#[test]
fn an_obstacle_of_1000_mm_stops_0_8_m_and_passes_1_3_m() {
    let mut b = field_of(1, '.');
    let t = man(&mut b, 1, m(100, 200), 0);
    let at = b.world.agents.pos(t).unwrap();
    b.obstacles.segments.push(Obstacle {
        a: Vec2Fx::new(at.x - Fx::from_int(10), at.y - Fx::from_int(50)),
        b: Vec2Fx::new(at.x - Fx::from_int(10), at.y + Fx::from_int(50)),
        height_mm: 1000,
    });
    let mut out = Vec::new();
    let mut low = bullet(&b, "minie_58", 999, 77, 1);
    let r = fire_through(&b, &mut low, at, 800, 50_000, 290, &mut out);
    assert!(r.stopped && r.struck == 0 && out.is_empty());
    let mut high = bullet(&b, "minie_58", 999, 77, 1);
    let r = fire_through(&b, &mut high, at, 1300, 50_000, 290, &mut out);
    assert_eq!(r.struck, 1);
    // An obstacle beside the line of fire does nothing.
    let mut aside = bullet(&b, "minie_58", 999, 77, 1);
    let r = fire_through(
        &b,
        &mut aside,
        Vec2Fx::new(at.x, at.y - Fx::from_int(60)),
        800,
        50_000,
        290,
        &mut out,
    );
    assert!(!r.stopped);
}

#[test]
fn an_obstacle_behind_the_target_does_not_shield_it() {
    let mut b = field_of(1, '.');
    let t = man(&mut b, 1, m(100, 200), 0);
    let at = b.world.agents.pos(t).unwrap();
    b.obstacles.segments.push(Obstacle {
        a: Vec2Fx::new(at.x + Fx::from_int(5), at.y - Fx::from_int(50)),
        b: Vec2Fx::new(at.x + Fx::from_int(5), at.y + Fx::from_int(50)),
        height_mm: 1000,
    });
    let mut out = Vec::new();
    let mut p = bullet(&b, "minie_58", 999, 77, 1);
    let r = fire_through(&b, &mut p, at, 800, 50_000, 290, &mut out);
    assert_eq!(r.struck, 1, "the soldier comes first on the chord");
}

#[test]
fn own_unit_soldiers_are_skipped_only_within_2500_mm() {
    let mut b = field_of(1, '.');
    let t = man(&mut b, 0, m(100, 200), 0);
    let at = b.world.agents.pos(t).unwrap();
    let unit = b.world.agents.unit(t).unwrap();
    let mut out = Vec::new();
    for (travelled, expect) in [(0, 0), (2499, 0), (2500, 1), (40_000, 1)] {
        let mut p = bullet(&b, "minie_58", 999, unit, 1);
        let r = fire_through(&b, &mut p, at, 1200, travelled, 290, &mut out);
        assert_eq!(r.struck, expect, "travelled {travelled}");
    }
    // Another unit is never skipped.
    let mut p = bullet(&b, "minie_58", 999, unit + 1, 1);
    assert_eq!(
        fire_through(&b, &mut p, at, 1200, 0, 290, &mut out).struck,
        1
    );
}

#[test]
fn the_owner_and_down_soldiers_are_never_hit() {
    let mut b = field_of(1, '.');
    let t = man(&mut b, 1, m(100, 200), 0);
    let at = b.world.agents.pos(t).unwrap();
    let mut out = Vec::new();
    let mut own = bullet(&b, "minie_58", t.index, 77, 1);
    assert_eq!(
        fire_through(&b, &mut own, at, 1200, 50_000, 290, &mut out).struck,
        0
    );
    for flag in [F_DISABLED, F_WOUNDED] {
        b.world.agents.set_flags(t, flag);
        let mut p = bullet(&b, "minie_58", 999, 77, 1);
        let r = fire_through(&b, &mut p, at, 1200, 50_000, 290, &mut out);
        assert_eq!(r.struck, 0);
        assert!(!r.stopped);
    }
}

#[test]
fn a_target_outside_the_body_width_is_missed() {
    let mut b = field_of(1, '.');
    let t = man(&mut b, 1, m(100, 200), 0);
    let at = b.world.agents.pos(t).unwrap();
    let mut out = Vec::new();
    // The body is 500 mm wide: 0.2 m off the centre hits, 0.4 m does not.
    for (dy_mm, hit) in [(200, 1), (-200, 1), (400, 0), (-400, 0)] {
        let mut p = bullet(&b, "minie_58", 999, 77, 1);
        let line = Vec2Fx::new(at.x, at.y + t023_support::mm(dy_mm));
        let r = fire_through(&b, &mut p, line, 1200, 50_000, 290, &mut out);
        assert_eq!(r.struck, hit, "offset {dy_mm} mm");
    }
}
