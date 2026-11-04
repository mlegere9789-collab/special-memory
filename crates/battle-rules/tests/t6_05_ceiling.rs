//! T6-05, ceiling and relaxation: `T` and `Tc` equal their formulas and stay in range, constant
//! inputs converge monotonically to the ceiling without overshoot, and a slow period applies all
//! of it to a battle (SPEC-006 section 5.3).

mod t6_support;

use battle_rules::morale::{
    ceiling_target, decay_window, effective_ceiling, officer_factor_pm, Drive, MoraleGlobals,
    MoraleInputs, MoraleKind, MoraleTable, MoraleTrainingRow,
};
use battle_rules::Training;
use proptest::prelude::*;
use t6_support::{field, line, m};

const MORALE: &str = include_str!("../../../data/rules/morale.toml");

/// `relax_step` with the drive spelled out as plain numbers.
#[allow(clippy::too_many_arguments)]
fn relax_step(
    g: &MoraleGlobals,
    row: &MoraleTrainingRow,
    kind: MoraleKind,
    c: u32,
    tc: u32,
    officer_pm: u32,
    leader_pm: u16,
    pressure: bool,
) -> u32 {
    let drive = Drive {
        officer_pm,
        leader_pm,
        pressure,
    };
    battle_rules::morale::relax_step(g, row, kind, c, tc, drive)
}

fn table() -> MoraleTable {
    MoraleTable::from_toml_str(MORALE).unwrap()
}

fn inputs(vals: [u32; 9]) -> MoraleInputs {
    MoraleInputs {
        fatigue: vals[0],
        ammo: vals[1],
        supply: vals[2],
        flank: vals[3],
        charge: vals[4],
        disorder: vals[5],
        cover: vals[6],
        wall: vals[7],
        leader: vals[8],
        ..MoraleInputs::default()
    }
}

#[test]
fn target_is_base_less_penalties_plus_bonuses() {
    let t = table();
    let g = &t.globals;
    let base = t.training(Training::Trained).base_ppm;
    assert_eq!(ceiling_target(g, base, 0, &MoraleInputs::default()), base);
    // Penalties 1000 + 2000 + 3000 + 4000 + 5000 + 6000, bonuses 700 + 800 + 900.
    let i = inputs([1000, 2000, 3000, 4000, 5000, 6000, 700, 800, 900]);
    assert_eq!(ceiling_target(g, base, 0, &i), base - 21_000 + 2400);
    // Repeat penalty: `repeat_pen` per earlier break, at most three.
    for (bc, n) in [(1u8, 1u32), (2, 2), (3, 3), (4, 3), (255, 3)] {
        assert_eq!(
            ceiling_target(g, base, bc, &MoraleInputs::default()),
            base - g.repeat_pen * n
        );
    }
}

#[test]
fn effective_ceiling_removes_loss_and_caps_broken() {
    let t = table();
    let g = &t.globals;
    assert_eq!(
        effective_ceiling(g, 800_000, 100_000, MoraleKind::Steady),
        700_000
    );
    assert_eq!(
        effective_ceiling(g, 100_000, 300_000, MoraleKind::Wavering),
        0
    );
    assert_eq!(
        effective_ceiling(g, 900_000, 0, MoraleKind::Broken),
        g.broken_ceiling
    );
    assert_eq!(
        effective_ceiling(g, 300_000, 0, MoraleKind::Broken),
        300_000
    );
    assert_eq!(
        effective_ceiling(g, 900_000, 0, MoraleKind::FallingBack),
        900_000
    );
    assert_eq!(
        effective_ceiling(g, 900_000, 0, MoraleKind::Rallied),
        900_000
    );
}

#[test]
fn officer_factor_runs_from_half_to_one() {
    assert_eq!(officer_factor_pm(10, 10), 1000);
    assert_eq!(officer_factor_pm(0, 10), 500);
    assert_eq!(officer_factor_pm(5, 10), 750);
    assert_eq!(officer_factor_pm(1, 2), 750);
    assert_eq!(officer_factor_pm(0, 0), 500, "no officers at all");
}

#[test]
fn relax_step_hand_values() {
    let t = table();
    let g = &t.globals;
    let row = t.training(Training::Trained); // up 40, rally 14
                                             // Above the ceiling: 80 permille of the gap per second, truncating toward zero.
    assert_eq!(
        relax_step(g, row, MoraleKind::Steady, 900_000, 800_000, 1000, 0, false),
        892_000
    );
    assert_eq!(
        relax_step(g, row, MoraleKind::Steady, 800_011, 800_000, 1000, 0, false),
        800_011
    );
    // Below the ceiling, steadying: 40 permille, scaled by the officer factor.
    assert_eq!(
        relax_step(g, row, MoraleKind::Steady, 500_000, 800_000, 1000, 0, false),
        512_000
    );
    assert_eq!(
        relax_step(
            g,
            row,
            MoraleKind::Wavering,
            500_000,
            800_000,
            500,
            0,
            false
        ),
        506_000
    );
    assert_eq!(
        relax_step(g, row, MoraleKind::Rallied, 500_000, 800_000, 1000, 0, true),
        512_000
    );
    // Rallying: 14 permille, with the leader gain `1000 + rally_leader_pm * leader / 1000`.
    assert_eq!(
        relax_step(g, row, MoraleKind::Broken, 100_000, 500_000, 1000, 0, false),
        105_600
    );
    assert_eq!(
        relax_step(
            g,
            row,
            MoraleKind::FallingBack,
            100_000,
            500_000,
            1000,
            1000,
            false
        ),
        111_200
    );
    assert_eq!(
        relax_step(g, row, MoraleKind::Broken, 100_000, 500_000, 500, 0, false),
        102_800
    );
    // Pressure stops recovery in FallingBack and Broken, not the fall.
    assert_eq!(
        relax_step(
            g,
            row,
            MoraleKind::Broken,
            100_000,
            500_000,
            1000,
            1000,
            true
        ),
        100_000
    );
    assert_eq!(
        relax_step(
            g,
            row,
            MoraleKind::FallingBack,
            100_000,
            500_000,
            1000,
            0,
            true
        ),
        100_000
    );
    assert_eq!(
        relax_step(g, row, MoraleKind::Broken, 600_000, 500_000, 1000, 0, true),
        592_000
    );
}

#[test]
fn window_decays_by_an_eighth() {
    assert_eq!(decay_window(0), 0);
    assert_eq!(decay_window(7), 7);
    assert_eq!(decay_window(8), 7);
    assert_eq!(decay_window(8000), 7000);
    assert_eq!(decay_window(u32::MAX), u32::MAX - u32::MAX / 8);
}

proptest! {
    #[test]
    fn target_stays_in_range(
        base in 0u32..=1_000_000,
        bc in any::<u8>(),
        v in proptest::array::uniform9(0u32..=1_000_000),
    ) {
        let t = table();
        let target = ceiling_target(&t.globals, base, bc, &inputs(v));
        prop_assert!(target <= 1_000_000);
        // Independent evaluation in i128.
        let pen: i128 = v[..6].iter().map(|x| i128::from(*x)).sum();
        let bon: i128 = v[6..].iter().map(|x| i128::from(*x)).sum();
        let want = (i128::from(base) - i128::from(t.globals.repeat_pen) * i128::from(bc.min(3)) - pen + bon)
            .clamp(0, 1_000_000);
        prop_assert_eq!(i128::from(target), want);
    }

    #[test]
    fn effective_ceiling_is_bounded(
        target in 0u32..=1_000_000,
        loss in 0u32..=1_000_000,
        k in 0usize..5,
    ) {
        let t = table();
        let kind = MoraleKind::ALL[k];
        let tc = effective_ceiling(&t.globals, target, loss, kind);
        prop_assert!(tc <= target.saturating_sub(loss));
        if kind != MoraleKind::Broken {
            prop_assert_eq!(tc, target.saturating_sub(loss));
        }
    }

    #[test]
    fn constant_inputs_converge_without_overshoot(
        c0 in 0u32..=1_000_000,
        tc in 0u32..=1_000_000,
        level in 0usize..3,
        k in 0usize..5,
        alive in 0usize..=10,
        leader_pm in 0u16..=1000,
    ) {
        let t = table();
        let row = t.training(Training::ALL[level]);
        let kind = MoraleKind::ALL[k];
        let o = officer_factor_pm(alive, 10);
        let mut c = c0;
        let mut last_gap = (i64::from(tc) - i64::from(c)).abs();
        for _ in 0..6000 {
            let next = relax_step(&t.globals, row, kind, c, tc, o, leader_pm, false);
            // Never crosses the ceiling, never moves away from it.
            if c <= tc {
                prop_assert!(next >= c && next <= tc);
            } else {
                prop_assert!(next <= c && next >= tc);
            }
            let gap = (i64::from(tc) - i64::from(next)).abs();
            prop_assert!(gap <= last_gap);
            last_gap = gap;
            c = next;
        }
        prop_assert!(last_gap <= 1000, "gap {} after 6000 seconds", last_gap);
    }
}

#[test]
fn a_slow_period_relaxes_every_regiment_from_the_frozen_state() {
    let mut b = field(1, '.');
    line(&mut b, 0, 40, m(10, 10));
    line(&mut b, 0, 40, m(300, 10));
    b.enable_morale();
    let t = table();
    let g = &t.globals;
    let row = t.training(Training::Trained);
    {
        let ms = b.morale.as_mut().unwrap();
        ms.regs[0].c = 0;
        ms.regs[0].window = 8000;
        ms.regs[1].c = 1_000_000;
        ms.regs[1].loss = 50_000;
    }
    b.relax_morale();
    let ms = b.morale.as_ref().unwrap();
    // No weapon issued: the empty-magazine penalty is the only term.
    let tgt = row.base_ppm - g.ammo_pen;
    assert_eq!(ms.regs[0].c, tgt * row.up_pm / 1000);
    assert_eq!(ms.regs[0].window, 7000);
    let tc = i64::from(tgt - 50_000);
    let gap = tc - 1_000_000;
    assert_eq!(
        i64::from(ms.regs[1].c),
        1_000_000 + gap * i64::from(g.down_pm) / 1000
    );
    // Nothing else moved: no state change, no event, no draw.
    assert!(ms
        .regs
        .iter()
        .all(|r| r.kind == MoraleKind::Steady && r.state_ticks == 0));
    assert!(ms.events.is_empty());
}

#[test]
fn leader_strength_is_stored_by_the_slow_period() {
    let mut b = field(1, '.');
    line(&mut b, 0, 40, m(100, 100));
    b.enable_morale();
    assert_eq!(b.morale.as_ref().unwrap().regs[0].leader_pm, 0);
    assert!(b.set_leader(battle_rules::morale::Leader {
        id: 1,
        team: 0,
        pos: m(110, 100),
        prestige_pm: 650,
        alive: true,
    }));
    b.relax_morale();
    assert_eq!(b.morale.as_ref().unwrap().regs[0].leader_pm, 650);
}
