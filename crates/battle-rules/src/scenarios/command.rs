//! Headless command scenarios (SPEC-007 sections 10 and 13): `command_chain`, the 27-regiment
//! tree of the golden replay; `cemetery_hill`, one tired or rested formation facing a contact;
//! and the layouts the calibration harness and the performance test run on.
//!
//! All coordinates are metres on a 6.4 km square of open ground. Officers of the army stand at a
//! fixed headquarters; the others stand at the mean of their regiments. No scenario draws a
//! random number: every draw belongs to a message, an observation or a soldier.

use crate::command::{CommandTable, Latitude, Level, Link, Medium, Officer, OfficerId, TraitMask};
use crate::{
    BattleState, Formation, Gait, Order, RegimentId, RegimentSpec, Tables, TerrainGrid, Training,
};
use core_math::{Angle, Fx, RngStreams, Vec2Fx};
use sim_state::{SpatialGrid, WorldState};

/// Army, corps, first division, first brigade and first regiment officer of [`command_tree`].
pub const ARMY: u16 = 1;
/// The corps officer of [`command_tree`].
pub const CORPS: u16 = 2;

/// Officer id of division `d` (0 to 2) of [`command_tree`].
pub const fn division(d: usize) -> u16 {
    10 + d as u16
}

/// Officer id of brigade `b` (0 to 8) of [`command_tree`].
pub const fn brigade(b: usize) -> u16 {
    20 + b as u16
}

/// Officer id of the owner of regiment `k` (0 to 26) of [`command_tree`].
pub const fn regiment_officer(k: usize) -> u16 {
    100 + k as u16
}

/// Men in each regiment of [`command_tree`].
const TREE_MEN: usize = 40;
/// Men of the formation and of the contact in `cemetery_hill`: odds of 700 permille at the
/// sighting quality 900 that a contact 180 m away gives.
const HILL_OWN: usize = 210;
const HILL_ENEMY: usize = 100;

fn metres(x: i64, y: i64) -> Vec2Fx {
    Vec2Fx::new(Fx::from_int(x as i32), Fx::from_int(y as i32))
}

/// An empty battle on 6.4 km of open ground.
fn field(tables: &Tables, seed: u64) -> BattleState {
    let terrain = TerrainGrid::from_rows(8000, 0, 0, &[('.', 0)], &["."], &tables.terrain)
        .expect("scenario terrain is valid");
    let grid = SpatialGrid::new(Fx::from_int(40), 160, 160, Vec2Fx::ZERO).expect("grid size");
    BattleState::new(
        WorldState::new(RngStreams::new(seed)),
        terrain,
        grid,
        tables.clone(),
    )
}

fn regiment(b: &mut BattleState, team: u8, men: usize, at: Vec2Fx) -> RegimentId {
    b.spawn_regiment(RegimentSpec {
        team,
        count: men,
        training: Training::Trained,
        formation: Formation::Line,
        anchor: at,
        heading: if team == 0 { Angle::ZERO } else { Angle::HALF },
    })
}

fn officer(
    id: u16,
    level: Level,
    parent: Option<u16>,
    reg: Option<usize>,
    hq: Option<Vec2Fx>,
) -> Officer {
    Officer {
        id: OfficerId(id),
        side: 0,
        level,
        parent: parent.map(OfficerId),
        regiment: reg.map(|k| RegimentId(k as u16)),
        mask: TraitMask(0),
        deputy: None,
        down: false,
        pos: hq.unwrap_or(Vec2Fx::ZERO),
        hq_fixed: hq,
        post_range_pm: 1000,
        seq: 0,
        last_seq_from: Vec::new(),
        last_report: 0,
    }
}

fn enable(b: &mut BattleState, officers: Vec<Officer>, links: Vec<Link>) {
    b.enable_command(officers, links)
        .expect("scenario officers ascend and name regiments that exist");
    b.grid.rebuild(&b.world.agents);
}

/// Position of regiment `k` (0 to 26) of [`command_tree`]: divisions 700 m apart, brigades
/// 200 m apart, regiments 80 m apart.
fn tree_anchor(k: usize) -> Vec2Fx {
    let (d, b, r) = (k / 9, (k / 3) % 3, k % 3);
    metres(3000 + 700 * d as i64 + 200 * b as i64, 1500 + 80 * r as i64)
}

/// The officers of `command_chain` over 27 regiments of 40 men of side 0 (an army at a fixed
/// headquarters 3 km behind a corps, three divisions of three brigades of three regiments, each
/// officer posted at the mean of his regiments), and one enemy regiment of 100 men 1.5 km beyond
/// the last regiment. With `links` the army has a telegraph to the corps and the first brigade a
/// flag link to its first regiment. Nothing has been ordered.
pub fn command_tree(tables: &Tables, seed: u64, links: bool) -> BattleState {
    let mut b = field(tables, seed);
    for k in 0..27 {
        regiment(&mut b, 0, TREE_MEN, tree_anchor(k));
    }
    regiment(&mut b, 1, 100, metres(6300, 1660));
    let mut o = vec![
        officer(ARMY, Level::Army, None, None, Some(metres(500, 1500))),
        officer(CORPS, Level::Corps, Some(ARMY), None, None),
    ];
    o.extend((0..3).map(|d| officer(division(d), Level::Division, Some(CORPS), None, None)));
    o.extend((0..9).map(|k| {
        officer(
            brigade(k),
            Level::Brigade,
            Some(division(k / 3)),
            None,
            None,
        )
    }));
    o.extend((0..27).map(|k| {
        officer(
            regiment_officer(k),
            Level::Regiment,
            Some(brigade(k / 3)),
            Some(k),
            None,
        )
    }));
    let wires = if links {
        vec![
            Link {
                medium: Medium::Telegraph,
                a: OfficerId(ARMY),
                b: OfficerId(CORPS),
                cut: false,
            },
            Link {
                medium: Medium::Flags,
                a: OfficerId(brigade(0)),
                b: OfficerId(regiment_officer(0)),
                cut: false,
            },
        ]
    } else {
        Vec::new()
    };
    enable(&mut b, o, wires);
    b
}

/// `command_chain`: [`command_tree`] with its links, and at tick 0 the army sends the corps a
/// discretionary quick-time move 300 m east of the corps, by the fastest medium.
pub fn command_chain(tables: &Tables, command: &CommandTable, seed: u64) -> BattleState {
    let mut b = command_tree(tables, seed, true);
    let order = Order::Move {
        to: metres(4300, 1580),
        gait: Gait::Quick,
    };
    b.dispatch(
        command,
        OfficerId(ARMY),
        OfficerId(CORPS),
        None,
        order,
        Latitude::Discretionary,
    )
    .expect("army and corps are on one side and the army is up");
    b
}

/// `cemetery_hill`: an army officer at 500 m and the owner of a regiment of 210 men (the mask
/// given, every man at `fatigue_ppm`), and 180 m beyond it a regiment of 100 enemy men, so that
/// observation gives the formation a contact at quality 900 and, noise aside, odds of 700
/// permille. At tick 0 the army sends the owner a discretionary quick-time move to 400 m beyond
/// the regiment, within the practicable radius of the contact.
pub fn cemetery_hill(
    tables: &Tables,
    command: &CommandTable,
    seed: u64,
    fatigue_ppm: u32,
    mask: TraitMask,
) -> BattleState {
    let mut b = field(tables, seed);
    let own = regiment(&mut b, 0, HILL_OWN, metres(1000, 1000));
    regiment(&mut b, 1, HILL_ENEMY, metres(1180, 1000));
    let men = b.regiments[usize::from(own.0)].roster.clone();
    for a in men {
        b.loco[a.index as usize].fatigue = fatigue_ppm;
    }
    let mut owner = officer(2, Level::Regiment, Some(1), Some(0), None);
    owner.mask = mask;
    let army = officer(1, Level::Army, None, None, Some(metres(500, 1000)));
    enable(&mut b, vec![army, owner], Vec::new());
    let order = Order::Move {
        to: metres(1400, 1000),
        gait: Gait::Quick,
    };
    b.dispatch(
        command,
        OfficerId(1),
        OfficerId(2),
        Some(Medium::Courier),
        order,
        Latitude::Discretionary,
    )
    .expect("both officers are on one side");
    b
}

/// A line of `n` officers of side 0 with no regiments, `spacing_m` apart on the x axis from 200 m,
/// officer `i + 1` of rank `i` under officer `i`; with `link` the first two share that medium.
/// The harness sends along it to measure delivery and friction.
pub fn command_line(
    tables: &Tables,
    seed: u64,
    n: usize,
    spacing_m: i64,
    link: Option<Medium>,
) -> BattleState {
    const RANKS: [Level; 4] = [Level::Army, Level::Corps, Level::Division, Level::Brigade];
    let mut b = field(tables, seed);
    let o = (0..n)
        .map(|i| {
            let at = Some(metres(200 + spacing_m * i as i64, 3200));
            officer(
                i as u16 + 1,
                RANKS[i],
                (i > 0).then_some(i as u16),
                None,
                at,
            )
        })
        .collect();
    let wires = link
        .map(|medium| {
            vec![Link {
                medium,
                a: OfficerId(1),
                b: OfficerId(2),
                cut: false,
            }]
        })
        .unwrap_or_default();
    enable(&mut b, o, wires);
    b
}

/// The size of the H7 measurement: 270 regiments of 20 men of side 0 (five to a brigade, 18
/// brigades to each of three divisions, two corps, one army: 330 officers) and ten enemy
/// regiments, with a hold sent from every officer to each of his subordinates at tick 0 so that
/// at least 300 messages are in flight.
pub fn command_perf(tables: &Tables, command: &CommandTable, seed: u64) -> BattleState {
    let mut b = field(tables, seed);
    for k in 0..270i64 {
        regiment(
            &mut b,
            0,
            20,
            metres(100 + 40 * (k % 30), 200 + 60 * (k / 30)),
        );
    }
    for e in 0..10i64 {
        regiment(&mut b, 1, 20, metres(1700 + 40 * e, 3000));
    }
    let mut o = vec![officer(1, Level::Army, None, None, Some(metres(100, 100)))];
    o.extend((0..2).map(|c| officer(2 + c, Level::Corps, Some(1), None, None)));
    o.extend((0..3).map(|d| {
        officer(
            4 + d,
            Level::Division,
            Some(2 + u16::from(d > 1)),
            None,
            None,
        )
    }));
    o.extend((0..54).map(|k| officer(10 + k, Level::Brigade, Some(4 + k / 18), None, None)));
    o.extend((0..270).map(|k| {
        officer(
            100 + k,
            Level::Regiment,
            Some(10 + k / 5),
            Some(usize::from(k)),
            None,
        )
    }));
    let pairs: Vec<(u16, u16)> = o
        .iter()
        .filter_map(|x| x.parent.map(|p| (p.0, x.id.0)))
        .collect();
    enable(&mut b, o, Vec::new());
    for (from, to) in pairs {
        b.dispatch(
            command,
            OfficerId(from),
            OfficerId(to),
            Some(Medium::Courier),
            Order::Hold,
            Latitude::Strict,
        )
        .expect("every officer is up and on side 0");
    }
    b
}
