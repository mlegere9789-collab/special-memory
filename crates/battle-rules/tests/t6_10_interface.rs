//! T6-10, interface (SPEC-006 sections 5.7 and 7, and the D-04 extension): no raw values, an
//! indicator that worsens at once and improves after `dwell`, causes ranked by size, the margin
//! band, the trend and the nine terms.

mod t64_support;
mod t6_support;

use battle_rules::morale::{rank_causes, Cause, Indicator, MoraleKind, MoraleReport};
use battle_rules::{Formation, RegimentId, Training};
use core_math::Angle;
use t64_support::{duel, period, rm, rm_mut, set, tick};
use t6_support::{field, line, m, spawn};

const R0: RegimentId = RegimentId(0);

fn report(b: &battle_rules::BattleState) -> MoraleReport {
    b.morale_report(R0).expect("morale is on")
}

#[test]
fn the_report_has_exactly_the_public_fields() {
    let b = duel(1, 100, None);
    // An exhaustive pattern: a field for `c` or `loss` would have to be named here.
    let MoraleReport {
        indicator,
        causes,
        fire_pm,
        shake_pm,
        looking,
        absent,
        thinning_pm,
        terms,
        margin_pm,
        trend,
    } = report(&b);
    assert_eq!(indicator, Indicator::Firm);
    assert_eq!(causes, [None, None]);
    assert_eq!((fire_pm, shake_pm), (1000, 1000));
    assert_eq!((looking, absent, thinning_pm), (0, 0, 0));
    assert_eq!(terms, [0; 9]);
    assert_eq!(trend, 0);
    assert!(margin_pm.is_multiple_of(50));
}

#[test]
fn the_report_is_none_without_morale_or_for_an_unknown_regiment() {
    let mut b = field(1, '.');
    line(&mut b, 0, 50, m(100, 100));
    assert!(b.morale_report(R0).is_none());
    b.enable_morale();
    assert!(b.morale_report(R0).is_some());
    assert!(b.morale_report(RegimentId(9)).is_none());
}

#[test]
fn the_indicator_worsens_at_once() {
    let mut b = duel(1, 100, None);
    set(&mut b, 0, 450_000, MoraleKind::Wavering);
    tick(&mut b, Some(false));
    assert_eq!(report(&b).indicator, Indicator::Shaken);
    set(&mut b, 0, 0, MoraleKind::FallingBack);
    tick(&mut b, Some(false));
    assert_eq!(report(&b).indicator, Indicator::Retiring);
    set(&mut b, 0, 0, MoraleKind::Broken);
    tick(&mut b, Some(false));
    assert_eq!(report(&b).indicator, Indicator::Routed);
}

#[test]
fn the_indicator_improves_only_after_dwell_ticks() {
    let mut b = duel(1, 100, None);
    let dwell = u64::from(b.tables.morale.globals.dwell);
    set(&mut b, 0, 450_000, MoraleKind::Wavering);
    tick(&mut b, Some(false));
    assert_eq!(report(&b).indicator, Indicator::Shaken);
    for k in 1..dwell {
        set(&mut b, 0, 1_000_000, MoraleKind::Steady);
        tick(&mut b, Some(false));
        assert_eq!(report(&b).indicator, Indicator::Shaken, "after {k} ticks");
    }
    set(&mut b, 0, 1_000_000, MoraleKind::Steady);
    tick(&mut b, Some(false));
    assert_eq!(report(&b).indicator, Indicator::Firm);
    assert_eq!(rm(&b, 0).dwell, 0);
}

#[test]
fn wavering_above_the_midline_is_uneasy_and_rallied_is_rallying() {
    let mut b = duel(1, 100, None);
    set(&mut b, 0, 620_000, MoraleKind::Wavering);
    tick(&mut b, Some(false));
    assert_eq!(report(&b).indicator, Indicator::Uneasy);
    let mut b = duel(1, 100, None);
    set(&mut b, 0, 500_000, MoraleKind::Rallied);
    tick(&mut b, Some(false));
    assert_eq!(report(&b).indicator, Indicator::Rallying);
}

#[test]
fn causes_sort_by_size_then_enum_order_and_need_cause_min() {
    let min = 20_000;
    let mut mag = [0u32; 9];
    mag[Cause::Supply as usize] = 100_000;
    mag[Cause::Fatigue as usize] = 100_000;
    mag[Cause::Terrain as usize] = 99_999;
    assert_eq!(
        rank_causes(&mag, min),
        [Some(Cause::Fatigue), Some(Cause::Supply)]
    );
    mag[Cause::Losses as usize] = 100_000;
    assert_eq!(
        rank_causes(&mag, min),
        [Some(Cause::Losses), Some(Cause::Fatigue)]
    );
    mag[Cause::Charge as usize] = 500_000;
    assert_eq!(
        rank_causes(&mag, min),
        [Some(Cause::Charge), Some(Cause::Losses)]
    );
    let mut small = [0u32; 9];
    small[Cause::Flank as usize] = min - 1;
    assert_eq!(rank_causes(&small, min), [None, None]);
    small[Cause::Flank as usize] = min;
    assert_eq!(rank_causes(&small, min), [Some(Cause::Flank), None]);
}

#[test]
fn a_flanked_regiment_lists_flank() {
    // The enemy stands due north of a regiment facing east: along its line.
    let mut b = field(2, '.');
    line(&mut b, 0, 100, m(100, 100));
    spawn(
        &mut b,
        1,
        100,
        Training::Trained,
        Formation::Line,
        m(100, 250),
        Angle::HALF,
    );
    b.enable_morale();
    let r = report(&b);
    assert_eq!(r.causes[0], Some(Cause::Flank));
    let base = i64::from(b.tables.morale.training(Training::Trained).base_ppm);
    let flank = i64::from(b.tables.morale.globals.flank_pen);
    assert_eq!(
        i64::from(r.terms[Cause::Flank as usize]),
        flank * 1000 / base
    );
}

#[test]
fn a_regiment_out_of_ammunition_lists_ammunition() {
    let mut b = field(2, '.');
    let id = line(&mut b, 0, 100, m(100, 100));
    b.equip_regiment(id, "rifle_58", 0).unwrap();
    b.enable_morale();
    assert_eq!(report(&b).causes[0], None);
    for a in b.regiments[0].roster.clone() {
        b.fire[a.index as usize].rounds = 0;
    }
    let r = report(&b);
    assert_eq!(r.causes[0], Some(Cause::Ammunition));
    let base = i64::from(b.tables.morale.training(Training::Trained).base_ppm);
    let pen = i64::from(b.tables.morale.globals.ammo_pen);
    assert_eq!(
        i64::from(r.terms[Cause::Ammunition as usize]),
        pen * 1000 / base
    );
}

#[test]
fn losses_and_officers_are_causes() {
    let mut b = duel(1, 100, None);
    {
        let r = rm_mut(&mut b, 0);
        r.loss = 300_000;
        r.officers.truncate(1);
    }
    let r = report(&b);
    assert_eq!(r.causes[0], Some(Cause::Losses));
    assert_eq!(r.causes[1], Some(Cause::Officers));
    let g = &b.tables.morale.globals;
    let lost = battle_rules::morale::officers_total(100) as u64 - 1;
    let want = lost * u64::from(g.w_officer_pm) * u64::from(rm(&b, 0).shock_unit) / 1000;
    let base = u64::from(b.tables.morale.training(Training::Trained).base_ppm);
    assert_eq!(
        u64::from(r.terms[Cause::Officers as usize]),
        want * 1000 / base
    );
    assert_eq!(
        u64::from(r.terms[Cause::Losses as usize]),
        (300_000 + u64::from(rm(&b, 0).window)) * 1000 / base
    );
}

#[test]
fn the_margin_is_a_quantised_distance_to_the_next_worse_level() {
    let mut b = duel(1, 100, None);
    let off = i64::from(rm(&b, 0).nerve_offset);
    let waver = i64::from(b.tables.morale.globals.waver) + off;
    for c in [880_000i64, 760_000, 700_000, 662_345] {
        set(&mut b, 0, c as u32, MoraleKind::Steady);
        let want = ((c - waver) / 1000) / 50 * 50;
        assert_eq!(i64::from(report(&b).margin_pm), want, "c = {c}");
    }
    set(&mut b, 0, 0, MoraleKind::Broken);
    tick(&mut b, Some(false));
    assert_eq!(report(&b).margin_pm, 0);
    // Wavering: the Uneasy part measures to 500,000, the Shaken part to the fall line.
    let mut b = duel(1, 100, None);
    set(&mut b, 0, 560_000, MoraleKind::Wavering);
    tick(&mut b, Some(false));
    let r = report(&b);
    assert_eq!(r.indicator, Indicator::Uneasy);
    assert!(
        r.margin_pm <= 100 && r.margin_pm.is_multiple_of(50),
        "{}",
        r.margin_pm
    );
}

#[test]
fn the_trend_follows_the_change_of_cohesion() {
    let mut b = duel(1, 100, None);
    let base = b.tables.morale.training(Training::Trained).base_ppm;
    // At the ceiling nothing moves.
    for _ in 0..3 {
        period(&mut b, Some(false));
    }
    assert_eq!(report(&b).trend, 0);
    // Steadily losing cohesion.
    for _ in 0..4 {
        rm_mut(&mut b, 0).c -= 30_000;
        period(&mut b, Some(false));
    }
    assert_eq!(report(&b).trend, -1);
    // Steadily regaining it.
    set(&mut b, 0, base / 2, MoraleKind::Rallied);
    {
        // The jump to the new level is not a trend: start the memory from it.
        let r = rm_mut(&mut b, 0);
        r.memo.c_prev = r.c;
        r.memo.slope = 0;
    }
    for _ in 0..6 {
        period(&mut b, Some(false));
        let r = rm_mut(&mut b, 0);
        r.state_ticks = 0;
    }
    assert_eq!(report(&b).trend, 1);
}

#[test]
fn thinning_counts_everyone_off_the_roster() {
    let mut b = duel(1, 100, None);
    let gone: Vec<_> = b.regiments[0].roster.iter().take(20).copied().collect();
    for a in &gone {
        b.world.agents.despawn(*a);
    }
    b.step_rosters();
    assert_eq!(report(&b).thinning_pm, 200);
}
