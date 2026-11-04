//! T5-06: elevation increases with range; each 25 m entry crosses the aim height within 50 mm;
//! `minie_58` at 183 m and 274 m lies in (0.1, 2.5) degrees, at 805 m in (2, 6)
//! (SPEC-005 sections 3 and 11).

use battle_rules::flight::advance_projectile;
use battle_rules::range::AIM_HEIGHT_MM;
use battle_rules::{
    fx_ratio, AmmoRow, AmmoTable, Ballistics, Flight, FlightEnv, Projectile, RangeTable,
    WeaponTable,
};
use core_math::{Angle, Fx, Vec2Fx};

const AMMO: &str = include_str!("../../../data/rules/ammo.toml");
const WEAPONS: &str = include_str!("../../../data/rules/weapons.toml");

fn degrees(a: Angle) -> f64 {
    f64::from(a.raw()) * 360.0 / 4_294_967_296.0
}

fn tables() -> (AmmoTable, WeaponTable) {
    (
        AmmoTable::from_toml_str(AMMO).unwrap(),
        WeaponTable::from_toml_str(WEAPONS).unwrap(),
    )
}

/// The largest `reach_mm` of a weapon that fires this ammunition.
fn reach_mm(ammo: &AmmoTable, weapons: &WeaponTable, id: &str) -> u32 {
    assert!(ammo.get(id).is_some(), "unknown ammunition {id}");
    weapons
        .rows()
        .iter()
        .filter(|w| w.ammo.iter().any(|a| a.id == id))
        .map(|w| w.reach_mm)
        .max()
        .unwrap_or_else(|| panic!("no weapon fires {id}"))
}

/// The largest `extended_mm` of a weapon that fires this ammunition.
fn extended_mm(ammo: &AmmoTable, weapons: &WeaponTable, id: &str) -> u32 {
    assert!(ammo.get(id).is_some(), "unknown ammunition {id}");
    weapons
        .rows()
        .iter()
        .filter(|w| w.ammo.iter().any(|a| a.id == id))
        .map(|w| w.extended_mm)
        .max()
        .unwrap_or_else(|| panic!("no weapon fires {id}"))
}

/// Height in mm of a projectile launched from the aim height at `elev`, when it first passes
/// horizontal range `range_m`, found by flying it tick by tick with the public flight step.
fn height_at(row: &AmmoRow, gravity: Fx, elev: Angle, range_m: i32) -> Option<f64> {
    let env = FlightEnv {
        gravity,
        wind: Vec2Fx::ZERO,
        ground_pm: 1000,
    };
    let body = Ballistics {
        restitution_pm: None,
        ..Ballistics::of_ammo(row)
    };
    let speed = fx_ratio(i64::from(row.muzzle_mm_s), 1000);
    let mut p = Projectile {
        seq: 0,
        owner: 0,
        unit: 0,
        team: 0,
        ammo: 0,
        pos: Vec2Fx::ZERO,
        z: fx_ratio(i64::from(AIM_HEIGHT_MM), 1000),
        vel: Vec2Fx::new(elev.cos() * speed, Fx::ZERO),
        vz: elev.sin() * speed,
        travelled_mm: 0,
        bounces: 0,
        hits: 0,
        fuse: -1,
        seed: 0,
    };
    let range = Fx::from_int(range_m);
    // One sub-step at a time so the crossing can be interpolated.
    let (mut prev_x, mut prev_z) = (p.pos.x, p.z);
    for _ in 0..20_000 {
        let (f, _) = advance_projectile(&mut p, &body, u32::MAX, &env, 1);
        if f == Flight::Ground {
            return None;
        }
        if p.pos.x >= range {
            let t = (range - prev_x) / (p.pos.x - prev_x);
            let z = prev_z + (p.z - prev_z) * t;
            return Some(z.raw() as f64 / 4_294_967_296.0 * 1000.0);
        }
        prev_x = p.pos.x;
        prev_z = p.z;
    }
    None
}

#[test]
fn elevation_increases_and_each_entry_crosses_the_aim_height() {
    let (ammo, weapons) = tables();
    let gravity = ammo.constants().gravity;
    for row in ammo.rows() {
        let reach = reach_mm(&ammo, &weapons, &row.id);
        let rt = RangeTable::build(row, gravity, reach);
        assert_eq!(rt.len(), (reach / 25_000) as usize + 1, "{}", row.id);
        let solved = (rt.solved_range().to_int_floor() / 25) as usize;
        let extended = extended_mm(&ammo, &weapons, &row.id);
        assert!(
            solved as u32 * 25_000 >= extended,
            "{}: solved only to {} m, extended range {} m",
            row.id,
            solved * 25,
            extended / 1000
        );
        let mut worst = 0.0f64;
        for i in 1..=solved {
            let (e0, e1) = (rt.entry(i - 1).unwrap().0, rt.entry(i).unwrap().0);
            assert!(e1 > e0, "{} entry {i} not increasing", row.id);
            let z = height_at(row, gravity, e1, 25 * i as i32)
                .unwrap_or_else(|| panic!("{} entry {i} never reaches its range", row.id));
            let err = (z - f64::from(AIM_HEIGHT_MM)).abs();
            worst = worst.max(err);
            assert!(err < 50.0, "{} entry {i}: height {z} mm", row.id);
        }
        println!(
            "{}: {} entries, solved to {} m, worst height error {worst:.2} mm",
            row.id,
            rt.len(),
            solved * 25
        );
    }
}

#[test]
fn rifle_bullet_elevations_are_plausible() {
    let (ammo, weapons) = tables();
    let row = ammo.get("minie_58").unwrap();
    let rt = RangeTable::build(
        row,
        ammo.constants().gravity,
        reach_mm(&ammo, &weapons, "minie_58"),
    );
    for (r, lo, hi) in [(183, 0.1, 2.5), (274, 0.1, 2.5), (805, 2.0, 6.0)] {
        let d = degrees(rt.elevation(Fx::from_int(r)));
        assert!(d > lo && d < hi, "{r} m: {d} degrees not in ({lo}, {hi})");
    }
}

#[test]
fn interpolation_is_linear_and_clamped() {
    let (ammo, weapons) = tables();
    let row = ammo.get("minie_58").unwrap();
    let rt = RangeTable::build(
        row,
        ammo.constants().gravity,
        reach_mm(&ammo, &weapons, "minie_58"),
    );
    // Exactly on an entry.
    assert_eq!(rt.elevation(Fx::from_int(100)), rt.entry(4).unwrap().0);
    assert_eq!(rt.time_of_flight(Fx::from_int(100)), rt.entry(4).unwrap().1);
    // Halfway between two entries.
    let (a, b) = (
        i64::from(rt.entry(8).unwrap().0.raw()),
        i64::from(rt.entry(9).unwrap().0.raw()),
    );
    let mid = i64::from(
        rt.elevation(Fx::from_raw((212i64 << 32) + (1 << 31)).max(Fx::ZERO))
            .raw(),
    );
    let want = (a + b) / 2;
    assert!((mid - want).abs() <= 1, "{mid} vs {want}");
    // Negative range clamps to the first entry, far range to the last.
    assert_eq!(rt.elevation(Fx::from_int(-5)), rt.entry(0).unwrap().0);
    let last = rt.entry(rt.len() - 1).unwrap();
    assert_eq!(rt.elevation(Fx::from_int(1_000_000)), last.0);
    assert_eq!(rt.time_of_flight(Fx::from_int(1_000_000)), last.1);
    // Time of flight grows with range over the solved span.
    let mut prev = 0;
    for r in (25..=800).step_by(25) {
        let t = rt.time_of_flight(Fx::from_int(r));
        assert!(t > prev, "{r}: {t} after {prev}");
        prev = t;
    }
}

#[test]
fn build_is_deterministic() {
    let (ammo, _) = tables();
    let row = ammo.get("ball_69").unwrap();
    let g = ammo.constants().gravity;
    assert_eq!(
        RangeTable::build(row, g, 400_000),
        RangeTable::build(row, g, 400_000)
    );
}
