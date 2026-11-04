//! T4-01 for `locomotion.toml` and `terrain.toml`: the committed files load, and each broken
//! variant returns its named `DataError` (SPEC-004 sections 2 to 5).

use battle_rules::{Confidence, DataError, Gait, LocomotionTable, TerrainTable};

const LOCO: &str = include_str!("../../../data/rules/locomotion.toml");
const TERRAIN: &str = include_str!("../../../data/rules/terrain.toml");

fn loco(text: &str) -> Result<LocomotionTable, DataError> {
    LocomotionTable::from_toml_str(text)
}

fn terrain(text: &str) -> Result<TerrainTable, DataError> {
    TerrainTable::from_toml_str(text)
}

/// Replaces the first occurrence of `from`, panicking if the fixture text changed.
fn swap(text: &str, from: &str, to: &str) -> String {
    assert!(text.contains(from), "fixture lacks {from:?}");
    text.replacen(from, to, 1)
}

#[test]
fn committed_locomotion_loads_with_spec_values() {
    let t = loco(LOCO).unwrap();
    let want = [
        (Gait::Walk, 1067, 7),
        (Gait::Quick, 1303, 28),
        (Gait::DoubleQuick, 2305, 139),
        (Gait::Run, 3600, 1111),
    ];
    for (gait, speed, drain) in want {
        let row = t.gait(gait);
        assert_eq!(row.gait, gait);
        assert_eq!(row.speed_mm_s, speed);
        assert_eq!(row.drain_ppm_per_tick, drain);
        assert!(row.provenance.placeholder);
        assert_eq!(row.provenance.calibration_target, Some(1));
        assert_eq!(row.provenance.confidence, Confidence::Unverified);
    }
    let g = t.globals();
    assert_eq!(g.accel_mm_s2, 3000);
    assert_eq!(g.body_radius_mm, 250);
    assert_eq!(g.personal_space_mm, 500);
    assert_eq!(g.avoid_gain_mm_s, 1200);
    assert_eq!(g.max_lag_mm, 6000);
    assert_eq!(g.catchup_mm, 1500);
    assert_eq!(g.catchup_fatigue_limit_ppm, 600_000);
    assert_eq!(g.rest_recover_ppm_per_tick, 83);
    assert_eq!(g.rest_speed_mm_s, 100);
    assert_eq!(g.provenance.calibration_target, Some(3));
    let curve: Vec<(u32, u32)> = t
        .curve()
        .iter()
        .map(|p| (p.fatigue_ppm, p.speed_pm))
        .collect();
    assert_eq!(
        curve,
        [(0, 1000), (500_000, 900), (800_000, 650), (1_000_000, 400)]
    );
    assert!(t.curve().iter().all(|p| p.provenance.placeholder));
}

#[test]
fn committed_terrain_loads_with_spec_values() {
    let t = terrain(TERRAIN).unwrap();
    // (id, name, speed, effort, line_disorder, cover, blocks)
    let want = [
        (0, "open", 1000, 1000, 0, 0, false),
        (1, "road", 1050, 900, 0, 0, false),
        (2, "orchard", 850, 1100, 150, 150, false),
        (3, "woods", 600, 1400, 400, 300, false),
        (4, "thicket", 350, 1800, 600, 450, false),
        (5, "marsh", 450, 1800, 300, 0, false),
        (6, "ford", 500, 1500, 300, 0, false),
        (7, "deep_water", 0, 1000, 0, 0, true),
    ];
    assert_eq!(t.classes().len(), want.len());
    for (id, name, speed, effort, disorder, cover, blocks) in want {
        let c = t.class(id).unwrap();
        assert_eq!(c.id, id);
        assert_eq!(c.name, name);
        assert_eq!(c.speed_pm, speed);
        assert_eq!(c.effort_pm, effort);
        assert_eq!(c.line_disorder_pm, disorder);
        assert_eq!(c.cover_pm, cover);
        assert_eq!(c.blocks_movement, blocks);
        assert!(c.provenance.placeholder);
        assert_eq!(c.provenance.calibration_target, Some(4));
    }
    assert!(t.class(8).is_none());
    // The qualitative page citations are carried for woods.
    assert_eq!(t.class(3).unwrap().provenance.sources[0].page, 511);
}

#[test]
fn locomotion_float_unknown_key_and_provenance_cases() {
    let e = loco(&swap(LOCO, "speed_mm_s = 1067", "speed_mm_s = 1067.0")).unwrap_err();
    assert!(matches!(e, DataError::Float(p) if p == "gait[0].speed_mm_s"));
    let e = loco(&swap(LOCO, "accel_mm_s2 = 3000", "accel_mm_s2 = 3e3")).unwrap_err();
    assert!(matches!(e, DataError::Float(_)));

    let e = loco(&swap(
        LOCO,
        "speed_mm_s = 1067",
        "speed_mm_s = 1067\nspin = 1",
    ))
    .unwrap_err();
    assert_eq!(e, DataError::UnknownKey("gait[0].spin".to_string()));
    let e = loco(&swap(LOCO, "tick_hz = 20", "tick_hz = 20\nextra = 1")).unwrap_err();
    assert_eq!(e, DataError::UnknownKey("extra".to_string()));

    // Placeholder row without calibration_target.
    let e = loco(&swap(
        LOCO,
        "placeholder = true\ncalibration_target = \"CT-2\"",
        "placeholder = true",
    ))
    .unwrap_err();
    assert!(matches!(e, DataError::Provenance(_)), "{e:?}");

    // Non-placeholder row without sources.
    let e = loco(&swap(
        LOCO,
        "placeholder = true\ncalibration_target = \"CT-3\"",
        "placeholder = false",
    ))
    .unwrap_err();
    assert!(matches!(e, DataError::Provenance(_)), "{e:?}");
}

#[test]
fn terrain_float_unknown_key_and_provenance_cases() {
    let e = terrain(&swap(TERRAIN, "speed_pm = 1000", "speed_pm = 1000.5")).unwrap_err();
    assert!(matches!(e, DataError::Float(p) if p == "class[0].speed_pm"));
    let e = terrain(&swap(TERRAIN, "cover_pm = 0", "cover_pm = 0\nshade = 1")).unwrap_err();
    assert_eq!(e, DataError::UnknownKey("class[0].shade".to_string()));
    let e = terrain(&swap(
        TERRAIN,
        "placeholder = true\ncalibration_target = \"CT-4\"",
        "placeholder = true",
    ))
    .unwrap_err();
    assert!(matches!(e, DataError::Provenance(_)), "{e:?}");
    let e = terrain(&swap(
        TERRAIN,
        "placeholder = true\ncalibration_target = \"CT-4\"",
        "placeholder = false",
    ))
    .unwrap_err();
    assert!(matches!(e, DataError::Provenance(_)), "{e:?}");
}

#[test]
fn locomotion_structure_is_enforced() {
    // Wrong schema tag and tick rate.
    let e = loco(&swap(LOCO, "locomotion/1", "locomotion/2")).unwrap_err();
    assert!(matches!(e, DataError::Parse(_)), "{e:?}");
    let e = loco(&swap(LOCO, "tick_hz = 20", "tick_hz = 30")).unwrap_err();
    assert!(matches!(e, DataError::Range(_)), "{e:?}");
    // Duplicate gait id (so another is missing) and an unknown gait id.
    let e = loco(&swap(LOCO, "id = \"quick\"", "id = \"walk\"")).unwrap_err();
    assert!(matches!(e, DataError::Parse(_)), "{e:?}");
    let e = loco(&swap(LOCO, "id = \"run\"", "id = \"sprint\"")).unwrap_err();
    assert!(matches!(e, DataError::Parse(_)), "{e:?}");
    // Zero speed.
    let e = loco(&swap(LOCO, "speed_mm_s = 3600", "speed_mm_s = 0")).unwrap_err();
    assert!(matches!(e, DataError::Range(_)), "{e:?}");
    // Curve: must start at 0, end at 1_000_000, strictly increase, and never speed up.
    let e = loco(&swap(LOCO, "fatigue_ppm = 0\n", "fatigue_ppm = 1\n")).unwrap_err();
    assert!(matches!(e, DataError::Range(_)), "{e:?}");
    let e = loco(&swap(
        LOCO,
        "fatigue_ppm = 1000000\n",
        "fatigue_ppm = 900000\n",
    ))
    .unwrap_err();
    assert!(matches!(e, DataError::Range(_)), "{e:?}");
    let e = loco(&swap(
        LOCO,
        "fatigue_ppm = 800000\n",
        "fatigue_ppm = 500000\n",
    ))
    .unwrap_err();
    assert!(matches!(e, DataError::Range(_)), "{e:?}");
    let e = loco(&swap(LOCO, "speed_pm = 650", "speed_pm = 950")).unwrap_err();
    assert!(matches!(e, DataError::Range(_)), "{e:?}");
    let e = loco(&swap(LOCO, "speed_pm = 400", "speed_pm = 1001")).unwrap_err();
    assert!(matches!(e, DataError::Range(_)), "{e:?}");
}

#[test]
fn terrain_structure_is_enforced() {
    let e = terrain(&swap(TERRAIN, "terrain/1", "terrain/9")).unwrap_err();
    assert!(matches!(e, DataError::Parse(_)), "{e:?}");
    // Ids must be unique and ascending.
    let e = terrain(&swap(TERRAIN, "id = 1\n", "id = 0\n")).unwrap_err();
    assert!(matches!(e, DataError::Range(_)), "{e:?}");
    let e = terrain(&swap(TERRAIN, "id = 2\n", "id = 300\n")).unwrap_err();
    assert!(matches!(e, DataError::Range(_)), "{e:?}");
    // A passable class must have a positive speed.
    let e = terrain(&swap(TERRAIN, "speed_pm = 350", "speed_pm = 0")).unwrap_err();
    assert!(matches!(e, DataError::Range(_)), "{e:?}");
    // No classes at all.
    let e = terrain("schema = \"terrain/1\"\n").unwrap_err();
    assert!(matches!(e, DataError::Parse(_)), "{e:?}");
}
