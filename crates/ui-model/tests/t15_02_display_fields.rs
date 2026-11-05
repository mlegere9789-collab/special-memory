//! T15-02 no hidden values: the display-field list equals the fields the interface shows, and
//! every shown number is the simulation value or its printed quantisation.

mod common;

use battle_rules::{Formation, Order, RegimentId};
use common::{battle, steady, Canned, CannedMorale};
use std::collections::BTreeSet;
use ui_model::{
    capture, display_fields, FieldKind, FullInfo, InfoMode, MoraleReport, MoraleSource, Sighting,
};

const FIELDS: &str = include_str!("../../../data/ui/display-fields.toml");
/// Structs whose fields are display fields, with the file that defines them.
const SHOWN: [(&str, &str); 5] = [
    ("Snapshot", "snapshot.rs"),
    ("RegimentView", "snapshot.rs"),
    ("EnemyView", "snapshot.rs"),
    ("MoraleReport", "source.rs"),
    ("OrderRecord", "source.rs"),
];

/// `Struct.field` for every public field in the shown structs, leaving out fields that only
/// hold a list of another shown struct (those fields are listed under that struct).
fn source_fields() -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for (name, file) in SHOWN {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join(file);
        let text = std::fs::read_to_string(path).unwrap();
        let head = format!("pub struct {name} {{");
        let body = text
            .split(&head)
            .nth(1)
            .unwrap_or_else(|| panic!("{name} not found"));
        for line in body.lines().skip(1) {
            let line = line.trim();
            if line == "}" {
                break;
            }
            let Some(rest) = line.strip_prefix("pub ") else {
                continue;
            };
            let (field, ty) = rest.split_once(':').unwrap();
            let ty = ty.trim().trim_end_matches(',');
            let inner = ty
                .strip_prefix("Vec<")
                .and_then(|t| t.strip_suffix('>'))
                .unwrap_or(ty);
            if SHOWN.iter().any(|(n, _)| *n == inner) {
                continue;
            }
            out.insert(format!("{name}.{field}"));
        }
    }
    out
}

#[test]
fn the_list_equals_the_field_set() {
    let listed: BTreeSet<String> = display_fields(FIELDS)
        .unwrap()
        .into_iter()
        .map(|f| f.name)
        .collect();
    assert_eq!(listed, source_fields());
}

#[test]
fn a_field_without_a_source_or_kind_is_refused() {
    let no_source = "field = [{ name = \"Snapshot.tick\", kind = \"exact\" }]";
    assert!(display_fields(no_source).is_err());
    let empty_source = "field = [{ name = \"Snapshot.tick\", kind = \"exact\", source = \"\" }]";
    assert!(display_fields(empty_source).is_err());
    let bad_kind = "field = [{ name = \"Snapshot.tick\", kind = \"guess\", source = \"x\" }]";
    assert!(display_fields(bad_kind).is_err());
    let twice = "field = [{ name = \"A.b\", kind = \"exact\", source = \"x\" }, \
                 { name = \"A.b\", kind = \"exact\", source = \"x\" }]";
    assert!(display_fields(twice).is_err());
}

fn splitmix(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

struct Varying(Vec<MoraleReport>);

impl MoraleSource for Varying {
    fn report(&self, r: RegimentId) -> MoraleReport {
        self.0[usize::from(r.0) % self.0.len()].clone()
    }
}

#[test]
fn exact_fields_equal_the_simulation_over_200_random_battles() {
    let kinds: std::collections::BTreeMap<String, FieldKind> = display_fields(FIELDS)
        .unwrap()
        .into_iter()
        .map(|f| (f.name, f.kind))
        .collect();
    assert_eq!(kinds["EnemyView.strength_lo"], FieldKind::Range);
    assert_eq!(kinds["RegimentView.strength"], FieldKind::Exact);
    for case in 0..200u64 {
        let mut s = case;
        let n = 2 + splitmix(&mut s) % 4;
        let regs: Vec<_> = (0..n)
            .map(|_| {
                let f = Formation::ALL[(splitmix(&mut s) % 3) as usize];
                let (x, y) = (
                    (splitmix(&mut s) % 300) as i32,
                    (splitmix(&mut s) % 300) as i32,
                );
                (
                    (splitmix(&mut s) % 2) as u8,
                    4 + (splitmix(&mut s) % 20) as usize,
                    f,
                    x,
                    y,
                )
            })
            .collect();
        let mut b = battle(case, &regs);
        for r in 0..regs.len() {
            let order = match splitmix(&mut s) % 3 {
                0 => Order::Hold,
                1 => Order::Form(Formation::ALL[(splitmix(&mut s) % 3) as usize]),
                _ => Order::Face(core_math::Angle::from_raw(splitmix(&mut s) as u32)),
            };
            b.issue_order(RegimentId(r as u16), order);
        }
        (0..splitmix(&mut s) % 50).for_each(|_| b.step());
        let reports: Vec<_> = (0..3)
            .map(|k| MoraleReport {
                level: k as u8,
                margin: k - 1,
                trend: 0,
                factors: vec![(k as u8, 5)],
            })
            .collect();
        let snap = capture(
            &b,
            &FullInfo::new(&b),
            &Varying(reports.clone()),
            InfoMode::Hq { side: 0 },
            100,
        );
        assert_eq!(snap.tick, b.world.tick);
        assert_eq!(snap.clock_s, 100 + (b.world.tick / 20) as u32);
        let own: Vec<_> = b.regiments.iter().filter(|r| r.team == 0).collect();
        assert_eq!(snap.regiments.len(), own.len());
        for (v, r) in snap.regiments.iter().zip(&own) {
            assert_eq!(v.id, r.id.0);
            assert_eq!((v.side, v.pos, v.heading), (r.team, r.anchor, r.heading));
            assert_eq!(v.formation, r.state.target_formation().tag());
            assert_eq!(v.strength as usize, r.roster.len());
            assert_eq!(v.order, r.order.tag());
            assert_eq!(v.morale, reports[usize::from(r.id.0) % 3]);
        }
        let foes: Vec<_> = b.regiments.iter().filter(|r| r.team == 1).collect();
        assert_eq!(snap.enemies.len(), foes.len());
        for (e, r) in snap.enemies.iter().zip(&foes) {
            let truth = r.roster.len() as u32;
            assert_eq!(
                (e.unit, e.pos, e.pos_err_mm, e.last_seen_tick),
                (r.id.0, r.anchor, 0, b.world.tick)
            );
            assert!(
                e.strength_lo <= truth && truth <= e.strength_hi,
                "range must bracket truth"
            );
            assert_eq!((e.strength_lo % 50, e.strength_hi % 50), (0, 0));
        }
    }
}

fn sighting(lo: u16, hi: u16) -> Sighting {
    Sighting {
        unit: 9,
        last_seen_tick: 40,
        pos: common::metres(5, 6),
        pos_err_mm: 300,
        strength_lo: lo,
        strength_hi: hi,
        formation_known: false,
    }
}

fn range_of(lo: u16, hi: u16) -> (u32, u32) {
    let b = battle(1, &[(0, 4, Formation::Line, 0, 0)]);
    let info = Canned {
        sightings: vec![sighting(lo, hi)],
        orders: vec![],
    };
    let s = capture(
        &b,
        &info,
        &CannedMorale(steady()),
        InfoMode::Hq { side: 0 },
        0,
    );
    (s.enemies[0].strength_lo, s.enemies[0].strength_hi)
}

#[test]
fn range_is_rounded_to_fifty_and_moves_across_a_threshold() {
    assert_eq!(range_of(100, 100), (100, 100));
    assert_eq!(range_of(101, 149), (100, 150));
    assert_eq!(range_of(149, 151), (100, 200));
    assert_ne!(range_of(149, 149), range_of(151, 151));
    assert_eq!(range_of(0, 49), (0, 50));
}

#[test]
fn enemy_fields_come_from_the_information_source_only() {
    let b = battle(
        1,
        &[
            (0, 4, Formation::Line, 0, 0),
            (1, 20, Formation::Line, 50, 0),
        ],
    );
    let info = Canned {
        sightings: vec![sighting(180, 260)],
        orders: vec![],
    };
    let s = capture(
        &b,
        &info,
        &CannedMorale(steady()),
        InfoMode::Hq { side: 0 },
        0,
    );
    assert_eq!(s.enemies.len(), 1);
    let e = &s.enemies[0];
    assert_eq!((e.unit, e.pos_err_mm, e.formation_known), (9, 300, false));
    assert_eq!((e.strength_lo, e.strength_hi), (150, 300));
}
