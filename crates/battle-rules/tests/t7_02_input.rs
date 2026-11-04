//! T7-02 and T7-11 (SPEC-007 sections 4 and 9): `apply_command_input` side checks, the
//! direct-order refusal, option and script effects, and officer position refresh.

mod t7_support;

use battle_rules::command::{
    encode_dispatch, encode_options, encode_script, CommandOptions, CommandTable, DispatchError,
    InputError, Latitude, OfficerId, ScriptEvent,
};
use battle_rules::{encode_fire_mode, encode_order, FireMode, Formation, Gait, Order, RegimentId};
use core_math::{Fx, Vec2Fx};
use t7_support::{command_battle, plain_battle, TABLE};

const FLAGS_ON: CommandOptions = CommandOptions {
    omniscient_view: false,
    instant_orders: true,
};

fn mv() -> Order {
    Order::Move {
        to: Vec2Fx::new(Fx::from_int(5), Fx::from_int(6)),
        gait: Gait::DoubleQuick,
    }
}

#[test]
fn seats_are_refused_direct_orders_when_the_command_layer_is_on() {
    let mut b = command_battle();
    let hash = b.hash();
    for actor in [0x00u8, 0x01, 0x80, 0x81, 0x02, 0x40] {
        for payload in [
            encode_order(RegimentId(0), &mv()),
            encode_order(RegimentId(0), &Order::Hold),
            encode_fire_mode(RegimentId(0), FireMode::HoldFire),
        ] {
            assert_eq!(
                b.apply_command_input(actor, &payload),
                Err(InputError::DirectOrder),
                "actor {actor:#x}"
            );
        }
    }
    assert_eq!(b.hash(), hash);
    assert_eq!(b.regiments[0].order, Order::Hold);
}

#[test]
fn scripts_and_instant_seats_are_not_refused() {
    let mut b = command_battle();
    for actor in [0xFDu8, 0xFE, 0xFF] {
        assert_eq!(
            b.apply_command_input(actor, &encode_order(RegimentId(0), &mv())),
            Ok(())
        );
        assert_eq!(b.regiments[0].order, mv());
        b.regiments[0].order = Order::Hold;
    }
    assert_eq!(
        b.apply_command_input(0xFE, &encode_fire_mode(RegimentId(0), FireMode::HoldFire)),
        Ok(())
    );
    // Instant orders of side 0 free the seats of side 0 only.
    assert_eq!(
        b.apply_command_input(0x00, &encode_options(0, FLAGS_ON)),
        Ok(())
    );
    let order = encode_order(RegimentId(1), &Order::Form(Formation::Column));
    assert_eq!(b.apply_command_input(0x00, &order), Ok(()));
    assert_eq!(b.apply_command_input(0x80, &order), Ok(()));
    assert_eq!(
        b.apply_command_input(0x01, &order),
        Err(InputError::DirectOrder)
    );
    // A seat still cannot order the other side's regiment, or one that does not exist.
    let enemy = encode_order(RegimentId(2), &mv());
    assert_eq!(
        b.apply_command_input(0x00, &enemy),
        Err(InputError::WrongSide)
    );
    let none = encode_order(RegimentId(9), &mv());
    assert_eq!(
        b.apply_command_input(0x00, &none),
        Err(InputError::UnknownRegiment)
    );
}

#[test]
fn without_the_command_layer_direct_orders_apply_and_command_tags_are_refused() {
    let mut b = plain_battle();
    assert_eq!(
        b.apply_command_input(0x00, &encode_order(RegimentId(0), &mv())),
        Ok(())
    );
    assert_eq!(b.regiments[0].order, mv());
    let d = encode_dispatch(
        OfficerId(1),
        OfficerId(2),
        0,
        Latitude::Strict,
        &Order::Hold,
    );
    assert_eq!(
        b.apply_command_input(0x00, &d),
        Err(InputError::NoCommandLayer)
    );
    assert_eq!(
        b.apply_command_input(0xFE, &encode_script(ScriptEvent::OfficerDown(OfficerId(1)))),
        Err(InputError::NoCommandLayer)
    );
}

#[test]
fn undecodable_payloads_change_nothing() {
    let mut b = command_battle();
    let hash = b.hash();
    for payload in [&[][..], &[0x11, 0, 0], &[0x20, 1], &[0x10, 1], &[0x14]] {
        assert!(b.apply_command_input(0x00, payload).is_err());
    }
    assert_eq!(b.hash(), hash);
}

#[test]
fn options_apply_to_the_seats_own_side() {
    let mut b = command_battle();
    let omni = CommandOptions {
        omniscient_view: true,
        instant_orders: false,
    };
    assert_eq!(
        b.apply_command_input(0x01, &encode_options(1, omni)),
        Ok(())
    );
    assert_eq!(
        b.cmd.as_ref().unwrap().opt,
        [CommandOptions::default(), omni]
    );
    assert_eq!(
        b.apply_command_input(0x00, &encode_options(1, omni)),
        Err(InputError::WrongActor)
    );
    assert_eq!(
        b.apply_command_input(0x81, &encode_options(1, FLAGS_ON)),
        Ok(())
    );
    assert_eq!(b.cmd.as_ref().unwrap().opt[1], FLAGS_ON);
    assert_eq!(
        b.apply_command_input(0xFE, &encode_options(0, FLAGS_ON)),
        Ok(())
    );
    assert_eq!(b.cmd.as_ref().unwrap().opt[0], FLAGS_ON);
}

#[test]
fn script_events_move_officers_and_links() {
    let mut b = command_battle();
    let down = |b: &mut battle_rules::BattleState, id: u16| {
        b.cmd
            .as_ref()
            .unwrap()
            .officers
            .iter()
            .find(|o| o.id.0 == id)
            .unwrap()
            .down
    };
    let cut = |b: &mut battle_rules::BattleState| {
        b.cmd
            .as_ref()
            .unwrap()
            .links
            .iter()
            .map(|l| l.cut)
            .collect::<Vec<_>>()
    };
    let run = |b: &mut battle_rules::BattleState, actor, e| {
        b.apply_command_input(actor, &encode_script(e))
    };
    let (a, c) = (OfficerId(2), OfficerId(1));
    assert_eq!(run(&mut b, 0xFE, ScriptEvent::OfficerDown(a)), Ok(()));
    assert!(down(&mut b, 2));
    assert_eq!(run(&mut b, 0xFE, ScriptEvent::OfficerUp(a)), Ok(()));
    assert!(!down(&mut b, 2));
    assert_eq!(cut(&mut b), [false, false]);
    // Links are symmetric: the order of the two ids does not matter.
    let (x, y) = (OfficerId(11), OfficerId(10));
    assert_eq!(run(&mut b, 0xFE, ScriptEvent::LinkCut(x, y)), Ok(()));
    assert_eq!(cut(&mut b), [false, true]);
    assert_eq!(run(&mut b, 0xFE, ScriptEvent::LinkRestore(y, x)), Ok(()));
    assert_eq!(cut(&mut b), [false, false]);
    assert_eq!(
        run(&mut b, 0xFE, ScriptEvent::OfficerDown(OfficerId(99))),
        Err(InputError::UnknownOfficer(OfficerId(99)))
    );
    assert_eq!(
        run(&mut b, 0xFE, ScriptEvent::LinkCut(a, c)),
        Err(InputError::UnknownLink)
    );
    // Only script actors may write script events.
    assert_eq!(
        run(&mut b, 0x00, ScriptEvent::OfficerDown(a)),
        Err(InputError::WrongActor)
    );
    assert_eq!(
        run(&mut b, 0x80, ScriptEvent::OfficerDown(a)),
        Err(InputError::WrongActor)
    );
}

#[test]
fn dispatch_checks_sides_and_state() {
    let mut b = command_battle();
    let send = |b: &mut battle_rules::BattleState, actor, from, to| {
        let d = encode_dispatch(OfficerId(from), OfficerId(to), 0, Latitude::Strict, &mv());
        b.apply_command_input(actor, &d)
    };
    assert_eq!(send(&mut b, 0x00, 1, 2), Ok(()));
    assert_eq!(send(&mut b, 0x80, 1, 6), Ok(()));
    assert_eq!(
        send(&mut b, 0x01, 1, 2),
        Err(InputError::Dispatch(DispatchError::WrongSide))
    );
    assert_eq!(
        send(&mut b, 0x00, 1, 11),
        Err(InputError::Dispatch(DispatchError::WrongSide))
    );
    assert_eq!(
        send(&mut b, 0x00, 1, 77),
        Err(InputError::Dispatch(DispatchError::UnknownOfficer(
            OfficerId(77)
        )))
    );
    assert_eq!(send(&mut b, 0xFE, 1, 2), Err(InputError::WrongActor));
    b.apply_command_input(0xFE, &encode_script(ScriptEvent::OfficerDown(OfficerId(1))))
        .unwrap();
    assert_eq!(
        send(&mut b, 0x00, 1, 2),
        Err(InputError::Dispatch(DispatchError::SenderDown))
    );
}

#[test]
fn officer_positions_follow_the_regiments() {
    let mut b = command_battle();
    let table = CommandTable::from_toml_str(TABLE).unwrap();
    let m = |x: i32, y: i32| Vec2Fx::new(Fx::from_int(x), Fx::from_int(y));
    let pos = |b: &battle_rules::BattleState, id: u16| {
        b.cmd
            .as_ref()
            .unwrap()
            .officers
            .iter()
            .find(|o| o.id.0 == id)
            .unwrap()
            .pos
    };
    assert_eq!(pos(&b, 5), m(10, 20));
    assert_eq!(pos(&b, 6), m(30, 40));
    assert_eq!(pos(&b, 2), m(20, 30)); // mean of regiments 0 and 1
    assert_eq!(pos(&b, 1), m(20, 30));
    assert_eq!(pos(&b, 14), m(50, 60));
    // A fixed headquarters wins over the formation: (1000, 2000) mm.
    assert_eq!(
        pos(&b, 11),
        Vec2Fx::new(
            battle_rules::fx_ratio(1_000_000, 1000),
            battle_rules::fx_ratio(2_000_000, 1000)
        )
    );
    // Positions refresh on the interval only (pos_interval_ticks = 20).
    b.regiments[0].anchor = m(110, 20);
    b.world.tick = 7;
    b.step_command(&table);
    assert_eq!(pos(&b, 5), m(10, 20));
    b.world.tick = 20;
    b.step_command(&table);
    assert_eq!(pos(&b, 5), m(110, 20));
    assert_eq!(pos(&b, 2), m(70, 30));
    // A regiment with no men no longer counts; a formation with none keeps its last position.
    b.regiments[0].roster.clear();
    b.regiments[1].roster.clear();
    b.world.tick = 40;
    b.step_command(&table);
    assert_eq!(pos(&b, 2), m(70, 30));
    assert_eq!(pos(&b, 5), m(110, 20));
}

#[test]
fn enable_command_refuses_unknown_regiments_and_unsorted_officers() {
    let mut b = plain_battle();
    let (mut officers, links) = t7_support::officers_and_links(&t7_support::officer_set());
    officers[4].regiment = Some(RegimentId(40));
    assert!(b.enable_command(officers.clone(), links.clone()).is_err());
    assert!(b.cmd.is_none());
    officers[4].regiment = Some(RegimentId(0));
    officers.swap(0, 1);
    assert!(b.enable_command(officers, links).is_err());
}
