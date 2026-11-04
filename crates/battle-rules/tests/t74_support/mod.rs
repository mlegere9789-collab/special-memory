//! Shared set-up for the SPEC-007 chain tests (T-074): a 27-regiment tree of officers on open
//! ground, a table with no randomness in latency, and independent latency arithmetic.
#![allow(dead_code)]

use battle_rules::command::{CommandTable, Level, Officer, OfficerId, TraitMask};
use battle_rules::{
    BattleState, Formation, RegimentId, RegimentSpec, Tables, TerrainGrid, Training,
};
use core_math::{Angle, Fx, RngStreams, Vec2Fx};
use sim_state::{SpatialGrid, WorldState};

pub const RULES_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/rules");

pub fn metres(x: i64, y: i64) -> Vec2Fx {
    Vec2Fx::new(Fx::from_int(x as i32), Fx::from_int(y as i32))
}

/// Friction off and no random latency: no loss, no garbling, no jitter, officers placed once.
pub fn exact_table() -> CommandTable {
    let text = std::fs::read_to_string(format!("{RULES_DIR}/command.toml")).unwrap();
    let mut t = CommandTable::from_toml_str(&text).unwrap();
    for m in &mut t.media {
        m.loss_base_ppm = 0;
        m.garble_ppm = 0;
    }
    let g = &mut t.globals;
    g.loss_ppm_per_km = 0;
    g.exposure_ppm_per_enemy = 0;
    g.jitter_ticks = 0;
    g.staff_jitter_ticks = 0;
    g.judge_sigma_pm = 0;
    g.pos_interval_ticks = 1_000_000;
    t
}

/// Anchors of the 27 regiments of the tree, regiment `k` at `(150 * (k % 9) + 5 * k, 400 * (k / 9) + 3 * k)`.
pub fn anchor(k: usize) -> Vec2Fx {
    metres(
        150 * (k as i64 % 9) + 5 * k as i64,
        400 * (k as i64 / 9) + 3 * k as i64,
    )
}

pub const ARMY: u16 = 1;
pub const CORPS: u16 = 2;

pub fn division(d: usize) -> u16 {
    10 + d as u16
}
pub fn brigade(b: usize) -> u16 {
    20 + b as u16
}
pub fn regiment_officer(k: usize) -> u16 {
    100 + k as u16
}

/// `(id, level, parent, regiment)` of every officer of the tree: an army over one corps over
/// three divisions of three brigades of three regiments.
pub fn tree_rows() -> Vec<(u16, Level, Option<u16>, Option<usize>)> {
    let mut rows = vec![
        (ARMY, Level::Army, None, None),
        (CORPS, Level::Corps, Some(ARMY), None),
    ];
    for d in 0..3 {
        rows.push((division(d), Level::Division, Some(CORPS), None));
    }
    for b in 0..9 {
        rows.push((brigade(b), Level::Brigade, Some(division(b / 3)), None));
    }
    for k in 0..27 {
        rows.push((
            regiment_officer(k),
            Level::Regiment,
            Some(brigade(k / 3)),
            Some(k),
        ));
    }
    rows
}

/// A battle with the 27 regiments of side 0 (20 men each) and the tree of officers enabled.
pub fn tree(seed: u64) -> BattleState {
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
    for k in 0..27 {
        b.spawn_regiment(RegimentSpec {
            team: 0,
            count: 20,
            training: Training::Trained,
            formation: Formation::Line,
            anchor: anchor(k),
            heading: Angle::ZERO,
        });
    }
    let officers = tree_rows()
        .into_iter()
        .map(|(id, level, parent, reg)| Officer {
            id: OfficerId(id),
            side: 0,
            level,
            parent: parent.map(OfficerId),
            regiment: reg.map(|k| RegimentId(k as u16)),
            mask: TraitMask(0),
            deputy: None,
            down: false,
            pos: Vec2Fx::ZERO,
            hq_fixed: None,
            post_range_pm: 1000,
            seq: 0,
            last_seq_from: Vec::new(),
            last_report: 0,
        })
        .collect();
    b.enable_command(officers, Vec::new()).unwrap();
    b.grid.rebuild(&b.world.agents);
    b
}

/// Regiment indices beneath an officer of the tree.
pub fn regiments_of(id: u16) -> Vec<usize> {
    (0..27)
        .filter(|&k| {
            let mut at = Some(regiment_officer(k));
            while let Some(a) = at {
                if a == id {
                    return true;
                }
                at = tree_rows().iter().find(|r| r.0 == a).and_then(|r| r.2);
            }
            false
        })
        .collect()
}

/// Integer mean (truncating) of the anchors of a formation: where its officer stands.
pub fn centroid(id: u16) -> Vec2Fx {
    let ks = regiments_of(id);
    let n = ks.len() as i128;
    let mean = |f: fn(Vec2Fx) -> Fx| {
        Fx::from_raw(
            (ks.iter()
                .map(|&k| i128::from(f(anchor(k)).raw()))
                .sum::<i128>()
                / n) as i64,
        )
    };
    Vec2Fx::new(mean(|p| p.x), mean(|p| p.y))
}

/// Ticks a courier needs between two points on open ground, by the SPEC-007 formula.
pub fn courier_leg(table: &CommandTable, a: Vec2Fx, b: Vec2Fx) -> u64 {
    let d = (i128::from((b - a).length().raw()) * 1000) >> 32;
    let num = d * i128::from(table.globals.path_factor_pm) * 20;
    let den = i128::from(table.media[0].speed_mm_s) * 1000;
    ((num + den - 1) / den) as u64
}

/// Tick at which the staff of `id` finishes an order that left its sender at `sent` by courier.
pub fn ready_tick(table: &CommandTable, from: u16, id: u16, level: Level, sent: u64) -> u64 {
    let m = &table.media[0];
    let arrive = sent
        + u64::from(m.write_ticks)
        + u64::from(m.base_ticks)
        + courier_leg(table, centroid(from), centroid(id));
    arrive + u64::from(table.levels[level.tag() as usize].staff_ticks)
}
