//! T12-14, adapter half (SPEC-012 section 4): `adapt_fire` maps every ammunition id of the shipped
//! table to the stated source row, or to nothing.

use acoustics_model::adapt_fire;
use battle_rules::{AmmoTable, FireRecord, FireRecordKind};
use core_math::{Fx, Vec2Fx};

const MUSKET: u8 = 0;
const CARBINE: u8 = 1;
const REVOLVER: u8 = 2;
const GUN_LAUNCH: u8 = 3;
const BURST_SHELL: u8 = 4;
const BURST_CASE: u8 = 5;

fn ammo() -> AmmoTable {
    AmmoTable::from_toml_str(include_str!("../../../data/rules/ammo.toml")).unwrap()
}

fn record(kind: FireRecordKind, ammo: usize, z_mm: i32) -> FireRecord {
    FireRecord {
        kind,
        seq: 77,
        unit: 12,
        team: 1,
        ammo: ammo as u8,
        pos: Vec2Fx {
            x: Fx::from_int(310),
            y: Fx::from_int(-45),
        },
        z_mm,
    }
}

fn source(t: &AmmoTable, kind: FireRecordKind, id: &str) -> Option<u8> {
    let i = t.index_of(id).expect("ammunition id exists");
    adapt_fire(&record(kind, i, 1500), t).map(|e| e.source)
}

#[test]
fn every_ammunition_id_maps_to_the_stated_row_or_none() {
    use FireRecordKind::{Burst, Shot};
    let t = ammo();
    let want: [(&str, Option<u8>, Option<u8>); 8] = [
        ("ball_69", Some(MUSKET), None),
        ("minie_58", Some(MUSKET), None),
        ("carbine_52", Some(CARBINE), None),
        ("revolver_44", Some(REVOLVER), None),
        ("shot_12", Some(GUN_LAUNCH), None),
        ("shell_12", Some(GUN_LAUNCH), Some(BURST_SHELL)),
        ("case_12", Some(GUN_LAUNCH), Some(BURST_CASE)),
        ("canister_12", Some(GUN_LAUNCH), None),
    ];
    assert_eq!(
        t.rows().len(),
        want.len(),
        "a new ammunition id needs a row here"
    );
    for (id, shot, burst) in want {
        assert_eq!(source(&t, Shot, id), shot, "{id} shot");
        assert_eq!(source(&t, Burst, id), burst, "{id} burst");
    }
}

#[test]
fn an_unknown_ammunition_number_makes_no_sound() {
    let t = ammo();
    let n = t.rows().len();
    assert_eq!(adapt_fire(&record(FireRecordKind::Shot, n, 1500), &t), None);
    assert_eq!(
        adapt_fire(&record(FireRecordKind::Burst, 255, 1500), &t),
        None
    );
}

#[test]
fn the_event_carries_the_record_fields() {
    let t = ammo();
    let i = t.index_of("shell_12").unwrap();
    let r = record(FireRecordKind::Burst, i, 2300);
    let e = adapt_fire(&r, &t).unwrap();
    assert_eq!(
        (e.seq, e.unit, e.team, e.pos, e.height_mm, e.count),
        (77, 12, 1, r.pos, 2300, 1)
    );
}

#[test]
fn the_height_is_clamped_to_the_u16_range() {
    let t = ammo();
    let i = t.index_of("ball_69").unwrap();
    let h = |z: i32| {
        adapt_fire(&record(FireRecordKind::Shot, i, z), &t)
            .unwrap()
            .height_mm
    };
    assert_eq!(
        (h(-500), h(0), h(65_535), h(70_000), h(i32::MAX)),
        (0, 0, 65_535, 65_535, 65_535)
    );
}
