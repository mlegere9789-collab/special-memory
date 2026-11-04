//! T5-16: pierce through a file and bursts (SPEC-005 sections 2 and 5).

mod t023_support;

use battle_rules::{AmmoTable, Flight};
use core_math::{Fx, SplitMix64};
use t023_support::{bullet, field_of, field_with, fire_through, m, man, mm, tables};

/// A file of `n` men one behind the other, 560 mm apart, facing the shooter. Returns the
/// battle and the position of the first.
fn file(n: i32) -> (battle_rules::BattleState, core_math::Vec2Fx) {
    let mut b = field_of(1, '.');
    let mut first = None;
    for i in 0..n {
        let a = man(&mut b, 1, m(100, 200), 0);
        let p = b.world.agents.pos(a).unwrap();
        b.world
            .agents
            .set_pos(a, core_math::Vec2Fx::new(p.x + mm(560 * i64::from(i)), p.y));
        first.get_or_insert(p);
    }
    b.grid.rebuild(&b.world.agents);
    let first = first.unwrap();
    (b, first)
}

#[test]
fn a_rifle_ball_at_20_m_through_a_three_man_file_hits_at_most_two() {
    for ammo in ["minie_58", "ball_69"] {
        let (b, at) = file(3);
        let mut p = bullet(&b, ammo, 999, 77, 11);
        let mut out = Vec::new();
        let r = fire_through(&b, &mut p, at, 1200, 20_000, 270, &mut out);
        assert!(r.struck <= 2, "{ammo} struck {}", r.struck);
        assert!(r.struck >= 1);
        assert!(r.stopped);
    }
}

#[test]
fn a_heavy_shot_goes_through_a_file_and_is_capped_at_eight_bodies() {
    let (b, at) = file(3);
    let mut p = bullet(&b, "shot_12", 999, 77, 11);
    p.vel = m(452, 0);
    let mut out = Vec::new();
    let r = fire_through(&b, &mut p, at, 1200, 20_000, 452, &mut out);
    assert!(r.struck >= 3, "struck {}", r.struck);
    assert!(!r.stopped);

    let (b, at) = file(10);
    let mut p = bullet(&b, "shot_12", 999, 77, 11);
    let r = fire_through(&b, &mut p, at, 1200, 20_000, 452, &mut out);
    assert_eq!(r.struck, 8);
    assert_eq!(p.hits, 8);
    assert!(r.stopped);
}

fn set_dud(ppm: u32) -> battle_rules::Tables {
    let mut t = tables();
    let text = std::fs::read_to_string(format!("{}/ammo.toml", t023_support::RULES_DIR)).unwrap();
    let i = text.rfind("dud_ppm = 0").unwrap();
    let text = format!("{}dud_ppm = {ppm}{}", &text[..i], &text[i + 11..]);
    t.ammo = AmmoTable::from_toml_str(&text).unwrap();
    t
}

fn canister_at_muzzle(b: &mut battle_rules::BattleState, seed: u64) {
    let mut p = bullet(b, "canister_12", 5, 3, seed);
    p.seq = 0;
    p.fuse = 0;
    p.vel = m(400, 0);
    p.pos = m(100, 200);
    p.z = mm(1000);
    b.pool.push(p);
    b.seq = 1;
}

#[test]
fn canister_makes_exactly_27_repeatable_children() {
    let run = || {
        let mut b = field_of(1, '.');
        canister_at_muzzle(&mut b, 42);
        b.step_flight();
        b
    };
    let (a, c) = (run(), run());
    assert_eq!(a.pool.len(), 27);
    assert_eq!(a.counters().projectiles_spawned, 27);
    assert_eq!(a.pool, c.pool);
    assert_eq!(a.seq, 28);
    let seqs: Vec<u64> = a.pool.iter().map(|p| p.seq).collect();
    assert_eq!(seqs, (1..=27).collect::<Vec<_>>());
    let mut seeds: Vec<u64> = a.pool.iter().map(|p| p.seed).collect();
    seeds.sort_unstable();
    seeds.dedup();
    assert_eq!(seeds.len(), 27);
    assert!(a
        .pool
        .iter()
        .all(|p| p.fuse == -3 && p.pos == m(100, 200) && p.owner == 5));
    let mut dirs: Vec<_> = a.pool.iter().map(|p| (p.vel, p.vz)).collect();
    dirs.sort_by_key(|d| (d.0.x.raw(), d.0.y.raw(), d.1.raw()));
    dirs.dedup();
    assert!(dirs.len() > 20, "directions differ");
    // Cone: every pellet leaves at 850 permille of the parent's 400 m/s, forward.
    for p in &a.pool {
        let speed = (p.velocity().length() - Fx::from_int(340)).raw().abs();
        assert!(speed < Fx::from_int(1).raw(), "speed");
        assert!(p.vel.x > Fx::ZERO);
    }
}

#[test]
fn a_dud_makes_no_children_and_flies_on_inert() {
    let mut b = field_with(1, '.', set_dud(1_000_000));
    canister_at_muzzle(&mut b, 42);
    b.step_flight();
    assert_eq!(b.counters().projectiles_spawned, 0);
    assert_eq!(b.pool.len(), 1);
    assert_eq!(b.pool[0].fuse, -2);
}

/// A seed whose first burst draw is not a 3 percent dud.
fn live_seed() -> u64 {
    (1u64..)
        .find(|&s| SplitMix64::new(s).next_u64() % 1_000_000 >= 30_000)
        .unwrap()
}

#[test]
fn a_shell_bursts_on_the_ground_and_a_case_when_its_fuse_runs_out() {
    let mut b = field_of(1, '.');
    let mut shell = bullet(&b, "shell_12", 5, 3, live_seed());
    (shell.pos, shell.z, shell.vel, shell.vz, shell.fuse) =
        (m(100, 200), mm(500), m(60, 0), Fx::from_int(-30), 50);
    b.pool.push(shell);
    let mut case = bullet(&b, "case_12", 5, 3, live_seed());
    case.seq = 1;
    (case.pos, case.z, case.vel, case.vz, case.fuse) =
        (m(100, 220), mm(3000), m(100, 0), Fx::ZERO, 3);
    b.pool.push(case);
    b.seq = 2;
    b.step_flight();
    assert_eq!(b.pool.len(), 40 + 76);
    assert_eq!(b.counters().projectiles_spawned, 116);
    // Shell fragments come first: parents are resolved in order and children follow by parent.
    assert!(b.pool[..40].iter().all(|p| p.seq >= 2 && p.fuse == -3));
    assert_eq!(b.pool.last().unwrap().seq, 117);
    let _ = Flight::Flying;
}
