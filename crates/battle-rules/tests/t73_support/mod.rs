//! Shared set-up for the SPEC-007 media tests (T-073): a 6.4 km field, the officers fixture,
//! a friction table with switchable friction, and a tick loop that records arrivals.
#![allow(dead_code)]

use battle_rules::command::{
    load_officers, CommandTable, Latitude, Link, Medium, Officer, OfficerId, OrderBody, TraitMask,
};
use battle_rules::{BattleState, Formation, Order, RegimentSpec, Tables, TerrainGrid, Training};
use core_math::{Angle, Fx, RngStreams, Vec2Fx};
use sim_state::{SpatialGrid, WorldState};

pub const RULES_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/rules");
pub const OFFICERS: &str = include_str!("../../../../qc/fixtures/command/officers_good.toml");

pub fn metres(x: i64, y: i64) -> Vec2Fx {
    Vec2Fx::new(Fx::from_int(x as i32), Fx::from_int(y as i32))
}

pub fn table() -> CommandTable {
    let text = std::fs::read_to_string(format!("{RULES_DIR}/command.toml")).unwrap();
    CommandTable::from_toml_str(&text).unwrap()
}

/// The table with no loss and no garbling, so only latency remains.
pub fn calm_table() -> CommandTable {
    let mut t = table();
    for m in &mut t.media {
        m.loss_base_ppm = 0;
        m.garble_ppm = 0;
    }
    t.globals.loss_ppm_per_km = 0;
    t.globals.exposure_ppm_per_enemy = 0;
    t
}

pub fn hold() -> OrderBody {
    OrderBody {
        order: Order::Hold,
        latitude: Latitude::Strict,
    }
}

pub fn walk_to(x: i64, y: i64) -> OrderBody {
    OrderBody {
        order: Order::Move {
            to: metres(x, y),
            gait: battle_rules::Gait::Quick,
        },
        latitude: Latitude::Strict,
    }
}

pub fn face(deg: i32) -> OrderBody {
    OrderBody {
        order: Order::Face(Angle::from_degrees_int(deg)),
        latitude: Latitude::Strict,
    }
}

/// A battle on open ground 6.4 km square (100 m cells) with regiments 0 and 1 of side 0 (20 men
/// each at 10 m and 30 m) and regiment 2 of side 1 with `enemies` men at `enemy_at`; the command
/// layer is on with the officers fixture, every officer at his own place.
pub fn field(seed: u64, enemies: usize, enemy_at: Vec2Fx) -> BattleState {
    let tables = Tables::load_dir(RULES_DIR).unwrap();
    let terrain = TerrainGrid::from_rows(
        100_000,
        0,
        0,
        &[('.', 0)],
        &vec![".".repeat(64); 64],
        &tables.terrain,
    )
    .unwrap();
    let grid = SpatialGrid::new(Fx::from_int(100), 64, 64, Vec2Fx::ZERO).unwrap();
    let mut b = BattleState::new(
        WorldState::new(RngStreams::new(seed)),
        terrain,
        grid,
        tables,
    );
    for (team, n, at) in [
        (0u8, 20, metres(10, 20)),
        (0, 20, metres(30, 40)),
        (1, enemies, enemy_at),
    ] {
        b.spawn_regiment(RegimentSpec {
            team,
            count: n,
            training: Training::Trained,
            formation: Formation::Line,
            anchor: at,
            heading: Angle::ZERO,
        });
    }
    let set = load_officers(OFFICERS).unwrap();
    let officers = set
        .officers
        .iter()
        .map(|s| Officer::from_spec(s, TraitMask(0)))
        .collect();
    let links = set.links.iter().map(Link::from_spec).collect();
    b.enable_command(officers, links).unwrap();
    b.grid.rebuild(&b.world.agents);
    b
}

/// Places officer `id` at the given point in metres.
pub fn place(b: &mut BattleState, id: u16, x: i64, y: i64) {
    let cmd = b.cmd.as_mut().unwrap();
    let i = cmd.index_of(OfficerId(id)).unwrap();
    cmd.officers[i].pos = metres(x, y);
}

/// Steps the message layer one tick at a time until `last` and returns `(tick, arrival)` pairs.
pub fn run_until(
    b: &mut BattleState,
    t: &CommandTable,
    last: u64,
) -> Vec<(u64, battle_rules::command::Arrival)> {
    let mut out = Vec::new();
    while b.world.tick <= last {
        for a in b.step_messages(t) {
            out.push((b.world.tick, a));
        }
        b.world.advance_tick();
    }
    out
}

pub fn medium_name(m: Medium) -> &'static str {
    match m {
        Medium::Courier => "courier",
        Medium::Flags => "flags",
        Medium::Telegraph => "telegraph",
    }
}
