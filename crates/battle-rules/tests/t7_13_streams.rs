//! T7-13: an extra unrelated dispatch leaves every other message's fate, garble and arrival tick
//! unchanged.

mod t73_support;

use battle_rules::command::{Medium, OfficerId};
use t73_support::*;

type Outcome = Vec<(u64, u32, Option<u8>, bool)>;

fn run(extra: bool) -> (Outcome, Vec<String>) {
    let t = table();
    let mut b = field(11, 30, metres(1500, 0));
    place(&mut b, 1, 0, 0);
    place(&mut b, 2, 2500, 0);
    place(&mut b, 3, 0, 1500);
    for k in 0..12 {
        let to = if k % 2 == 0 { 2 } else { 3 };
        b.send_message(
            &t,
            OfficerId(1),
            OfficerId(to),
            Some(Medium::Courier),
            walk_to(10 + k, 20),
        )
        .unwrap();
    }
    if extra {
        b.send_message(
            &t,
            OfficerId(5),
            OfficerId(6),
            Some(Medium::Courier),
            hold(),
        )
        .unwrap();
    }
    let mut seen = Vec::new();
    while b.world.tick < 50_000 {
        if extra && b.world.tick == 100 {
            b.send_message(
                &t,
                OfficerId(6),
                OfficerId(5),
                Some(Medium::Courier),
                hold(),
            )
            .unwrap();
        }
        for a in b.step_messages(&t) {
            if a.msg.id.0 < 12 {
                let r = a.reading.unwrap();
                seen.push((
                    b.world.tick,
                    a.msg.id.0,
                    r.garbled.map(|g| g as u8),
                    r.stale,
                ));
            }
        }
        b.world.advance_tick();
    }
    seen.sort();
    let truths = b
        .cmd
        .as_ref()
        .unwrap()
        .records
        .iter()
        .filter(|r| r.id.0 < 12)
        .map(|r| format!("{} {:?} {:?}", r.id.0, r.truth, r.garble))
        .collect();
    (seen, truths)
}

#[test]
fn an_unrelated_dispatch_changes_nothing_else() {
    let (a, ta) = run(false);
    let (b, tb) = run(true);
    assert!(!a.is_empty());
    assert_eq!(a, b);
    assert_eq!(ta, tb);
}
