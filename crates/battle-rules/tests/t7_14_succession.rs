//! T7-14: a message to a fallen officer reaches his deputy after `redirect_ticks`; with no deputy
//! it is lost.

mod t73_support;

use battle_rules::command::{encode_script, Medium, OfficerId, ScriptEvent, Truth};
use t73_support::*;

fn down(b: &mut battle_rules::BattleState, id: u16) {
    let p = encode_script(ScriptEvent::OfficerDown(OfficerId(id)));
    b.apply_command_input(0xFE, &p).unwrap();
}

#[test]
fn deputy_receives_after_the_redirect_delay() {
    let t = calm_table();
    let mut b = field(3, 1, metres(5000, 5000));
    place(&mut b, 5, 0, 0);
    place(&mut b, 2, 1000, 0);
    place(&mut b, 3, 1000, 0);
    let id = b
        .send_message(
            &t,
            OfficerId(5),
            OfficerId(2),
            Some(Medium::Courier),
            hold(),
        )
        .unwrap();
    let due = b.cmd.as_ref().unwrap().msgs[0].due;
    down(&mut b, 2);
    let got = run_until(&mut b, &t, due + 5000);
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].0, due + u64::from(t.globals.redirect_ticks));
    assert_eq!(got[0].1.msg.to, OfficerId(3));
    assert_eq!(got[0].1.msg.id, id);
}

#[test]
fn no_deputy_means_lost() {
    let t = calm_table();
    let mut b = field(3, 1, metres(5000, 5000));
    place(&mut b, 5, 0, 0);
    place(&mut b, 1, 1000, 0);
    let id = b
        .send_message(
            &t,
            OfficerId(5),
            OfficerId(1),
            Some(Medium::Courier),
            hold(),
        )
        .unwrap();
    down(&mut b, 1);
    let got = run_until(&mut b, &t, 30_000);
    assert!(got.is_empty());
    let cmd = b.cmd.as_ref().unwrap();
    assert!(cmd.msgs.is_empty());
    assert_eq!(
        cmd.records.iter().find(|r| r.id == id).unwrap().truth,
        Truth::Lost
    );
}
