//! Shared set-up for the SPEC-007 state tests: three regiments and the officers fixture.
#![allow(dead_code)]

#[path = "../t018_support/mod.rs"]
mod t018_support;

pub use t018_support::{empty_battle, spec};

use battle_rules::command::{load_officers, Link, Officer, OfficerSet, TraitMask};
use battle_rules::{BattleState, Formation, Training};
use core_math::{Fx, Vec2Fx};

pub const OFFICERS: &str = include_str!("../../../../qc/fixtures/command/officers_good.toml");
pub const TABLE: &str = include_str!("../../../../data/rules/command.toml");

pub fn officer_set() -> OfficerSet {
    load_officers(OFFICERS).unwrap()
}

pub fn officers_and_links(set: &OfficerSet) -> (Vec<Officer>, Vec<Link>) {
    let officers = set
        .officers
        .iter()
        .map(|s| Officer::from_spec(s, TraitMask(0)))
        .collect();
    (officers, set.links.iter().map(Link::from_spec).collect())
}

/// Regiments 0 and 1 of side 0 at (10, 20) and (30, 40) metres, regiment 2 of side 1 at
/// (50, 60); the command layer is not enabled.
pub fn plain_battle() -> BattleState {
    let mut b = empty_battle(7);
    for (team, x, y) in [(0u8, 10, 20), (0, 30, 40), (1, 50, 60)] {
        let mut s = spec(20, Training::Trained, Formation::Line);
        s.team = team;
        s.anchor = Vec2Fx::new(Fx::from_int(x), Fx::from_int(y));
        b.spawn_regiment(s);
    }
    b
}

/// [`plain_battle`] with the command layer enabled from the officers fixture.
pub fn command_battle() -> BattleState {
    let mut b = plain_battle();
    let (officers, links) = officers_and_links(&officer_set());
    b.enable_command(officers, links).unwrap();
    b
}
