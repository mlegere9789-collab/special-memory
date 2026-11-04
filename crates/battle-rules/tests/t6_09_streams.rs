//! T6-09, rolls (SPEC-006 section 5.4): exactly `1 + roster_len` draws per regiment per slow
//! period from the regiment's own named stream.

mod t6_support;

use battle_rules::morale::SLOW_PERIOD_TICKS;
use battle_rules::registry::morale_stream;
use battle_rules::{BattleState, Formation, Training};
use core_math::{Angle, RngStreams};
use t6_support::{field, m, spawn};

fn build(seed: u64, counts: &[usize]) -> BattleState {
    let mut b = field(seed, '.');
    for (i, n) in counts.iter().enumerate() {
        spawn(
            &mut b,
            (i % 2) as u8,
            *n,
            Training::Trained,
            Formation::Line,
            m(100 + 600 * i as i32, 100),
            Angle::ZERO,
        );
    }
    b.enable_morale();
    b
}

fn periods(b: &mut BattleState, n: u64) -> Vec<Vec<[u64; 4]>> {
    let mut seen = Vec::new();
    for _ in 0..n {
        for _ in 0..SLOW_PERIOD_TICKS {
            b.step_morale();
            b.world.advance_tick();
        }
        seen.push(
            b.morale
                .as_ref()
                .unwrap()
                .regs
                .iter()
                .map(|r| r.rng.state())
                .collect(),
        );
    }
    seen
}

#[test]
fn one_roll_plus_one_per_soldier_each_period() {
    let seed = 21;
    let counts = [40usize, 7, 90];
    let mut b = build(seed, &counts);
    let seen = periods(&mut b, 6);
    let g = b.tables.morale.globals.clone();
    for (i, n) in counts.iter().enumerate() {
        let mut want = RngStreams::new(seed).stream(morale_stream(i as u16));
        let spread = i32::try_from(g.nerve_spread).unwrap();
        want.range_i32(-spread, spread);
        for (p, states) in seen.iter().enumerate() {
            want.next_u32();
            for _ in 0..*n {
                want.next_u64();
            }
            assert_eq!(states[i], want.state(), "regiment {i}, period {p}");
        }
    }
}

#[test]
fn non_slow_ticks_draw_nothing() {
    let mut b = build(2, &[40]);
    b.world.advance_tick();
    let before = b.morale.as_ref().unwrap().regs[0].rng.state();
    for _ in 0..(SLOW_PERIOD_TICKS - 1) {
        b.step_morale();
        b.world.advance_tick();
    }
    assert_eq!(b.morale.as_ref().unwrap().regs[0].rng.state(), before);
}

#[test]
fn removing_an_unrelated_regiment_leaves_the_first_fifty_rolls() {
    let mut full = build(9, &[40, 60, 25]);
    let mut part = build(9, &[40, 60]);
    let a = periods(&mut full, 50);
    let b = periods(&mut part, 50);
    for p in 0..50 {
        assert_eq!(a[p][0], b[p][0], "period {p}");
        assert_eq!(a[p][1], b[p][1], "period {p}");
    }
    assert_ne!(a[0][0], a[0][1]);
}

#[test]
fn a_forced_outcome_still_draws_the_same() {
    let mut x = build(4, &[30]);
    let mut y = build(4, &[30]);
    for _ in 0..(3 * SLOW_PERIOD_TICKS) {
        x.step_morale_hooked(false, Some(true));
        y.step_morale_hooked(false, Some(false));
        x.world.advance_tick();
        y.world.advance_tick();
    }
    assert_eq!(
        x.morale.as_ref().unwrap().regs[0].rng,
        y.morale.as_ref().unwrap().regs[0].rng
    );
}
