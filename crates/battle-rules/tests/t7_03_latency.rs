//! T7-03: `estimate` against `qc/fixtures/ref_command.py` (exact `Fraction` reference, output
//! committed in `ref_command.out`), and arrival inside `[eta_lo, eta_hi]` for 1,000 seeds.

mod t73_support;

use battle_rules::command::{Medium, OfficerId};
use battle_rules::{BattleState, Formation, RegimentSpec, Tables, TerrainGrid, Training};
use core_math::{Angle, Fx, RngStreams, Vec2Fx};
use sim_state::{SpatialGrid, WorldState};
use t73_support::*;

const REF: &str = include_str!("../../../qc/fixtures/ref_command.out");

/// The reference terrain: rows from the south, legend characters mapped to class ids.
fn ref_battle() -> BattleState {
    let (mut cell, mut rows, mut legend) = (0i64, Vec::new(), Vec::new());
    for line in REF.lines() {
        let f: Vec<&str> = line.split(',').collect();
        match f[0] {
            "cell" => cell = f[1].parse().unwrap(),
            "terrain" => rows.push(f[1].to_owned()),
            "legend" => legend.push((f[1].chars().next().unwrap(), f[2].parse().unwrap())),
            _ => {}
        }
    }
    let tables = Tables::load_dir(RULES_DIR).unwrap();
    let terrain = TerrainGrid::from_rows(cell, 0, 0, &legend, &rows, &tables.terrain).unwrap();
    let grid = SpatialGrid::new(Fx::from_int(100), 64, 64, Vec2Fx::ZERO).unwrap();
    let mut b = BattleState::new(WorldState::new(RngStreams::new(1)), terrain, grid, tables);
    for (team, at) in [
        (0u8, metres(10, 20)),
        (0, metres(30, 40)),
        (1, metres(50, 60)),
    ] {
        b.spawn_regiment(RegimentSpec {
            team,
            count: 5,
            training: Training::Trained,
            formation: Formation::Line,
            anchor: at,
            heading: Angle::ZERO,
        });
    }
    let field = field(1, 1, metres(0, 0));
    let cmd = field.cmd.clone().unwrap();
    b.cmd = Some(cmd);
    b
}

struct Case {
    medium: Medium,
    t0: u64,
    from: (i64, i64),
    to: (i64, i64),
    words: u32,
    lo: u64,
    hi: u64,
}

fn cases() -> Vec<Case> {
    REF.lines()
        .filter(|l| l.starts_with("case,"))
        .map(|l| {
            let f: Vec<&str> = l.split(',').collect();
            let n = |i: usize| f[i].parse::<i64>().unwrap();
            Case {
                medium: match f[1] {
                    "courier" => Medium::Courier,
                    "flags" => Medium::Flags,
                    _ => Medium::Telegraph,
                },
                t0: n(2) as u64,
                from: (n(3), n(4)),
                to: (n(5), n(6)),
                words: n(7) as u32,
                lo: n(8) as u64,
                hi: n(9) as u64,
            }
        })
        .collect()
}

/// Officer pair that a medium uses in the fixture: courier 1 to 2, flags 6 to 5, telegraph 10 to 11.
fn pair(m: Medium) -> (u16, u16) {
    match m {
        Medium::Courier => (1, 2),
        Medium::Flags => (6, 5),
        Medium::Telegraph => (10, 11),
    }
}

#[test]
fn estimate_matches_reference() {
    let t = table();
    let all = cases();
    assert_eq!(all.len(), 30);
    for c in &all {
        let mut b = ref_battle();
        let (f, to) = pair(c.medium);
        place(&mut b, f, c.from.0, c.from.1);
        place(&mut b, to, c.to.0, c.to.1);
        b.world.tick = c.t0;
        let e = b
            .estimate(&t, OfficerId(f), OfficerId(to), Some(c.medium), c.words)
            .unwrap();
        assert_eq!(e.medium, c.medium, "{:?}", c.from);
        assert_eq!(
            (e.eta_lo, e.eta_hi),
            (c.lo, c.hi),
            "{} {:?}",
            medium_name(c.medium),
            c.from
        );
    }
}

#[test]
fn arrival_lies_inside_the_estimate() {
    let t = calm_table();
    for (m, from, to, words_body) in [
        (
            Medium::Courier,
            (1u16, (0i64, 0i64)),
            (2u16, (3000i64, 0i64)),
            hold(),
        ),
        (Medium::Flags, (6, (0, 0)), (5, (3000, 0)), walk_to(10, 10)),
        (Medium::Telegraph, (10, (0, 0)), (11, (3000, 0)), face(30)),
    ] {
        let mut base = field(1, 1, metres(5000, 5000));
        place(&mut base, from.0, from.1 .0, from.1 .1);
        place(&mut base, to.0, to.1 .0, to.1 .1);
        for seed in 1..=1000u64 {
            let mut b = base.clone();
            b.world.rng = RngStreams::new(seed);
            let id = b
                .send_message(&t, OfficerId(from.0), OfficerId(to.0), Some(m), words_body)
                .unwrap();
            let rec = b
                .cmd
                .as_ref()
                .unwrap()
                .records
                .iter()
                .find(|r| r.id == id)
                .unwrap();
            let (lo, hi) = (rec.eta_lo, rec.eta_hi);
            let got = run_until(&mut b, &t, hi + 5);
            assert_eq!(got.len(), 1, "{m:?} seed {seed}");
            let tick = got[0].0;
            assert!(
                (lo..=hi).contains(&tick),
                "{m:?} seed {seed}: {tick} not in {lo}..={hi}"
            );
        }
    }
}
