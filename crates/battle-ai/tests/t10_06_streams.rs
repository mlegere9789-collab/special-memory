//! T10-06: AI streams are `STREAM_AI_BASE + side * 16 + layer`; adding an unrelated seat leaves
//! the other side's first 50 decisions unchanged.

mod support;

use battle_ai::{AiNote, Difficulty, SeatLevel, AI_ACTOR_BASE, STREAM_AI_BASE};
use core_math::{RngStreams, StreamId};
use support::{ai, run, seat, Draws, ScriptPort};

const SEED: u64 = 0xA11CE;

fn side_ai(side: u8, extra: bool) -> battle_ai::Ai {
    let mut seats = vec![
        seat(0, SeatLevel::Operational, None, ""),
        seat(1, SeatLevel::Tactical, Some(0), ""),
        seat(2, SeatLevel::Tactical, Some(0), ""),
    ];
    if extra {
        seats.push(seat(9, SeatLevel::Tactical, Some(0), ""));
    }
    let mut a = ai(side, Difficulty::Hard, &seats, SEED);
    a.set_planners(
        Box::new(Draws { unit: 4, n: 0 }),
        Box::new(Draws { unit: 5, n: 0 }),
    );
    a
}

fn first_50(a: &battle_ai::Ai) -> Vec<AiNote> {
    a.notes().iter().take(50).cloned().collect()
}

#[test]
fn the_constants_are_the_registry_values() {
    assert_eq!(STREAM_AI_BASE, 0x5400_0000);
    assert_eq!(AI_ACTOR_BASE, 0x80);
}

#[test]
fn layers_draw_from_their_own_named_streams() {
    for side in 0..2u8 {
        let mut a = side_ai(side, false);
        let mut p = ScriptPort::new(10);
        run(&mut a, &mut p, 2000);
        let streams = RngStreams::new(SEED);
        let id = |layer: u32| StreamId(STREAM_AI_BASE + u32::from(side) * 16 + layer);
        let mut ol = streams.stream(id(0));
        let mut tl = streams.stream(id(1));
        // Within a tick the seats draw in ascending seat id; layer 0 serves seat 0 and layer 1
        // seats 1 and 2.
        for n in a.notes() {
            let expect = if n.seat.0 == 0 {
                ol.next_u32()
            } else {
                tl.next_u32()
            };
            assert_eq!(
                n.v0,
                i64::from(expect),
                "side {side} seat {} tick {}",
                n.seat.0,
                n.tick
            );
        }
        assert!(a.notes().len() > 50);
    }
}

#[test]
fn an_unrelated_seat_leaves_the_other_sides_decisions_unchanged() {
    let alone = {
        let mut b = side_ai(1, false);
        let mut p = ScriptPort::new(10);
        run(&mut b, &mut p, 6000);
        (first_50(&b), b.hash(), b.notes().len())
    };
    // Both sides on one port, side 0 with an extra seat, stepped alternately.
    let mut a = side_ai(0, true);
    let mut b = side_ai(1, false);
    let mut pa = ScriptPort::new(10);
    let mut pb = ScriptPort::new(10);
    for t in 0..=6000u64 {
        pa.now = t;
        pb.now = t;
        a.step(&mut pa);
        b.step(&mut pb);
    }
    assert_eq!(first_50(&b), alone.0);
    assert_eq!(b.hash(), alone.1);
    assert_eq!(b.notes().len(), alone.2);
    // The extra seat changes side 0 itself.
    let mut plain = side_ai(0, false);
    let mut pp = ScriptPort::new(10);
    run(&mut plain, &mut pp, 6000);
    assert_ne!(plain.hash(), a.hash());
}

#[test]
fn the_seed_and_the_side_separate_the_draws() {
    let run_side = |side: u8| {
        let mut a = side_ai(side, false);
        let mut p = ScriptPort::new(10);
        run(&mut a, &mut p, 1000);
        a.notes().iter().map(|n| n.v0).collect::<Vec<_>>()
    };
    assert_ne!(run_side(0), run_side(1));
    assert_eq!(run_side(1), run_side(1));
}
