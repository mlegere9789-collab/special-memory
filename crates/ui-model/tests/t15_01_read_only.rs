//! T15-01 read-only: the interface never changes the battle, and only `commands.rs` may.

mod common;

use battle_rules::{
    battle_hash, decode_order, encode_order, BattleState, Formation, Gait, Order, RegimentId,
};
use common::{battle, metres, steady, Canned, CannedMorale};
use core_math::Angle;
use proptest::prelude::*;
use ui_model::{
    capture, snapshot_hash, soldier_frame, InfoMode, Route, SoldierFrame, UiAction, UiCommand,
    UiMode, UiState,
};

#[derive(Clone, Debug)]
enum Op {
    Ui(UiAction),
    Step(u8),
    Capture,
}

fn mode() -> impl Strategy<Value = UiMode> {
    (0..UiMode::ALL.len()).prop_map(|i| UiMode::ALL[i])
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        proptest::option::of(0u16..4).prop_map(|s| Op::Ui(UiAction::Select(s))),
        any::<bool>().prop_map(|p| Op::Ui(UiAction::Pause(p))),
        (-5000i64..5000, -5000i64..5000).prop_map(|(x, y)| Op::Ui(UiAction::Pan([x, y]))),
        (0u8..4).prop_map(|k| Op::Ui(UiAction::ToggleOverlay(k))),
        mode().prop_map(|m| Op::Ui(UiAction::SetMode(m))),
        (0u16..20_000).prop_map(|s| Op::Ui(UiAction::SetSpeed(s))),
        Just(Op::Ui(UiAction::Draft(Some(Order::Form(Formation::Line))))),
        (1u8..6).prop_map(Op::Step),
        Just(Op::Capture),
    ]
}

fn start() -> BattleState {
    battle(
        7,
        &[
            (0, 12, Formation::Column, 0, 0),
            (1, 12, Formation::Line, 0, 120),
        ],
    )
}

fn run_with_interface(ops: &[Op]) -> u64 {
    let mut b = start();
    let mut ui = UiState::new(false);
    let info = Canned {
        sightings: vec![],
        orders: vec![],
    };
    let morale = CannedMorale(steady());
    let mut frame = SoldierFrame::default();
    for o in ops {
        match o {
            Op::Ui(a) => {
                let _ = ui.apply(a.clone());
            }
            Op::Step(n) => (0..*n).for_each(|_| b.step()),
            Op::Capture => {
                let s = capture(&b, &info, &morale, InfoMode::Hq { side: 0 }, 0);
                let _ = snapshot_hash(&s);
                soldier_frame(&b, &mut frame);
            }
        }
    }
    battle_hash(&b)
}

fn run_without_interface(ops: &[Op]) -> u64 {
    let mut b = start();
    for o in ops {
        if let Op::Step(n) = o {
            (0..*n).for_each(|_| b.step());
        }
    }
    battle_hash(&b)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]
    #[test]
    fn interface_leaves_battle_hash_unchanged(ops in proptest::collection::vec(op(), 0..24)) {
        prop_assert_eq!(run_with_interface(&ops), run_without_interface(&ops));
    }
}

#[test]
fn only_commands_rs_may_change_the_battle() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut scanned = 0;
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.file_name().unwrap() == "commands.rs" {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        for needle in ["issue_order(", "spawn_", "&mut BattleState"] {
            assert!(
                !text.contains(needle),
                "{} contains {needle}",
                path.display()
            );
        }
        scanned += 1;
    }
    assert!(scanned >= 5, "the scan found only {scanned} files");
}

fn go_east() -> UiCommand {
    UiCommand::Order {
        regiment: RegimentId(1),
        order: Order::Move {
            to: metres(30, 0),
            gait: Gait::Quick,
        },
    }
}

#[test]
fn direct_order_becomes_an_input_record() {
    let rec = go_east().to_input(40, 1, 3, Route::Direct);
    assert_eq!((rec.tick, rec.actor, rec.seq), (40, 1, 3));
    let (r, o) = decode_order(&rec.payload).unwrap();
    assert_eq!(
        (r, o),
        (
            RegimentId(1),
            Order::Move {
                to: metres(30, 0),
                gait: Gait::Quick
            }
        )
    );
    // The record is accepted by the simulation when the command layer is off.
    let mut b = start();
    b.apply_command_input(rec.actor, &rec.payload).unwrap();
    assert_eq!(b.regiments[1].order, o);
}

#[test]
fn dispatch_route_uses_tag_0x10_and_the_seat_officer() {
    use battle_rules::command::{decode_dispatch, Latitude, OfficerId};
    let route = Route::Dispatch {
        from: OfficerId(5),
        to: OfficerId(9),
        medium: 0,
        latitude: Latitude::Strict,
    };
    let rec = go_east().to_input(0, 1, 0, route);
    assert_eq!((rec.actor, rec.payload[0]), (1, 0x10));
    let d = decode_dispatch(&rec.payload).unwrap();
    assert_eq!((d.from, d.to), (OfficerId(5), OfficerId(9)));
    let bytes = encode_order(RegimentId(9), &Order::Face(Angle::ZERO));
    assert_eq!(decode_order(&bytes).unwrap().1, Order::Face(Angle::ZERO));
}

#[test]
fn route_choice_follows_the_command_layer() {
    use battle_rules::command::OfficerId;
    let b = start();
    let direct = Route::choose(&b, 0, OfficerId(1), OfficerId(2));
    assert!(
        matches!(direct, Route::Direct),
        "layer off gives a direct order"
    );
}
