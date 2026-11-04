//! T9-17 (peek and inputs): every hashed peek field and every `EnvInputs` field changes the hash
//! when the environment is on, and none is hashed when it is off.

mod t022_support;
mod t9_support;

use battle_rules::EnvInputs;
use t022_support::{field, line, metres};
use t9_support::{env_table, weather};

use battle_rules::{BattleState, Training};
use core_math::Angle;

fn base(on: bool) -> BattleState {
    let mut b = field(4);
    b.spawn_regiment(line(0, 6, Training::Trained, metres(100, 200), Angle::ZERO));
    b.spawn_regiment(line(1, 6, Training::Trained, metres(300, 200), Angle::HALF));
    if on {
        b.set_environment(env_table(), weather(&[(0, 700, 800, 0, 0, 0)]));
    }
    b
}

#[test]
fn each_peek_field_changes_the_hash() {
    let b = base(true);
    let h0 = b.hash();
    let slot = b.regiments[1].roster[2].index as usize;
    let edits: [fn(&mut BattleState, usize); 3] = [
        |b, s| b.loco[s].peek_mm = 500,
        |b, s| b.loco[s].blind = true,
        |b, s| b.loco[s].fired = true,
    ];
    let mut seen = vec![h0];
    for edit in edits {
        let mut c = b.clone();
        edit(&mut c, slot);
        let h = c.hash();
        assert!(!seen.contains(&h));
        seen.push(h);
    }
}

#[test]
fn each_input_field_changes_the_hash() {
    let b = base(true);
    let h0 = b.hash();
    let one = EnvInputs {
        smoke_pm: 1,
        blind_pm: 0,
        view_blocked: false,
        heat_drain_pm: 0,
    };
    let variants = [
        one,
        EnvInputs {
            smoke_pm: 0,
            blind_pm: 1,
            ..one
        },
        EnvInputs {
            smoke_pm: 0,
            view_blocked: true,
            ..one
        },
        EnvInputs {
            smoke_pm: 0,
            heat_drain_pm: 1,
            ..one
        },
    ];
    let mut seen = vec![h0];
    for v in variants {
        let mut c = b.clone();
        *c.env.as_mut().unwrap().inputs_mut() = vec![EnvInputs::default(), v];
        let h = c.hash();
        assert!(!seen.contains(&h), "{v:?}");
        seen.push(h);
    }
}

#[test]
fn with_the_environment_off_nothing_of_it_is_hashed() {
    let b = base(false);
    let h0 = b.hash();
    let mut c = b.clone();
    let slot = c.regiments[0].roster[0].index as usize;
    c.loco[slot].peek_mm = 900;
    c.loco[slot].blind = true;
    c.regiments[0].view_blocked = true;
    assert_eq!(c.hash(), h0);
}

#[test]
fn the_period_step_writes_the_inputs() {
    let mut b = base(true);
    for _ in 0..20 {
        b.step();
    }
    let i = b.env_inputs(battle_rules::RegimentId(0)).unwrap();
    assert_eq!(i.heat_drain_pm, 1000);
    assert!(!i.view_blocked);
    assert!(b.env_inputs(battle_rules::RegimentId(9)).is_none());
}
