//! T4-09: slot layout dimensions and `assign_slots` (SPEC-004 section 8), plus the roster helper.

use battle_rules::{
    assign_slots, fx_ratio, regiment_roster, slot_layout, DrillTable, Formation, SlotLocal,
    F_DISABLED, F_WOUNDED,
};
use core_math::{Angle, Fx, Vec2Fx};
use proptest::prelude::*;
use sim_state::{AgentId, AgentInit, AgentStore};

const FILE: &str = include_str!("../../../data/rules/drill.toml");

fn table() -> DrillTable {
    DrillTable::from_toml_str(FILE).unwrap()
}

fn id(index: u32) -> AgentId {
    AgentId {
        index,
        generation: 0,
    }
}

fn frame(heading: Angle) -> (Vec2Fx, Vec2Fx) {
    (
        Vec2Fx::from_angle(heading),
        Vec2Fx::from_angle(heading - Angle::QUARTER),
    )
}

fn project(p: Vec2Fx, anchor: Vec2Fx, heading: Angle) -> (i64, i64) {
    let (forward, right) = frame(heading);
    let d = p - anchor;
    (d.dot(right).raw(), d.dot(forward).raw())
}

/// Rank sizes of a layout as the spec defines them: runs of equal `v`.
fn ranks(layout: &[SlotLocal]) -> Vec<(usize, usize)> {
    let mut out: Vec<(usize, usize)> = Vec::new();
    for (i, s) in layout.iter().enumerate() {
        match out.last_mut() {
            Some((start, len)) if layout[*start].v == s.v => *len += 1,
            _ => out.push((i, 1)),
        }
    }
    out
}

fn assert_canonical(layout: &[SlotLocal]) {
    for (start, len) in ranks(layout) {
        for k in start + 1..start + len {
            assert!(layout[k - 1].u <= layout[k].u, "u not ascending in rank");
        }
    }
    for w in layout.windows(2) {
        assert!(w[0].v >= w[1].v, "v increases along the layout");
    }
}

#[test]
fn line_of_400_is_two_ranks_of_200_and_200_files_wide() {
    let t = table();
    let l = slot_layout(Formation::Line, 400, &t);
    assert_eq!(l.len(), 400);
    assert_eq!(ranks(&l), vec![(0, 200), (200, 200)]);
    assert_canonical(&l);
    let (min_u, max_u) = (l[0].u, l[199].u);
    // width = span of file centres + one file width = 200 * 560 mm, to within fixed-point rounding.
    let width = max_u - min_u + fx_ratio(560, 1000);
    let want = fx_ratio(200 * 560, 1000);
    assert!(
        (width.raw() - want.raw()).abs() <= 3,
        "{width:?} vs {want:?}"
    );
    assert_eq!(l[0].u, -l[199].u, "centred on the anchor");
    assert_eq!(l[0].v, Fx::ZERO);
    assert_eq!(l[200].v, fx_ratio(-900, 1000));
    // Adjacent files are one file width apart (to the rounding of the exact ratio).
    let step = l[1].u - l[0].u;
    assert!((step.raw() - fx_ratio(560, 1000).raw()).abs() <= 1);
}

#[test]
fn line_with_odd_count_has_a_longer_front_rank() {
    let t = table();
    let l = slot_layout(Formation::Line, 401, &t);
    assert_eq!(ranks(&l), vec![(0, 201), (201, 200)]);
    assert_eq!(l[100].u, Fx::ZERO, "middle file of 201 is on the anchor");
    assert_canonical(&l);
}

#[test]
fn column_of_400_is_fifty_ranks_of_eight() {
    let t = table();
    let l = slot_layout(Formation::Column, 400, &t);
    let r = ranks(&l);
    assert_eq!(r.len(), 50);
    assert!(r.iter().all(|&(_, len)| len == 8));
    assert_eq!(l[49 * 8].v, fx_ratio(-49 * 900, 1000));
    assert_canonical(&l);
    // A partial last rank is centred on its own count.
    let l = slot_layout(Formation::Column, 11, &t);
    assert_eq!(ranks(&l), vec![(0, 8), (8, 3)]);
    assert_eq!(l[9].u, Fx::ZERO);
}

#[test]
fn skirmish_of_400_has_40_deployed_and_a_reserve_column() {
    let t = table();
    let l = slot_layout(Formation::Skirmish, 400, &t);
    assert_eq!(l.len(), 400);
    let r = ranks(&l);
    assert_eq!(r[0], (0, 40));
    assert!(l[..40].iter().all(|s| s.v == Fx::ZERO));
    let step = l[1].u - l[0].u;
    assert!((step.raw() - fx_ratio(3500, 1000).raw()).abs() <= 1);
    // Reserve: 360 men, 8 wide, starting 100 m behind the skirmish rank.
    assert_eq!(l[40].v, fx_ratio(-100_000, 1000));
    assert_eq!(r.len(), 1 + 45);
    assert!(r[1..].iter().all(|&(_, len)| len == 8));
    assert_canonical(&l);
    // 401 men: ceil(40.1) = 41 deployed.
    let l = slot_layout(Formation::Skirmish, 401, &t);
    assert_eq!(ranks(&l)[0], (0, 41));
}

#[test]
fn tiny_layouts_are_well_formed() {
    let t = table();
    for f in Formation::ALL {
        assert!(slot_layout(f, 0, &t).is_empty());
        for n in 1..40 {
            let l = slot_layout(f, n, &t);
            assert_eq!(l.len(), n);
            assert_canonical(&l);
        }
    }
    let one = slot_layout(Formation::Line, 1, &table());
    assert_eq!(one, vec![SlotLocal::default()]);
}

#[test]
fn soldiers_standing_on_their_slots_keep_them() {
    let t = table();
    let anchor = Vec2Fx::new(Fx::from_int(30), Fx::from_int(-12));
    for f in Formation::ALL {
        for deg in [0, 30, 90, 200] {
            let heading = Angle::from_degrees_int(deg);
            let layout = slot_layout(f, 120, &t);
            let (forward, right) = frame(heading);
            // Soldier k stands on slot k; list them in a scrambled order.
            let mut soldiers: Vec<(AgentId, Vec2Fx)> = layout
                .iter()
                .enumerate()
                .map(|(k, s)| (id(k as u32), anchor + right.scale(s.u) + forward.scale(s.v)))
                .collect();
            soldiers.reverse();
            soldiers.rotate_left(17);
            let got = assign_slots(&soldiers, &layout, anchor, heading);
            let want: Vec<(AgentId, usize)> = (0..120).map(|k| (id(k as u32), k)).collect();
            assert_eq!(got, want, "{f:?} at {deg} degrees");
        }
    }
}

#[test]
fn ties_break_by_ascending_index() {
    // Four soldiers on one point, four slots in one rank: ids are handed out in index order.
    let layout = vec![
        SlotLocal {
            u: Fx::from_int(-3),
            v: Fx::ZERO,
        },
        SlotLocal {
            u: Fx::from_int(-1),
            v: Fx::ZERO,
        },
        SlotLocal {
            u: Fx::from_int(1),
            v: Fx::ZERO,
        },
        SlotLocal {
            u: Fx::from_int(3),
            v: Fx::ZERO,
        },
    ];
    let p = Vec2Fx::new(Fx::from_int(5), Fx::from_int(5));
    let a = [(id(9), p), (id(2), p), (id(7), p), (id(4), p)];
    let got = assign_slots(&a, &layout, Vec2Fx::ZERO, Angle::ZERO);
    let order: Vec<u32> = got.iter().map(|(a, _)| a.index).collect();
    assert_eq!(order, vec![2, 4, 7, 9]);
    assert_eq!(
        got.iter().map(|g| g.1).collect::<Vec<_>>(),
        vec![0, 1, 2, 3]
    );
}

#[test]
fn front_men_take_the_front_rank() {
    // Two ranks of two: the two soldiers with the larger forward offset take rank 0.
    let layout = vec![
        SlotLocal {
            u: Fx::from_int(-1),
            v: Fx::ZERO,
        },
        SlotLocal {
            u: Fx::from_int(1),
            v: Fx::ZERO,
        },
        SlotLocal {
            u: Fx::from_int(-1),
            v: Fx::from_int(-1),
        },
        SlotLocal {
            u: Fx::from_int(1),
            v: Fx::from_int(-1),
        },
    ];
    let (forward, right) = frame(Angle::ZERO);
    let place = |u: i32, v: i32| right.scale(Fx::from_int(u)) + forward.scale(Fx::from_int(v));
    let soldiers = [
        (id(0), place(5, -20)),
        (id(1), place(-5, 20)),
        (id(2), place(7, 21)),
        (id(3), place(0, -30)),
    ];
    let got = assign_slots(&soldiers, &layout, Vec2Fx::ZERO, Angle::ZERO);
    // Front: ids 2 (v 21) and 1 (v 20), left to right by u: id 1 (u -5) then id 2 (u 7).
    // Rear: ids 0 (v -20) and 3 (v -30), by u: id 3 (u 0) then id 0 (u 5).
    assert_eq!(got, vec![(id(1), 0), (id(2), 1), (id(3), 2), (id(0), 3)]);
}

#[test]
fn empty_inputs_give_empty_results() {
    let t = table();
    let l = slot_layout(Formation::Line, 4, &t);
    assert!(assign_slots(&[], &l, Vec2Fx::ZERO, Angle::ZERO).is_empty());
    let p = [(id(0), Vec2Fx::ZERO)];
    assert!(assign_slots(&p, &[], Vec2Fx::ZERO, Angle::ZERO).is_empty());
}

#[test]
fn roster_is_ascending_and_skips_the_down_and_other_units() {
    let mut store = AgentStore::new();
    let mut ids = Vec::new();
    for i in 0..12u32 {
        ids.push(store.spawn(AgentInit {
            unit: i % 3,
            ..AgentInit::default()
        }));
    }
    // Unit 1 holds indices 1, 4, 7, 10.
    store.set_flags(ids[4], F_DISABLED);
    store.set_flags(ids[7], F_WOUNDED);
    assert_eq!(regiment_roster(&store, 1), vec![ids[1], ids[10]]);
    store.despawn(ids[1]);
    assert_eq!(regiment_roster(&store, 1), vec![ids[10]]);
    // Reuse of the freed slot (LIFO) keeps ascending index order.
    let again = store.spawn(AgentInit {
        unit: 1,
        ..AgentInit::default()
    });
    assert_eq!(again.index, 1);
    assert_eq!(regiment_roster(&store, 1), vec![again, ids[10]]);
    let r = regiment_roster(&store, 0);
    assert!(r.windows(2).all(|w| w[0].index < w[1].index));
    assert_eq!(r.len(), 4);
    assert!(regiment_roster(&store, 99).is_empty());
}

fn formation_strategy() -> impl Strategy<Value = Formation> {
    prop_oneof![
        Just(Formation::Column),
        Just(Formation::Line),
        Just(Formation::Skirmish)
    ]
}

fn heading_strategy() -> impl Strategy<Value = i32> {
    prop_oneof![Just(0), Just(90), Just(180), Just(270), 0..360i32]
}

/// Soldiers with distinct ids on integer-metre positions (many ties), plus a shuffled copy.
type Soldiers = Vec<(AgentId, Vec2Fx)>;

fn soldiers() -> impl Strategy<Value = (Soldiers, Soldiers)> {
    (1usize..90)
        .prop_flat_map(|n| {
            (
                proptest::collection::btree_set(0u32..5000, n),
                proptest::collection::vec((-12i32..12, -12i32..12), n),
            )
        })
        .prop_flat_map(|(ids, pts)| {
            let v: Soldiers = ids
                .into_iter()
                .zip(pts)
                .map(|(i, (x, y))| (id(i), Vec2Fx::new(Fx::from_int(x), Fx::from_int(y))))
                .collect();
            let again = v.clone();
            Just(v).prop_shuffle().prop_map(move |s| (again.clone(), s))
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn assignment_is_a_permutation_independent_of_input_order_and_monotone_in_u(
        (a, b) in soldiers(),
        f in formation_strategy(),
        deg in heading_strategy(),
        ax in -20i32..20,
        ay in -20i32..20,
    ) {
        let t = table();
        let heading = Angle::from_degrees_int(deg);
        let anchor = Vec2Fx::new(Fx::from_int(ax), Fx::from_int(ay));
        let layout = slot_layout(f, a.len(), &t);
        let ra = assign_slots(&a, &layout, anchor, heading);
        let rb = assign_slots(&b, &layout, anchor, heading);
        prop_assert_eq!(&ra, &rb);

        // Permutation: every soldier once, every slot once.
        prop_assert_eq!(ra.len(), a.len());
        let mut got_ids: Vec<AgentId> = ra.iter().map(|g| g.0).collect();
        got_ids.sort();
        let mut want_ids: Vec<AgentId> = a.iter().map(|g| g.0).collect();
        want_ids.sort();
        prop_assert_eq!(got_ids, want_ids);
        let slots: Vec<usize> = ra.iter().map(|g| g.1).collect();
        prop_assert_eq!(slots, (0..a.len()).collect::<Vec<_>>());

        // Rank monotonic in u, and ranks ordered front to back by v.
        let pos = |who: AgentId| a.iter().find(|s| s.0 == who).map(|s| project(s.1, anchor, heading));
        for (start, len) in ranks(&layout) {
            let group: Vec<(i64, i64)> = ra[start..start + len].iter().map(|g| pos(g.0).unwrap()).collect();
            for w in group.windows(2) {
                prop_assert!(w[0].0 <= w[1].0, "u decreases inside a rank");
            }
        }
        let rs = ranks(&layout);
        for pair in rs.windows(2) {
            let (s0, l0) = pair[0];
            let (s1, l1) = pair[1];
            let back_max = ra[s1..s1 + l1].iter().map(|g| pos(g.0).unwrap().1).max().unwrap();
            let front_min = ra[s0..s0 + l0].iter().map(|g| pos(g.0).unwrap().1).min().unwrap();
            prop_assert!(front_min >= back_max, "a rear soldier is ahead of a front one");
        }
    }

    #[test]
    fn layouts_are_canonical_for_any_size(f in formation_strategy(), n in 0usize..700) {
        let l = slot_layout(f, n, &table());
        prop_assert_eq!(l.len(), n);
        assert_canonical(&l);
    }
}
