//! `battle_hash` (SPEC-004 section 11): a golden for a freshly spawned 400-man regiment, and a
//! check that every listed field is part of the hash.

mod t018_support;

use battle_rules::{
    battle_hash, BattleState, Formation, Gait, Order, RegimentSpec, RegimentState, Training,
};
use core_math::{Angle, Fx, Vec2Fx};
use t018_support::{battle_with, empty_battle, spec};

/// Recorded once from this implementation (seed 1, one 400-man Trained regiment of team 1 in
/// Column, anchor (100 m, 200 m), heading 0, tick 0).
///
/// Independently re-derived by a throwaway Python script that does not use this crate. It reads
/// only SPEC-004 sections 8 and 11 and the SPEC-002 world encoding: it builds the column slots
/// (8 files of 560 mm, ranks 900 mm apart, `u = (2*file - 7) * 560 / 2` mm, `v = -rank * 900` mm,
/// each converted with `raw = trunc((mm << 32) / 1000)`), places soldier k at
/// `(100 m + v, 200 m - u)` (heading 0 gives forward (1, 0) and right (0, -1)), writes the
/// little-endian byte stream (tick, seed, slot count, 400 agent slots, free-list length, one
/// regiment record with its 400 roster indices, 400 locomotion slots, three zero `u32` blocks) and
/// folds it with FNV-1a 64 (offset 0xcbf29ce484222325, prime 0x100000001b3). The script printed
/// the literal below and a stream length of 30098 bytes.
const GOLDEN_FRESH_400: u64 = 0x7494_b68e_1f12_b84b;

fn fresh_400() -> BattleState {
    battle_with(400, Training::Trained, Formation::Column).0
}

#[test]
fn fresh_400_man_regiment_matches_the_golden() {
    let b = fresh_400();
    assert_eq!(battle_hash(&b), GOLDEN_FRESH_400);
    assert_eq!(b.hash(), GOLDEN_FRESH_400);
}

#[test]
fn identical_builds_hash_identically_and_hashing_is_pure() {
    let (a, b) = (fresh_400(), fresh_400());
    assert_eq!(a.hash(), b.hash());
    assert_eq!(a.hash(), a.hash());
}

#[test]
fn spawned_men_stand_on_their_slots_with_ascending_ids() {
    let b = fresh_400();
    let r = &b.regiments[0];
    assert_eq!(r.roster.len(), 400);
    assert!(r.roster.windows(2).all(|w| w[0].index + 1 == w[1].index));
    let anchor = r.anchor;
    for (k, id) in r.roster.iter().enumerate() {
        let s = r.slots[k];
        assert_eq!(b.loco[id.index as usize].slot, s);
        let a = b.world.agents.get(*id).unwrap();
        // Heading 0: forward is +x, right is -y.
        assert_eq!(a.pos, Vec2Fx::new(anchor.x + s.v, anchor.y - s.u));
        assert_eq!((a.hp, a.flags, a.team, a.unit), (100, 0, 1, 0));
        assert_eq!(a.facing, Angle::ZERO);
    }
}

/// Applies `change` to a fresh two-regiment battle and expects a different hash.
fn changes_hash(name: &str, change: impl FnOnce(&mut BattleState)) {
    let mut b = empty_battle(1);
    b.spawn_regiment(spec(12, Training::Trained, Formation::Line));
    b.spawn_regiment(RegimentSpec {
        team: 2,
        ..spec(9, Training::Raw, Formation::Column)
    });
    let before = b.hash();
    change(&mut b);
    assert_ne!(b.hash(), before, "{name} is not in the hash");
}

#[test]
fn every_listed_field_changes_the_hash() {
    changes_hash("world tick", |b| b.world.advance_tick());
    changes_hash("an agent field", |b| {
        let id = b.regiments[0].roster[3];
        b.world.agents.set_hp(id, 99);
    });
    changes_hash("regiment count", |b| {
        b.spawn_regiment(spec(1, Training::Raw, Formation::Line));
    });
    changes_hash("regiment id", |b| b.regiments[1].id.0 = 9);
    changes_hash("team", |b| b.regiments[0].team = 3);
    changes_hash("training", |b| b.regiments[0].training = Training::Veteran);
    changes_hash("state tag", |b| {
        b.regiments[0].state = RegimentState::Forming {
            from: Formation::Line,
            to: Formation::Line,
            ticks_left: 0,
        }
    });
    changes_hash("steady formation", |b| {
        b.regiments[0].state = RegimentState::Steady(Formation::Skirmish)
    });
    let forming = |from, to, ticks_left| RegimentState::Forming {
        from,
        to,
        ticks_left,
    };
    // Each of from, to and ticks_left, against a common forming baseline.
    let base = || {
        let mut b = empty_battle(1);
        b.spawn_regiment(spec(12, Training::Trained, Formation::Line));
        b.regiments[0].state = forming(Formation::Line, Formation::Column, 10);
        b
    };
    let h0 = base().hash();
    for (name, state) in [
        ("from", forming(Formation::Skirmish, Formation::Column, 10)),
        ("to", forming(Formation::Line, Formation::Skirmish, 10)),
        (
            "ticks_left",
            forming(Formation::Line, Formation::Column, 11),
        ),
    ] {
        let mut b = base();
        b.regiments[0].state = state;
        assert_ne!(b.hash(), h0, "{name} is not in the hash");
    }
    changes_hash("anchor x", |b| b.regiments[0].anchor.x += Fx::EPSILON);
    changes_hash("anchor y", |b| b.regiments[0].anchor.y += Fx::EPSILON);
    changes_hash("heading", |b| b.regiments[0].heading = Angle::from_raw(1));
    changes_hash("order tag", |b| {
        b.regiments[0].order = Order::Face(Angle::ZERO)
    });
    let mv = |x: i64, y: i64, gait| Order::Move {
        to: Vec2Fx::new(Fx::from_raw(x), Fx::from_raw(y)),
        gait,
    };
    let mut seen = vec![];
    for (name, order) in [
        ("move baseline", mv(5, 6, Gait::Walk)),
        ("order target x", mv(7, 6, Gait::Walk)),
        ("order target y", mv(5, 8, Gait::Walk)),
        ("order gait", mv(5, 6, Gait::Run)),
        ("face angle", Order::Face(Angle::from_raw(77))),
        ("form target", Order::Form(Formation::Skirmish)),
    ] {
        let mut b = empty_battle(1);
        b.spawn_regiment(spec(12, Training::Trained, Formation::Line));
        b.regiments[0].order = order;
        let h = b.hash();
        assert!(!seen.contains(&h), "{name} collides with an earlier order");
        seen.push(h);
    }
    changes_hash("dirty", |b| b.regiments[0].dirty = true);
    changes_hash("roster length", |b| {
        b.regiments[1].roster.pop();
    });
    changes_hash("roster index", |b| {
        let last = b.regiments[0].roster.len() - 1;
        b.regiments[0].roster.swap(0, last);
    });
    changes_hash("slot count", |b| b.loco.push(Default::default()));
    changes_hash("fatigue", |b| b.loco[4].fatigue = 1);
    changes_hash("slot u", |b| b.loco[4].slot.u += Fx::EPSILON);
    changes_hash("slot v", |b| b.loco[4].slot.v += Fx::EPSILON);
    changes_hash("catchup", |b| b.loco[4].catchup = true);
}

#[test]
fn a_started_transition_is_hashed() {
    let (mut a, id) = battle_with(12, Training::Trained, Formation::Column);
    let before = a.hash();
    a.issue_order(id, Order::Form(Formation::Line));
    a.issue_order(id, Order::Face(Angle::HALF));
    assert_ne!(a.hash(), before, "the transition itself is hashed");
}

#[test]
fn the_three_reserved_blocks_are_zero_u32s_at_the_end() {
    // An empty battle hashes as: world, regiment count 0, slot count 0, three zero blocks.
    let b = empty_battle(5);
    let mut h = sim_state::StateHasher::new();
    h.write_world(&b.world);
    for _ in 0..5 {
        h.write_u32(0);
    }
    assert_eq!(b.hash(), h.finish());
}
