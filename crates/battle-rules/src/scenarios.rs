//! Headless movement scenarios (SPEC-004 section 12). Each builds a [`BattleState`] at tick 0
//! with its orders already issued; none draws random numbers, so the seed only enters the hash
//! through the world header.

pub mod fidelity;
pub mod fire;

use crate::{BattleState, Formation, Gait, Order, RegimentSpec, Tables, TerrainGrid, Training};
use core_math::{Angle, Fx, RngStreams, Vec2Fx};
use sim_state::{SpatialGrid, WorldState};

/// Names accepted by [`by_name`].
pub const NAMES: [&str; 4] = [
    "march_single",
    "deploy_pair",
    "terrain_strip",
    "march_large",
];

/// Metres from `march_single`'s anchor to its destination.
pub const MARCH_M: i32 = 200;

fn metres(x: i32, y: i32) -> Vec2Fx {
    Vec2Fx::new(Fx::from_int(x), Fx::from_int(y))
}

/// An empty battle on the rectangle `[x0, x0 + 2 w) x [y0, y0 + 2 h)` metres (the spatial index
/// has 2 m cells, a few men each, so a personal-space query touches little); `rows` is the
/// terrain (`.` open, `W` woods; positions outside the rows take the nearest edge cell).
fn field(
    tables: &Tables,
    seed: u64,
    (x0, y0): (i32, i32),
    (w, h): (u32, u32),
    rows: &[&str],
) -> BattleState {
    let legend = [('.', 0), ('W', 3)];
    let terrain = TerrainGrid::from_rows(8000, 0, 0, &legend, rows, &tables.terrain)
        .expect("scenario terrain is valid");
    let grid = SpatialGrid::new(Fx::from_int(2), w, h, metres(x0, y0)).expect("grid size is valid");
    BattleState::new(
        WorldState::new(RngStreams::new(seed)),
        terrain,
        grid,
        tables.clone(),
    )
}

fn column(team: u8, count: usize, training: Training, anchor: Vec2Fx) -> RegimentSpec {
    RegimentSpec {
        team,
        count,
        training,
        formation: Formation::Column,
        anchor,
        heading: Angle::ZERO,
    }
}

fn march_on(mut b: BattleState, gait: Gait) -> BattleState {
    let id = b.spawn_regiment(column(0, 400, Training::Trained, Vec2Fx::ZERO));
    b.issue_order(
        id,
        Order::Move {
            to: metres(MARCH_M, 0),
            gait,
        },
    );
    b
}

/// One 400-man trained column marches 200 m east at `gait` over open ground.
pub fn march_single_at(tables: &Tables, seed: u64, gait: Gait) -> BattleState {
    march_on(field(tables, seed, (-64, -64), (192, 64), &["."]), gait)
}

/// `march_single`: [`march_single_at`] at quick time.
pub fn march_single(tables: &Tables, seed: u64) -> BattleState {
    march_single_at(tables, seed, Gait::Quick)
}

/// `deploy_pair`: a raw and a veteran column, 300 m apart in y, both ordered into line at tick 0.
pub fn deploy_pair(tables: &Tables, seed: u64) -> BattleState {
    let mut b = field(tables, seed, (-64, -128), (64, 288), &["."]);
    for (team, training, y) in [(0, Training::Raw, 0), (1, Training::Veteran, 300)] {
        let id = b.spawn_regiment(column(team, 400, training, metres(0, y)));
        b.issue_order(id, Order::Form(Formation::Line));
    }
    b
}

/// `terrain_strip`: `march_single` across a 40 m wide strip of woods centred at x = 100.
pub fn terrain_strip(tables: &Tables, seed: u64) -> BattleState {
    let row = format!("{}{}{}", ".".repeat(10), "W".repeat(5), ".".repeat(25));
    march_on(
        field(tables, seed, (-64, -64), (192, 64), &[row.as_str()]),
        Gait::Quick,
    )
}

/// `march_large`: 50 regiments of 400 (20,000 soldiers), 100 m apart in y, all marching 300 m
/// east at quick time.
pub fn march_large(tables: &Tables, seed: u64) -> BattleState {
    let mut b = field(tables, seed, (-64, -64), (220, 2560), &["."]);
    for i in 0..50 {
        let team = u8::from(i % 2 == 1);
        let id = b.spawn_regiment(column(team, 400, Training::Trained, metres(0, 100 * i)));
        b.issue_order(
            id,
            Order::Move {
                to: metres(300, 100 * i),
                gait: Gait::Quick,
            },
        );
    }
    b
}

/// The scenario called `name` (see [`NAMES`]).
pub fn by_name(name: &str, tables: &Tables, seed: u64) -> Option<BattleState> {
    match name {
        "march_single" => Some(march_single(tables, seed)),
        "deploy_pair" => Some(deploy_pair(tables, seed)),
        "terrain_strip" => Some(terrain_strip(tables, seed)),
        "march_large" => Some(march_large(tables, seed)),
        _ => None,
    }
}
