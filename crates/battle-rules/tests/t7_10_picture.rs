//! T7-10: in-range regiments appear and others do not; woods and smoke lower the quality;
//! confidence follows the formula and contacts drop below `drop_pm`; a contact needs one report
//! hop per level to reach the army.

mod t73_support;

use battle_rules::command::{CommandTable, OfficerId};
use battle_rules::{BattleState, TerrainGrid};
use core_math::Vec2Fx;
use t73_support::*;

/// Runs `step_command` once per tick through tick `last`.
fn run(b: &mut BattleState, t: &CommandTable, last: u64) {
    while b.world.tick <= last {
        b.step_command(t);
        b.world.advance_tick();
    }
}

fn contacts(b: &BattleState, officer: u16) -> usize {
    let cmd = b.cmd.as_ref().unwrap();
    cmd.pics[cmd.index_of(OfficerId(officer)).unwrap()]
        .contacts
        .len()
}

/// Regiment 2 stands 1000 m east of regiment 0; the first sighting has quality 445.
fn sighted(terrain: Option<u8>, smoke: u32) -> u32 {
    let t = calm_table();
    let mut b = field(1, 20, metres(1010, 20));
    if let Some(class) = terrain {
        let rows: Vec<String> = (0..64)
            .map(|y| if y == 0 { "w" } else { "." }.repeat(64))
            .collect();
        b.terrain = TerrainGrid::from_rows(
            100_000,
            0,
            0,
            &[('.', 0), ('w', class)],
            &rows,
            &b.tables.terrain,
        )
        .unwrap();
    }
    if smoke > 0 {
        b.smoke = battle_rules::SmokeGrid::new(100_000, 64, 64, Vec2Fx::ZERO).unwrap();
        b.smoke.add(metres(1010, 20), smoke);
    }
    b.step_command(&t);
    let cmd = b.cmd.as_ref().unwrap();
    let pic = &cmd.pics[cmd.index_of(OfficerId(5)).unwrap()];
    assert_eq!(pic.contacts.len(), 1);
    pic.contacts[0].quality_pm
}

#[test]
fn in_range_appears_and_out_of_range_does_not() {
    let t = calm_table();
    let mut near = field(1, 20, metres(1010, 20));
    let mut far = field(1, 20, metres(2510, 20));
    run(&mut near, &t, 1000);
    run(&mut far, &t, 1000);
    assert_eq!(contacts(&near, 5), 1, "repeated sightings join one contact");
    assert_eq!(contacts(&far, 5), 0);
    // A higher officer sees nothing at first hand.
    assert_eq!(contacts(&near, 4), 0);
}

#[test]
fn woods_and_smoke_lower_the_quality() {
    // q = (1000 - 1000 * 1000 / 1800) = 445 in the open.
    assert_eq!(sighted(None, 0), 445);
    // Woods have cover 300: 445 * 700 / 1000.
    assert_eq!(sighted(Some(3), 0), 311);
    // Smoke 100 at 5 per unit blocks 500 permille: 445 * 500 / 1000.
    assert_eq!(sighted(None, 100), 222);
}

#[test]
fn confidence_decays_and_contacts_drop() {
    let t = calm_table();
    let g = t.globals.clone();
    let mut b = field(1, 20, metres(1010, 20));
    b.step_command(&t);
    let q = 445u64;
    let view = b.picture_view(&t, OfficerId(5)).unwrap();
    assert_eq!(view.contacts.len(), 1);
    assert_eq!(view.contacts[0].confidence_pm, 445);
    assert!(view.contacts[0].strength >= 1);
    // Half the stale time later the confidence has halved and the radius has grown by the drift.
    let radius0 = view.contacts[0].radius_mm;
    let half = u64::from(g.stale_ticks) / 2;
    b.world.tick = half;
    let view = b.picture_view(&t, OfficerId(5)).unwrap();
    assert_eq!(u64::from(view.contacts[0].confidence_pm), q / 2);
    assert_eq!(view.contacts[0].age_ticks, half);
    assert_eq!(
        view.contacts[0].radius_mm,
        radius0 + half as i64 * g.drift_mm_per_tick
    );
    // The regiment leaves; once the confidence is below `drop_pm` the contact is gone.
    b.regiments[2].anchor = metres(6000, 6000);
    let late = u64::from(g.stale_ticks) * 22 / 24 + 100;
    b.world.tick = late - late % u64::from(g.observe_interval_ticks);
    b.step_command(&t);
    assert_eq!(contacts(&b, 5), 0);
    assert!(b
        .picture_view(&t, OfficerId(5))
        .unwrap()
        .contacts
        .is_empty());
}

#[test]
fn a_contact_needs_one_report_hop_per_level() {
    let t = calm_table();
    let mut b = field(1, 20, metres(1010, 20));
    let chain = [5u16, 4, 3, 2, 1];
    let mut first = [None; 5];
    while b.world.tick <= 20_000 {
        b.step_command(&t);
        for (k, &o) in chain.iter().enumerate() {
            if first[k].is_none() && contacts(&b, o) > 0 {
                first[k] = Some(b.world.tick);
            }
        }
        b.world.advance_tick();
    }
    let first: Vec<u64> = first.iter().map(|f| f.expect("reached")).collect();
    assert_eq!(first[0], 0);
    assert!(
        first[1] >= u64::from(t.globals.report_interval_ticks) - 6,
        "{first:?}"
    );
    assert!(first.windows(2).all(|w| w[0] < w[1]), "{first:?}");
    // The army also learns where its regiments stand, again by report.
    let army = b.picture_view(&t, OfficerId(1)).unwrap();
    let regs: Vec<u16> = army.friends.iter().map(|f| f.regiment.0).collect();
    assert_eq!(regs, vec![0, 1]);
    // The contact carries no regiment identity: its id is the picture's own counter.
    assert_eq!(army.contacts.len(), 1);
}
