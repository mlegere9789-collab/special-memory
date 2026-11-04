//! T8-10: stream continuity. A soldier's stream position is a function of the shots it has
//! fired, never of whether they were F0 or F1; materialised soldiers take fresh serial streams;
//! unrelated hand-overs change nothing (SPEC-005 amendment A7, SPEC-008 section 5).

mod t083_support;

use battle_rules::aim::{draw_shot, SHOT_WORDS};
use battle_rules::fire::discard_shot_words;
use battle_rules::registry::STREAM_MATERIALISED_BASE;
use battle_rules::{condense, materialise, soldier_stream, step_ticks, RegimentId, F0};
use core_math::{Rng, RngStreams, StreamId};
use t083_support::{member, six};

#[test]
fn every_f0_f1_pattern_of_ten_shots_follows_the_pure_f0_stream() {
    let streams = RngStreams::new(77);
    let id = soldier_stream(5);
    let mut pure = streams.stream(id);
    let mut states = Vec::new();
    let mut words = Vec::new();
    for _ in 0..10 {
        words.push(draw_shot(&mut pure).misfire_roll);
        states.push(pure.state());
    }
    for pattern in 0u32..1024 {
        let mut rng = streams.stream(id);
        for k in 0..10 {
            let roll = if pattern >> k & 1 == 1 {
                discard_shot_words(&mut rng)
            } else {
                draw_shot(&mut rng).misfire_roll
            };
            assert_eq!(roll, words[k], "pattern {pattern:010b} shot {k}");
            assert_eq!(rng.state(), states[k], "pattern {pattern:010b} shot {k}");
        }
    }
}

#[test]
fn a_shot_is_fifty_words() {
    assert_eq!(SHOT_WORDS, 50);
    let mut a = Rng::from_seed(9);
    let mut b = Rng::from_seed(9);
    discard_shot_words(&mut a);
    for _ in 0..50 {
        b.next_u64();
    }
    assert_eq!(a.state(), b.state());
}

#[test]
fn materialised_soldiers_take_serial_streams() {
    let mut battle = six(3);
    for _ in 0..10 {
        battle.step();
    }
    assert!(condense(&mut battle, RegimentId(0)));
    let serial0 = battle.soldier_serial;
    assert!(materialise(&mut battle, RegimentId(0), F0));
    assert_eq!(battle.soldier_serial, serial0 + 60);
    let streams = RngStreams::new(3);
    let w = &battle.tables.weapons.rows()[battle.tables.weapons.index_of("rifle_58").unwrap()];
    let first = step_ticks(w, 0, battle.regiments[0].training, 0);
    let layout = battle_rules::slot_layout(
        battle.regiments[0].state.target_formation(),
        60,
        &battle.tables.drill,
    );
    for (k, slot) in layout.iter().enumerate() {
        let a = *battle.regiments[0]
            .roster
            .iter()
            .find(|a| battle.loco[a.index as usize].slot == *slot)
            .unwrap();
        let mut rng = streams.stream(StreamId(STREAM_MATERIALISED_BASE + serial0 + k as u32));
        let left = rng.range_u32(0, u32::from(first)) as u16;
        let f = &battle.fire[a.index as usize];
        assert_eq!(
            (f.rng, f.left, f.step, f.mag),
            (rng.state(), left, 0, 0),
            "soldier {k}"
        );
    }
    // A second hand-over never reuses a serial, even though the slots are recycled.
    assert!(condense(&mut battle, RegimentId(0)));
    assert!(materialise(&mut battle, RegimentId(0), F0));
    assert_eq!(battle.soldier_serial, serial0 + 120);
    let a = *battle.regiments[0]
        .roster
        .iter()
        .find(|a| battle.loco[a.index as usize].slot == layout[0])
        .unwrap();
    let mut rng = streams.stream(StreamId(STREAM_MATERIALISED_BASE + serial0 + 60));
    rng.range_u32(0, u32::from(first));
    assert_eq!(battle.fire[a.index as usize].rng, rng.state());
}

#[test]
fn an_unrelated_hand_over_leaves_a_soldiers_stream_alone() {
    let run = |hand_over: bool| {
        let mut b = six(8);
        for t in 0..500u32 {
            if hand_over && t == 40 {
                assert!(condense(&mut b, RegimentId(2)));
            }
            if hand_over && t == 90 {
                assert!(materialise(&mut b, RegimentId(2), F0));
            }
            b.step();
        }
        let x = member(&b, 0, 7);
        (
            b.fire[x.index as usize].rng,
            b.fire[x.index as usize].rounds,
        )
    };
    let (plain, rounds) = run(false);
    assert_eq!(run(true), (plain, rounds));
    assert!(rounds < 60, "the soldier fired during the run");
}
