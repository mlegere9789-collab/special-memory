//! T8-03, the `Battle::hash` half (SPEC-008 sections 3, 9, 11): shuffled `[[unit]]`,
//! `[[arrival]]` and `[[trigger]]` rows give equal `scenario_hash` and equal `Battle::hash` at
//! tick 600; the hash extends `battle_hash` with the FID1 block and `BattleState::hash` stays
//! `battle_hash`.

mod t084_support;

use battle_rules::registry::MAGIC_FIDELITY;
use battle_rules::{battle_hash, battle_hash_into, scenario_hash, Battle, Scenario};
use sim_state::StateHasher;
use t084_support::{battle, four, scenario_text, tables, trigger};

const FID_SMALL: &str = include_str!("../../../data/scenarios/fid-small.toml");

/// Reverses the order of the `[[header]]` blocks of `text`, leaving everything else in place.
fn reverse_rows(text: &str, header: &str) -> String {
    let marker = format!("[[{header}]]");
    let mut blocks: Vec<String> = Vec::new();
    for line in text.split_inclusive('\n') {
        if line.starts_with("[[") || line.starts_with('[') || blocks.is_empty() {
            blocks.push(String::new());
        }
        blocks.last_mut().unwrap().push_str(line);
    }
    let mut rows: Vec<String> = blocks
        .iter()
        .filter(|b| b.starts_with(&marker))
        .cloned()
        .collect();
    rows.reverse();
    let mut rows = rows.into_iter();
    blocks
        .iter()
        .map(|b| {
            if b.starts_with(&marker) {
                rows.next().unwrap()
            } else {
                b.clone()
            }
        })
        .collect()
}

fn shuffled(text: &str) -> String {
    let text = reverse_rows(text, "unit");
    let text = reverse_rows(&text, "arrival");
    reverse_rows(&text, "trigger")
}

fn hash_at(text: &str, seed: u64, ticks: u64) -> (u64, u64) {
    let s = Scenario::from_toml_str(text, &tables(), None).unwrap();
    let mut b = Battle::from_scenario(&s, seed).unwrap();
    b.run(ticks);
    assert_eq!(b.state.world.tick, ticks);
    (scenario_hash(&s), b.hash())
}

#[test]
fn fid_small_rows_in_any_order_give_one_hash() {
    let text = shuffled(FID_SMALL);
    assert_ne!(text, FID_SMALL, "the rows moved");
    assert_eq!(hash_at(FID_SMALL, 7, 600), hash_at(&text, 7, 600));
}

#[test]
fn shuffled_triggers_give_one_hash() {
    let extra = [
        trigger("t_a", 2, "b1", "marker", ""),
        trigger("t_b", 2, "g1", "resupply", "rounds = 3"),
        trigger("t_c", 1, "b2", "order", "order = \"hold\""),
    ]
    .concat();
    let text = scenario_text(0, 60, &four(), &extra);
    let flipped = shuffled(&text);
    assert_ne!(text, flipped);
    assert_eq!(hash_at(&text, 3, 600), hash_at(&flipped, 3, 600));
}

#[test]
fn the_seed_and_the_tick_change_the_hash() {
    let text = scenario_text(0, 60, &four(), "");
    let base = hash_at(&text, 1, 100).1;
    assert_ne!(base, hash_at(&text, 2, 100).1);
    assert_ne!(base, hash_at(&text, 1, 101).1);
    assert_eq!(base, hash_at(&text, 1, 100).1);
}

#[test]
fn the_fid1_block_follows_battle_hash() {
    let mut b = battle(&scenario_text(0, 60, &four(), ""), 5);
    b.run(40);
    let mut h = StateHasher::new();
    battle_hash_into(&b.state, &mut h);
    assert_eq!(h.finish(), battle_hash(&b.state));
    assert_eq!(b.state.hash(), battle_hash(&b.state));
    // The wrapper adds the FID1 block.
    let mut h = StateHasher::new();
    battle_hash_into(&b.state, &mut h);
    h.write_u32(MAGIC_FIDELITY);
    assert_ne!(
        h.finish(),
        b.hash(),
        "the block carries fields after its magic"
    );
    assert_ne!(b.hash(), battle_hash(&b.state));
    // Fields of the wrapper are hashed: a trigger cursor, a class and a serial change it.
    let before = b.hash();
    b.fid.triggers_done += 1;
    assert_ne!(b.hash(), before);
    b.fid.triggers_done -= 1;
    assert_eq!(b.hash(), before);
    b.state.soldier_serial += 1;
    assert_ne!(b.hash(), before);
}
