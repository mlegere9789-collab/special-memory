//! T11-03: the driver applies a tick's inputs in `seq` order, then steps; the history hash covers
//! the trigger log; the replay it writes verifies (SPEC-011 sections 4.2 and 8, amendment A1).

mod common;

use battle_rules::registry::ACTOR_HISTORY_SCRIPT;
use battle_rules::{
    encode_order, scenarios, BattleState, Formation, Order, RegimentId, RegimentSpec, Training,
};
use common::{compile, order, script, tables, trigger, units};
use core_math::{Angle, Vec2Fx};
use history_cal::{
    encode_trigger, stage_battle, DriverError, ReplayDriver, ScriptInput, TriggerHook,
};
use std::cell::RefCell;
use std::rc::Rc;

fn staged(s: &history_cal::ReplayScript) -> BattleState {
    stage_battle(&units(), s, &tables(), 1).unwrap()
}

/// Spawns one regiment per trigger and records `(index, tick at the call, regiments after)`.
struct Spawner(Rc<RefCell<Vec<(u16, u64)>>>);

impl TriggerHook for Spawner {
    fn on_trigger(&mut self, state: &mut BattleState, index: u16) {
        self.0.borrow_mut().push((index, state.world.tick));
        state.spawn_regiment(RegimentSpec {
            team: 0,
            count: 10,
            training: Training::Trained,
            formation: Formation::Column,
            anchor: Vec2Fx::ZERO,
            heading: Angle::ZERO,
        });
    }
}

#[test]
fn a_tick_applies_its_inputs_then_steps() {
    let s = script(0, &[trigger("t_a", 0, ""), trigger("t_b", 5, "")].concat());
    let calls = Rc::new(RefCell::new(Vec::new()));
    let mut d = ReplayDriver::new(scenarios::march_single(&tables(), 1), compile(&s), &s).unwrap();
    d.set_hook(Box::new(Spawner(calls.clone())));
    d.run_to(100).unwrap();
    // The first trigger spawned regiment 1 during tick 0; regiment 2 does not exist yet.
    assert_eq!(d.state().regiments.len(), 2);
    assert_eq!(d.state().world.tick, 100);
    d.step().unwrap();
    assert_eq!(
        d.state().regiments.len(),
        3,
        "regiment 2 exists after the step at tick 100"
    );
    assert_eq!(d.state().world.tick, 101);
    assert_eq!(*calls.borrow(), vec![(0, 0), (1, 100)]);
    assert_eq!(d.timeline_log(), &[(0, 0), (100, 1)]);
}

#[test]
fn permuting_rows_with_different_times_changes_nothing() {
    let rows = [
        order("o1", "toy_r1", 10, "move"),
        order("o2", "toy_r2", 20, "form"),
        order("o3", "toy_b1", 15, "hold"),
        trigger("t1", 12, ""),
    ];
    let hash = |idx: [usize; 4]| {
        let text: String = idx.iter().map(|&i| rows[i].clone()).collect();
        let s = script(0, &text);
        let mut d = ReplayDriver::new(staged(&s), compile(&s), &s).unwrap();
        d.run_to(120).unwrap();
        d.history_hash()
    };
    assert_eq!(hash([0, 1, 2, 3]), hash([3, 2, 1, 0]));
    assert_eq!(hash([0, 1, 2, 3]), hash([1, 3, 0, 2]));
}

#[test]
fn same_tick_rows_keep_file_order() {
    let last_order = |a: &str, b: &str| {
        let s = script(
            0,
            &[order("oa", "toy_r1", 1, a), order("ob", "toy_r1", 1, b)].concat(),
        );
        let mut d = ReplayDriver::new(staged(&s), compile(&s), &s).unwrap();
        d.run_to(21).unwrap();
        d.state().regiments[1].order
    };
    assert_eq!(last_order("move", "hold"), Order::Hold);
    assert!(matches!(last_order("hold", "move"), Order::Move { .. }));
}

#[test]
fn removing_a_trigger_from_the_log_changes_the_hash() {
    let s = script(0, &trigger("t1", 2, ""));
    let inputs = compile(&s);
    let run = |inputs: Vec<ScriptInput>| {
        let mut d = ReplayDriver::new(staged(&s), inputs, &s).unwrap();
        d.run_to(100).unwrap();
        (d.history_hash(), d.state().hash())
    };
    let (with, battle_with) = run(inputs.clone());
    let (without, battle_without) = run(Vec::new());
    assert_eq!(
        battle_with, battle_without,
        "the marker hook leaves the battle alone"
    );
    assert_ne!(with, without);
}

fn bad_input(payload: Vec<u8>) -> DriverError {
    let s = script(0, &trigger("t1", 1, ""));
    let input = ScriptInput {
        tick: 0,
        seq: 0,
        payload,
    };
    let mut d = ReplayDriver::new(staged(&s), vec![input], &s).unwrap();
    d.step().unwrap_err()
}

#[test]
fn malformed_inputs_are_errors() {
    assert_eq!(bad_input(encode_trigger(1)), DriverError::UnknownTrigger(1));
    assert_eq!(bad_input(vec![0x30, 0, 0]), DriverError::UnknownTag(0x30));
    assert_eq!(bad_input(vec![0x7F]), DriverError::UnknownTag(0x7F));
    assert!(matches!(bad_input(vec![0x31, 0]), DriverError::Payload(_)));
    assert!(matches!(
        bad_input(vec![5, 0, 0, 1]),
        DriverError::Payload(_)
    ));
    assert_eq!(
        bad_input(encode_order(RegimentId(9), &Order::Hold)),
        DriverError::UnknownRegiment(9)
    );
}

#[test]
fn inputs_before_the_current_tick_are_refused() {
    let s = script(0, &trigger("t1", 0, ""));
    let mut b = staged(&s);
    b.step();
    let err = ReplayDriver::new(b, compile(&s), &s).err();
    assert_eq!(err, Some(DriverError::PastInput { tick: 0 }));
}

#[test]
fn fire_at_inputs_in_both_forms_are_applied() {
    let s = script(0, "");
    let five = ScriptInput {
        tick: 0,
        seq: 0,
        payload: vec![5, 1, 0, 0, 0],
    };
    let six = history_cal::ScriptInput {
        tick: 0,
        seq: 1,
        payload: battle_rules::encode_fire_mode(
            RegimentId(2),
            battle_rules::FireMode::AtWill {
                target: RegimentId(0),
            },
        ),
    };
    let mut d = ReplayDriver::new(staged(&s), vec![five, six], &s).unwrap();
    d.step().unwrap();
}

#[test]
fn the_written_replay_carries_the_script_actor_and_reproduces() {
    let s = script(
        0,
        &[order("o1", "toy_r1", 1, "move"), trigger("t1", 2, "")].concat(),
    );
    let inputs = compile(&s);
    let d = ReplayDriver::new(staged(&s), inputs.clone(), &s).unwrap();
    let replay = d.record("toy_replay/nominal", 1, 0, 120, 40).unwrap();
    assert_eq!(replay.scenario_id(), "toy_replay/nominal");
    assert_eq!(replay.inputs().len(), inputs.len());
    assert!(replay
        .inputs()
        .iter()
        .all(|i| i.actor == ACTOR_HISTORY_SCRIPT));
    assert_eq!(
        replay
            .checkpoints()
            .iter()
            .map(|c| c.tick)
            .collect::<Vec<_>>(),
        vec![40, 80, 120]
    );
    let mut again = ReplayDriver::new(staged(&s), inputs, &s).unwrap();
    again.run_to(120).unwrap();
    assert_eq!(replay.final_hash(), again.history_hash());
}
