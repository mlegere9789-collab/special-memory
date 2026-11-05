//! T12-14, log half (SPEC-005 amendment A10): the sound log of one tick holds a `Shot` per round
//! that left the barrel and a `Burst` per round that burst; it is empty after a silent tick and
//! is not part of the state hash.

mod t022_support;
mod t023_support;

use battle_rules::{FireRecord, FireRecordKind, Training};
use core_math::{Fx, SplitMix64};
use t022_support::duel;

fn count(log: &[FireRecord], kind: FireRecordKind) -> u64 {
    log.iter().filter(|r| r.kind == kind).count() as u64
}

#[test]
fn shot_records_equal_shots_fired_minus_misfires_on_every_tick() {
    let (mut b, shooters, _) = duel(7, "smoothbore_69", 0, Training::Trained, 60, 60);
    b.weather.wet_pm = 3000; // plenty of misfires
    let (mut shots, mut misfires, mut logged) = (0u64, 0u64, 0u64);
    for _ in 0..1500 {
        let before = b.counters();
        let seq0 = b.seq;
        b.step();
        let after = b.counters();
        let log = b.sound_log();
        let fired = (after.shots_fired - before.shots_fired) - (after.misfires - before.misfires);
        assert_eq!(count(log, FireRecordKind::Shot), fired);
        // Ascending, consecutive seq from the tick's first new projectile.
        let seqs: Vec<u64> = log
            .iter()
            .filter(|r| r.kind == FireRecordKind::Shot)
            .map(|r| r.seq)
            .collect();
        assert_eq!(seqs, (seq0..seq0 + fired).collect::<Vec<_>>());
        assert!(log
            .iter()
            .all(|r| r.unit == u32::from(shooters.0) && r.team == 0 && r.z_mm > 0));
        shots += after.shots_fired - before.shots_fired;
        misfires += after.misfires - before.misfires;
        logged += fired;
    }
    assert!(misfires > 0 && shots > misfires && logged == shots - misfires);
}

#[test]
fn the_log_is_empty_after_a_tick_with_no_fire() {
    let (mut b, _, _) = duel(7, "smoothbore_69", 0, Training::Trained, 20, 60);
    let mut seen_fire = false;
    for _ in 0..1500 {
        b.step();
        seen_fire |= !b.sound_log().is_empty();
    }
    assert!(seen_fire);
    // Hold fire, and wait for the projectiles in the air to land.
    b.set_fire_mode(
        battle_rules::RegimentId(0),
        battle_rules::FireMode::HoldFire,
    );
    for _ in 0..400 {
        b.step();
    }
    assert!(b.pool.is_empty());
    assert!(b.sound_log().is_empty());
}

#[test]
fn the_hash_ignores_the_log() {
    let (mut b, _, _) = duel(3, "smoothbore_69", 0, Training::Trained, 30, 60);
    let mut cleared = None;
    for _ in 0..1500 {
        b.step();
        if !b.sound_log().is_empty() {
            cleared = Some(b.clone());
            break;
        }
    }
    let mut c = cleared.expect("a tick with fire");
    let with_log = c.hash();
    assert_eq!(with_log, b.hash());
    c.sound_log.clear();
    assert!(c.sound_log().is_empty());
    assert_eq!(c.hash(), with_log);
    c.sound_log.push(FireRecord {
        kind: FireRecordKind::Burst,
        seq: 9,
        unit: 1,
        team: 1,
        ammo: 2,
        pos: Default::default(),
        z_mm: 5,
    });
    assert_eq!(c.hash(), with_log);
}

fn live_seed() -> u64 {
    (1u64..)
        .find(|&s| SplitMix64::new(s).next_u64() % 1_000_000 >= 30_000)
        .unwrap()
}

/// A shell about to burst by ground contact and a case at the end of its fuse, plus a canister
/// at the muzzle; the parents have seq 4, 2 and 3 in pool order.
fn bursting(reverse: bool) -> battle_rules::BattleState {
    use t023_support::{bullet, field_of, m, mm};
    let mut b = field_of(1, '.');
    let mut shell = bullet(&b, "shell_12", 5, 3, live_seed());
    (shell.seq, shell.pos, shell.z) = (4, m(100, 200), mm(500));
    (shell.vel, shell.vz, shell.fuse) = (m(60, 0), Fx::from_int(-30), 50);
    let mut case = bullet(&b, "case_12", 6, 8, live_seed());
    (case.seq, case.pos, case.z) = (2, m(100, 220), mm(3000));
    (case.vel, case.vz, case.fuse) = (m(100, 0), Fx::ZERO, 3);
    case.team = 1;
    let mut canister = bullet(&b, "canister_12", 7, 9, 42);
    (canister.seq, canister.pos, canister.z) = (3, m(100, 240), mm(1000));
    (canister.vel, canister.fuse) = (m(400, 0), 0);
    b.pool.extend([shell, case, canister]);
    b.seq = 5;
    b.step_flight_with(reverse);
    b
}

#[test]
fn a_burst_appends_one_record_in_ascending_parent_seq_whatever_the_flight_order() {
    let (fwd, rev) = (bursting(false), bursting(true));
    assert_eq!(fwd.sound_log(), rev.sound_log());
    let log = fwd.sound_log();
    assert_eq!(log.len(), 3);
    assert!(log.iter().all(|r| r.kind == FireRecordKind::Burst));
    assert_eq!(log.iter().map(|r| r.seq).collect::<Vec<_>>(), [2, 3, 4]);
    assert_eq!(log.iter().map(|r| r.unit).collect::<Vec<_>>(), [8, 9, 3]);
    assert_eq!(log.iter().map(|r| r.team).collect::<Vec<_>>(), [1, 0, 0]);
    let ammo = |n: &str| fwd.tables.ammo.index_of(n).unwrap() as u8;
    assert_eq!(
        log.iter().map(|r| r.ammo).collect::<Vec<_>>(),
        [ammo("case_12"), ammo("canister_12"), ammo("shell_12")]
    );
    // The canister burst at the muzzle, 1 m up.
    assert_eq!(log[1].pos, t023_support::m(100, 240));
    assert_eq!(log[1].z_mm, 1000);
    // Burst children do not make records of their own.
    assert!(fwd.pool.len() > 3);
}

fn all_duds() -> battle_rules::Tables {
    let mut t = t023_support::tables();
    let text = std::fs::read_to_string(format!("{}/ammo.toml", t023_support::RULES_DIR)).unwrap();
    let text = text.replace("dud_ppm = 0", "dud_ppm = 1000000");
    let text = text.replace("dud_ppm = 30000", "dud_ppm = 1000000");
    t.ammo = battle_rules::AmmoTable::from_toml_str(&text).unwrap();
    t
}

#[test]
fn a_dud_and_a_round_still_in_the_air_append_nothing() {
    use t023_support::{bullet, field_with, m, mm};
    let mut b = field_with(1, '.', all_duds());
    let mut canister = bullet(&b, "canister_12", 7, 9, 42);
    (canister.seq, canister.pos, canister.z) = (0, m(100, 240), mm(1000));
    (canister.vel, canister.fuse) = (m(400, 0), 0);
    let mut case = bullet(&b, "case_12", 6, 8, 42);
    (case.seq, case.pos, case.z) = (1, m(100, 220), mm(3000));
    (case.vel, case.vz, case.fuse) = (m(100, 0), Fx::ZERO, 3);
    let mut shell = bullet(&b, "shell_12", 5, 3, live_seed());
    (shell.seq, shell.pos, shell.z) = (2, m(100, 200), mm(5000));
    (shell.vel, shell.fuse) = (m(60, 0), 500);
    b.pool.extend([canister, case, shell]);
    b.seq = 3;
    b.step_flight();
    assert_eq!(b.counters().projectiles_spawned, 0);
    assert!(b.sound_log().is_empty());
}
