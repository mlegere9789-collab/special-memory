//! T6-06, states and casualty ingestion (SPEC-006 sections 5.1 and 5.5): hysteresis, the `hit`
//! requirement for every downward step, the entry actions and the shock arithmetic.

mod t6_support;

use battle_rules::morale::{MoraleEventKind, MoraleKind, SLOW_PERIOD_TICKS};
use battle_rules::{BattleState, Casualty, CasualtyKind, RegimentId};
use t6_support::{field, line, m, spawn};

const R0: RegimentId = RegimentId(0);

fn kind(b: &BattleState, i: usize) -> MoraleKind {
    b.morale.as_ref().unwrap().regs[i].kind
}

/// One tick of morale alone, then the clock.
fn tick(b: &mut BattleState, hit: Option<bool>) {
    b.step_morale_hooked(false, hit);
    b.world.advance_tick();
}

/// One whole slow period (20 ticks); the slow work runs on its first tick.
fn period(b: &mut BattleState, hit: Option<bool>) {
    for _ in 0..SLOW_PERIOD_TICKS {
        tick(b, hit);
    }
}

fn offset(b: &BattleState, i: usize) -> i64 {
    i64::from(b.morale.as_ref().unwrap().regs[i].nerve_offset)
}

fn set(b: &mut BattleState, i: usize, c: i64, k: MoraleKind) {
    let rm = &mut b.morale.as_mut().unwrap().regs[i];
    rm.c = u32::try_from(c.clamp(0, 1_000_000)).unwrap();
    rm.kind = k;
}

fn one_regiment() -> BattleState {
    let mut b = field(3, '.');
    line(&mut b, 0, 400, m(100, 100));
    b.enable_morale();
    b
}

fn cas(b: &BattleState, index: u32, kind: CasualtyKind) -> Casualty {
    Casualty {
        tick: b.world.tick,
        target_index: index,
        unit: 0,
        team: 0,
        kind,
        shooter_unit: 1,
        ammo: 0,
    }
}

/// A roster slot that is neither an officer nor the colonel.
fn plain_soldier(b: &BattleState) -> u32 {
    let rm = &b.morale.as_ref().unwrap().regs[0];
    b.regiments[0]
        .roster
        .iter()
        .find(|a| !rm.officers.contains(a))
        .unwrap()
        .index
}

#[test]
fn no_casualties_and_neutral_inputs_leave_every_regiment_steady() {
    let mut b = field(5, '.');
    line(&mut b, 0, 100, m(100, 100));
    spawn(
        &mut b,
        1,
        100,
        battle_rules::Training::Raw,
        battle_rules::Formation::Line,
        m(900, 900),
        core_math::Angle::ZERO,
    );
    b.enable_morale();
    for _ in 0..10_000 {
        tick(&mut b, None);
    }
    for i in 0..2 {
        let rm = &b.morale.as_ref().unwrap().regs[i];
        assert_eq!(rm.kind, MoraleKind::Steady, "regiment {i}");
        assert_eq!(rm.broken_count, 0);
    }
    assert!(b.morale.as_ref().unwrap().events.is_empty());
}

#[test]
fn cohesion_inside_the_band_changes_no_state() {
    let g = one_regiment().tables.morale.globals.clone();
    for start in [MoraleKind::Steady, MoraleKind::Wavering] {
        let mut b = one_regiment();
        let mid = (i64::from(g.waver) + i64::from(g.steady)) / 2 + offset(&b, 0);
        set(&mut b, 0, mid, start);
        for _ in 0..3000 {
            set(&mut b, 0, mid, start);
            tick(&mut b, Some(true));
            assert_eq!(kind(&b, 0), start);
        }
    }
}

#[test]
fn crossing_the_thresholds_moves_one_step_each_way() {
    let mut b = one_regiment();
    let g = b.tables.morale.globals.clone();
    let off = offset(&b, 0);
    set(
        &mut b,
        0,
        i64::from(g.waver) + off - 10_000,
        MoraleKind::Steady,
    );
    period(&mut b, None);
    assert_eq!(kind(&b, 0), MoraleKind::Wavering);
    // Back up through the top of the band.
    set(
        &mut b,
        0,
        i64::from(g.steady) + off + 10_000,
        MoraleKind::Wavering,
    );
    period(&mut b, None);
    assert_eq!(kind(&b, 0), MoraleKind::Steady);
}

#[test]
fn a_forced_miss_blocks_falling_back_and_a_hit_allows_it() {
    let mut b = one_regiment();
    let g = b.tables.morale.globals.clone();
    let low = i64::from(g.fall) + offset(&b, 0) - 20_000;
    for _ in 0..200 {
        set(&mut b, 0, low, MoraleKind::Wavering);
        period(&mut b, Some(false));
        assert_eq!(kind(&b, 0), MoraleKind::Wavering);
    }
    set(&mut b, 0, low, MoraleKind::Wavering);
    period(&mut b, Some(true));
    let ms = b.morale.as_ref().unwrap();
    assert_eq!(ms.regs[0].kind, MoraleKind::FallingBack);
    assert_eq!(ms.regs[0].state_ticks, SLOW_PERIOD_TICKS as u32 - 1);
    assert_eq!(ms.events.len(), 1);
    assert_eq!(ms.events[0].kind, MoraleEventKind::Fell);
    assert_eq!(ms.events[0].regiment, R0);
    assert_eq!(ms.events[0].pos, b.regiments[0].anchor);
    assert_eq!(ms.regs[0].broken_count, 0);
}

#[test]
fn the_natural_hit_rate_lets_a_shaken_regiment_fall_eventually() {
    let mut b = one_regiment();
    let g = b.tables.morale.globals.clone();
    let low = i64::from(g.fall) + offset(&b, 0) - 20_000;
    let mut periods = 0;
    set(&mut b, 0, low, MoraleKind::Wavering);
    while kind(&b, 0) == MoraleKind::Wavering {
        set(&mut b, 0, low, MoraleKind::Wavering);
        period(&mut b, None);
        periods += 1;
        assert!(periods < 400, "never fell");
    }
    assert_eq!(kind(&b, 0), MoraleKind::FallingBack);
}

#[test]
fn breaking_counts_and_records_a_broke_event() {
    let mut b = one_regiment();
    let g = b.tables.morale.globals.clone();
    let low = i64::from(g.break_) + offset(&b, 0) - 50_000;
    set(&mut b, 0, low, MoraleKind::FallingBack);
    period(&mut b, Some(false));
    assert_eq!(kind(&b, 0), MoraleKind::FallingBack);
    set(&mut b, 0, low, MoraleKind::FallingBack);
    period(&mut b, Some(true));
    let ms = b.morale.as_ref().unwrap();
    assert_eq!(ms.regs[0].kind, MoraleKind::Broken);
    assert_eq!(ms.regs[0].broken_count, 1);
    assert_eq!(ms.events.len(), 1);
    assert_eq!(ms.events[0].kind, MoraleEventKind::Broke);
}

#[test]
fn a_fallen_regiment_rallies_after_min_fall_without_pressure() {
    let mut b = one_regiment();
    let g = b.tables.morale.globals.clone();
    let high = i64::from(g.fall_exit) + offset(&b, 0) + 100_000;
    let mut left_at = None;
    set(&mut b, 0, high, MoraleKind::FallingBack);
    b.morale.as_mut().unwrap().regs[0].state_ticks = 0;
    for t in 0..(2 * g.min_fall) {
        set_c_only(&mut b, high);
        tick(&mut b, None);
        if kind(&b, 0) == MoraleKind::Rallied && left_at.is_none() {
            left_at = Some(t);
        }
    }
    let t = left_at.expect("rallied");
    assert!(t + 1 >= g.min_fall, "rallied after {t} ticks");
    assert!(t < g.min_fall + SLOW_PERIOD_TICKS as u32 + 1);
    assert_eq!(b.morale.as_ref().unwrap().regs[0].broken_count, 0);
}

fn set_c_only(b: &mut BattleState, c: i64) {
    b.morale.as_mut().unwrap().regs[0].c = u32::try_from(c.clamp(0, 1_000_000)).unwrap();
}

#[test]
fn a_broken_regiment_waits_for_pressure_to_lift_and_min_broken() {
    let g = one_regiment().tables.morale.globals.clone();
    // With an enemy inside the pressure radius it never rallies.
    let mut b = one_regiment();
    spawn(
        &mut b,
        1,
        50,
        battle_rules::Training::Trained,
        battle_rules::Formation::Line,
        m(150, 100),
        core_math::Angle::ZERO,
    );
    b.enable_morale_again_for_two();
    let high = i64::from(g.rally) + offset(&b, 0) + 200_000;
    set(&mut b, 0, high, MoraleKind::Broken);
    for _ in 0..(2 * g.min_broken) {
        set_c_only(&mut b, high);
        tick(&mut b, None);
        assert_eq!(kind(&b, 0), MoraleKind::Broken);
    }
    // Without it, it rallies once min_broken has passed, not before.
    let mut b = one_regiment();
    let high = i64::from(g.rally) + offset(&b, 0) + 200_000;
    set(&mut b, 0, high, MoraleKind::Broken);
    let mut at = None;
    for t in 0..(2 * g.min_broken) {
        set_c_only(&mut b, high);
        tick(&mut b, None);
        if at.is_none() && kind(&b, 0) == MoraleKind::Rallied {
            at = Some(t);
        }
    }
    let t = at.expect("rallied");
    assert!(t + 1 >= g.min_broken, "rallied after {t} ticks");
    assert!(t < g.min_broken + SLOW_PERIOD_TICKS as u32 + 1);
}

trait EnableTwo {
    fn enable_morale_again_for_two(&mut self);
}

impl EnableTwo for BattleState {
    /// The enemy was spawned after morale was enabled; start over with both regiments.
    fn enable_morale_again_for_two(&mut self) {
        self.morale = None;
        self.enable_morale();
    }
}

#[test]
fn an_officerless_leaderless_regiment_waits_the_long_time() {
    let mut b = one_regiment();
    let g = b.tables.morale.globals.clone();
    let high = i64::from(g.rally) + offset(&b, 0) + 200_000;
    set(&mut b, 0, high, MoraleKind::Broken);
    b.morale.as_mut().unwrap().regs[0].officers.clear();
    let mut at = None;
    for t in 0..(g.officerless_rally + 100) {
        set_c_only(&mut b, high);
        tick(&mut b, None);
        if at.is_none() && kind(&b, 0) == MoraleKind::Rallied {
            at = Some(t);
        }
    }
    let t = at.expect("rallied");
    assert!(t + 1 >= g.officerless_rally, "rallied after {t} ticks");
}

#[test]
fn a_rallied_regiment_settles_or_falls_again() {
    let mut b = one_regiment();
    let g = b.tables.morale.globals.clone();
    let off = offset(&b, 0);
    // Settles into Steady above the steady threshold once min_rallied has passed.
    set(&mut b, 0, 900_000, MoraleKind::Rallied);
    b.morale.as_mut().unwrap().regs[0].state_ticks = 0;
    for _ in 0..(g.min_rallied + 40) {
        set_c_only(&mut b, 900_000);
        tick(&mut b, None);
    }
    assert_eq!(kind(&b, 0), MoraleKind::Steady);
    // Between fall and steady it settles into Wavering.
    let mid = i64::from(g.fall) + off + 30_000;
    set(&mut b, 0, mid, MoraleKind::Rallied);
    b.morale.as_mut().unwrap().regs[0].state_ticks = 0;
    for _ in 0..(g.min_rallied + 40) {
        set_c_only(&mut b, mid);
        tick(&mut b, None);
    }
    assert_eq!(kind(&b, 0), MoraleKind::Wavering);
    // Below fall with a hit it falls back at once.
    let low = i64::from(g.fall) + off - 30_000;
    set(&mut b, 0, low, MoraleKind::Rallied);
    set_c_only(&mut b, low);
    period(&mut b, Some(true));
    assert_eq!(kind(&b, 0), MoraleKind::FallingBack);
}

#[test]
fn at_most_one_transition_per_period() {
    let mut b = one_regiment();
    set(&mut b, 0, 0, MoraleKind::Steady);
    // c is far below every threshold, yet Steady only becomes Wavering.
    tick(&mut b, Some(true));
    assert_eq!(kind(&b, 0), MoraleKind::Wavering);
}

#[test]
fn casualty_weights_follow_the_kind_and_the_post() {
    let mut b = one_regiment();
    b.world.advance_tick(); // off the slow period: only ingestion runs
    let rm = b.morale.as_ref().unwrap().regs[0].clone();
    let plain = plain_soldier(&b);
    let officer = rm
        .officers
        .iter()
        .find(|a| **a != rm.colonel)
        .unwrap()
        .index;
    let cases = [
        (cas(&b, plain, CasualtyKind::Killed), 5495u32),
        (cas(&b, plain, CasualtyKind::Wounded), 5495),
        (cas(&b, plain, CasualtyKind::Disabled), 3846),
        (cas(&b, officer, CasualtyKind::Killed), 32_970),
        (cas(&b, rm.colonel.index, CasualtyKind::Killed), 54_950),
        (cas(&b, u32::MAX, CasualtyKind::Killed), 5495),
    ];
    for (c, want) in cases {
        let mut x = one_regiment();
        x.world.advance_tick();
        x.casualties.push(c);
        let before = x.morale.as_ref().unwrap().regs[0].c;
        x.step_morale();
        let after = &x.morale.as_ref().unwrap().regs[0];
        assert_eq!(before - after.c, want, "{c:?}");
        assert_eq!(after.window, want);
        assert_eq!(after.loss, want * 600 / 1000);
        assert_eq!(x.morale.as_ref().unwrap().cursor, 1);
    }
}

#[test]
fn the_colonel_costs_ten_kills_and_leaves_the_officers() {
    let mut b = one_regiment();
    b.world.advance_tick();
    let colonel = b.morale.as_ref().unwrap().regs[0].colonel;
    let n = b.morale.as_ref().unwrap().regs[0].officers.len();
    b.casualties
        .push(cas(&b, colonel.index, CasualtyKind::Killed));
    b.step_morale();
    let rm = &b.morale.as_ref().unwrap().regs[0];
    assert_eq!(rm.officers.len(), n - 1);
    assert!(!rm.officers.contains(&colonel));
    let ten_kills = 10 * 5495;
    assert_eq!(880_000 - rm.c, ten_kills);
    // A second record of the same slot is an ordinary casualty.
    let before = rm.c;
    b.casualties
        .push(cas(&b, colonel.index, CasualtyKind::Killed));
    b.step_morale();
    assert_eq!(before - b.morale.as_ref().unwrap().regs[0].c, 5495);
}

#[test]
fn absentees_and_unknown_units_are_skipped_and_a_leader_calms() {
    let mut b = one_regiment();
    b.world.advance_tick();
    let who = plain_soldier(&b);
    let id = b.regiments[0]
        .roster
        .iter()
        .find(|a| a.index == who)
        .copied()
        .unwrap();
    b.morale.as_mut().unwrap().regs[0].absent.push(id);
    b.casualties.push(cas(&b, who, CasualtyKind::Killed));
    let mut other = cas(&b, who, CasualtyKind::Killed);
    other.unit = 9;
    b.casualties.push(other);
    let before = b.morale.as_ref().unwrap().regs[0].c;
    b.step_morale();
    let ms = b.morale.as_ref().unwrap();
    assert_eq!(ms.regs[0].c, before);
    assert_eq!(ms.cursor, 2);

    // A leader in reach lowers the shock by calm_pm * leader_pm / 1000.
    let mut b = one_regiment();
    b.world.advance_tick();
    b.morale.as_mut().unwrap().regs[0].leader_pm = 1000;
    let who = plain_soldier(&b);
    b.casualties.push(cas(&b, who, CasualtyKind::Killed));
    let before = b.morale.as_ref().unwrap().regs[0].c;
    b.step_morale();
    assert_eq!(
        before - b.morale.as_ref().unwrap().regs[0].c,
        5495 * 750 / 1000
    );
}

#[test]
fn state_ticks_count_every_tick_and_the_log_is_read_once() {
    let mut b = one_regiment();
    b.world.advance_tick();
    let who = plain_soldier(&b);
    b.casualties.push(cas(&b, who, CasualtyKind::Killed));
    for _ in 0..5 {
        b.step_morale();
    }
    let rm = &b.morale.as_ref().unwrap().regs[0];
    assert_eq!(rm.state_ticks, 5);
    assert_eq!(rm.window, 5495);
}

#[test]
fn nothing_happens_with_morale_off() {
    let mut b = field(1, '.');
    line(&mut b, 0, 40, m(100, 100));
    let h = b.hash();
    b.step_morale();
    assert!(b.morale.is_none());
    assert_eq!(b.hash(), h);
}
