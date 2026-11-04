//! T5-05: range, time of flight and impact speed for every ammunition at three elevations,
//! with and without wind, match the independent f64 RK4 reference
//! `qc/fixtures/ref_flight.py` (committed output `qc/fixtures/ref_flight.csv`) within 0.5 percent
//! (SPEC-005 sections 2 and 11).

use battle_rules::flight::{advance_projectile, DT, SUBSTEPS_PER_TICK};
use battle_rules::{fx_ratio, AmmoTable, Ballistics, Flight, FlightEnv, Projectile};
use core_math::{Angle, Fx, Vec2Fx};

const AMMO: &str = include_str!("../../../data/rules/ammo.toml");
const REF: &str = include_str!("../../../qc/fixtures/ref_flight.csv");
const LAUNCH_HEIGHT_MM: i64 = 1200;

struct RefRow<'a> {
    ammo: &'a str,
    elev_deg: i32,
    wind: &'a str,
    range_mm: f64,
    tof_us: f64,
    impact_mm_s: f64,
}

fn rows() -> Vec<RefRow<'static>> {
    let mut out = Vec::new();
    for line in REF.lines() {
        if line.starts_with('#') || line.starts_with("ammo,") || line.trim().is_empty() {
            continue;
        }
        let f: Vec<&str> = line.split(',').collect();
        assert_eq!(f.len(), 7, "bad row {line}");
        out.push(RefRow {
            ammo: f[0],
            elev_deg: f[1].parse().unwrap(),
            wind: f[2],
            range_mm: f[3].parse().unwrap(),
            tof_us: f[5].parse().unwrap(),
            impact_mm_s: f[6].parse().unwrap(),
        });
    }
    out
}

fn to_f64(v: Fx) -> f64 {
    v.raw() as f64 / 4_294_967_296.0
}

struct Result {
    range_mm: f64,
    tof_us: f64,
    impact_mm_s: f64,
}

fn fly(table: &AmmoTable, r: &RefRow<'_>) -> Result {
    let row = table.get(r.ammo).expect("ammo row");
    let wind = match r.wind {
        "none" => Vec2Fx::ZERO,
        "cross" => Vec2Fx::new(Fx::ZERO, Fx::from_int(5)),
        "head" => Vec2Fx::new(Fx::from_int(-3), Fx::ZERO),
        other => panic!("wind {other}"),
    };
    let env = FlightEnv {
        gravity: table.constants().gravity,
        wind,
        ground_pm: 1000,
    };
    // Solid shot lands once: the reference ends at first contact.
    let body = Ballistics {
        restitution_pm: None,
        ..Ballistics::of_ammo(row)
    };
    let speed = fx_ratio(i64::from(row.muzzle_mm_s), 1000);
    let elev = Angle::from_degrees_int(r.elev_deg);
    let mut p = Projectile {
        seq: 0,
        owner: 0,
        unit: 0,
        team: 0,
        ammo: 0,
        pos: Vec2Fx::ZERO,
        z: fx_ratio(LAUNCH_HEIGHT_MM, 1000),
        vel: Vec2Fx::new(elev.cos() * speed, Fx::ZERO),
        vz: elev.sin() * speed,
        travelled_mm: 0,
        bounces: 0,
        hits: 0,
        fuse: -1,
        seed: 0,
    };
    let mut substeps = Fx::ZERO;
    for _ in 0..10_000 {
        let (f, n) = advance_projectile(&mut p, &body, u32::MAX, &env, SUBSTEPS_PER_TICK);
        substeps += n;
        match f {
            Flight::Flying => {}
            Flight::Ground => {
                let speed3 = p.velocity().length();
                return Result {
                    range_mm: to_f64(p.pos.length()) * 1000.0,
                    tof_us: to_f64(substeps * DT) * 1.0e6,
                    impact_mm_s: to_f64(speed3) * 1000.0,
                };
            }
            other => panic!("{} {}: ended {other:?}", r.ammo, r.elev_deg),
        }
    }
    panic!("never landed");
}

fn rel(a: f64, b: f64) -> f64 {
    ((a - b) / b).abs()
}

#[test]
fn every_ammo_elevation_and_wind_matches_the_rk4_reference() {
    let table = AmmoTable::from_toml_str(AMMO).unwrap();
    let reference = rows();
    assert_eq!(reference.len(), table.rows().len() * 3 * 3, "row count");
    let mut worst = 0.0f64;
    for r in &reference {
        let got = fly(&table, r);
        let (er, et, ev) = (
            rel(got.range_mm, r.range_mm),
            rel(got.tof_us, r.tof_us),
            rel(got.impact_mm_s, r.impact_mm_s),
        );
        worst = worst.max(er).max(et).max(ev);
        assert!(
            er < 0.005 && et < 0.005 && ev < 0.005,
            "{} {} deg {}: range {:.0} vs {} ({er:.5}), tof {:.0} vs {} ({et:.5}), speed {:.0} vs {} ({ev:.5})",
            r.ammo,
            r.elev_deg,
            r.wind,
            got.range_mm,
            r.range_mm,
            got.tof_us,
            r.tof_us,
            got.impact_mm_s,
            r.impact_mm_s
        );
    }
    println!("worst relative error against the reference: {worst:.6}");
}

#[test]
fn flight_is_repeatable() {
    let table = AmmoTable::from_toml_str(AMMO).unwrap();
    let r = &rows()[20];
    let (a, b) = (fly(&table, r), fly(&table, r));
    assert_eq!(a.range_mm.to_bits(), b.range_mm.to_bits());
    assert_eq!(a.tof_us.to_bits(), b.tof_us.to_bits());
}
