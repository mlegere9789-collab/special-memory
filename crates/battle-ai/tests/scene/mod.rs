//! Shared set-up for the lookahead tests: a truth world and the picture a headquarters would
//! hold of it. Integration tests may name the world state; the crate's own sources may not.
#![allow(dead_code)]

use battle_ai::{Area, BeliefView, Contact, ContactId, GridSpec, MapView, OwnReport, UnitRef};
use battle_rules::{
    BattleState, FireMode, Formation, RegimentId, RegimentSpec, Tables, TerrainGrid, Training,
};
use core_math::{Angle, Fx, RngStreams, Vec2Fx};
use sim_state::{SpatialGrid, WorldState};

pub const WEAPON: &str = "rifle_58";

pub fn tables() -> Tables {
    Tables::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/rules")).expect("tables")
}

pub fn map(t: &Tables) -> MapView {
    let terrain = TerrainGrid::from_rows(8000, 0, 0, &[('.', 0)], &["........"; 8], &t.terrain)
        .expect("terrain");
    MapView {
        terrain,
        obstacles: Default::default(),
        heights_mm: None,
        key_points: Vec::new(),
    }
}

pub fn m(x: i32, y: i32) -> Vec2Fx {
    Vec2Fx::new(Fx::from_int(x), Fx::from_int(y))
}

pub fn grid() -> GridSpec {
    GridSpec {
        cell_m: 8,
        width: 256,
        height: 64,
        origin: Vec2Fx::ZERO,
    }
}

pub fn area() -> Area {
    Area {
        center: m(200, 120),
        radius_mm: 150_000,
    }
}

/// An empty truth world with `seed` over `map`.
pub fn world(seed: u64, t: &Tables, map: &MapView) -> BattleState {
    let g = grid();
    BattleState::new(
        WorldState::new(RngStreams::new(seed)),
        map.terrain.clone(),
        SpatialGrid::new(Fx::from_int(g.cell_m), g.width, g.height, g.origin).expect("grid"),
        t.clone(),
    )
}

/// The heading the belief gives a contact: toward the objective centre.
pub fn toward(from: Vec2Fx, to: Vec2Fx) -> Angle {
    Angle::atan2(to.y - from.y, to.x - from.x)
}

pub fn spawn(b: &mut BattleState, team: u8, n: usize, at: Vec2Fx, heading: Angle) -> RegimentId {
    let id = b.spawn_regiment(RegimentSpec {
        team,
        count: n,
        training: Training::Trained,
        formation: Formation::Line,
        anchor: at,
        heading,
    });
    b.equip_regiment(id, WEAPON, 0).expect("weapon");
    id
}

/// Reports of team 0 and contacts for the enemy regiments in `visible` (ids ascending), exactly
/// as the truth holds them. Contact ids are `500 + regiment index`.
pub fn picture(b: &BattleState, side: u8, visible: &[u16]) -> (Vec<OwnReport>, Vec<Contact>) {
    let widx = b.tables.weapons.index_of(WEAPON).expect("weapon");
    let mut own = Vec::new();
    let mut foes = Vec::new();
    for r in &b.regiments {
        let strength = b.strength(r.id).unwrap() as u16;
        if r.team == side {
            own.push(OwnReport {
                unit: UnitRef(r.id.0),
                pos: r.anchor,
                heading: r.heading,
                formation: r.state.target_formation(),
                strength,
                strength_ref: strength,
                rounds_pm: b.rounds_pm(r.id).unwrap(),
                fatigue_pm: b.fatigue_pm(r.id).unwrap(),
                cohesion_pm: None,
                age_ticks: 0,
            });
        } else if visible.contains(&r.id.0) {
            foes.push(Contact {
                id: ContactId(500 + u32::from(r.id.0)),
                pos: r.anchor,
                strength_est: strength,
                formation_est: Some(r.state.target_formation()),
                weapon_est: Some(widx as u8),
                last_seen: b.world.tick,
                quality_pm: 1000,
            });
        }
    }
    (own, foes)
}

pub fn view<'a>(
    side: u8,
    now: u64,
    (reports, contacts): (&'a [OwnReport], &'a [Contact]),
    previous: &'a [Contact],
    map: &'a MapView,
    tables: &'a Tables,
) -> BeliefView<'a> {
    BeliefView {
        side,
        now,
        reports,
        contacts,
        previous,
        map,
        tables,
        grid: grid(),
        area: area(),
        own_weapon: WEAPON,
        enemy_weapon: WEAPON,
    }
}

/// The nearest regiment of another team to `of`, lowest id on a tie.
pub fn nearest_foe(b: &BattleState, of: RegimentId) -> Option<RegimentId> {
    let me = &b.regiments[usize::from(of.0)];
    b.regiments
        .iter()
        .filter(|r| r.team != me.team)
        .min_by_key(|r| {
            let (dx, dy) = (
                r.anchor.x.raw() - me.anchor.x.raw(),
                r.anchor.y.raw() - me.anchor.y.raw(),
            );
            (i128::from(dx).pow(2) + i128::from(dy).pow(2), r.id)
        })
        .map(|r| r.id)
}

pub fn at_will(b: &mut BattleState, who: RegimentId, target: RegimentId) {
    assert!(b.set_fire_mode(who, FireMode::AtWill { target }));
}
