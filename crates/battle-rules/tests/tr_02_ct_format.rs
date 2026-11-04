//! TR-02: calibration target ids are `CT-<spec>-<n>` (REGISTRY section 7). The loader accepts the
//! full form, refuses a bare `CT-n` and every other deviation, and every shipped rules row carries
//! the spec prefix of its file.

use battle_rules::{
    parse_table, AmmoTable, DamageTable, DataError, DrillTable, LocomotionTable, TerrainTable,
    WeaponTable,
};

const LOCOMOTION: &str = include_str!("../../../data/rules/locomotion.toml");
const TERRAIN: &str = include_str!("../../../data/rules/terrain.toml");
const DRILL: &str = include_str!("../../../data/rules/drill.toml");
const AMMO: &str = include_str!("../../../data/rules/ammo.toml");
const WEAPONS: &str = include_str!("../../../data/rules/weapons.toml");
const DAMAGE: &str = include_str!("../../../data/rules/damage.toml");

/// Loads a one-row placeholder carrying `target` and returns `(spec, number)`.
fn load(target: &str) -> Result<(Option<u16>, Option<u32>), DataError> {
    let text = format!(
        "speed_mm_s = 1\nconfidence = \"unverified\"\nplaceholder = true\ncalibration_target = \"{target}\"\n"
    );
    let mut t = parse_table(&text)?;
    t.take_int("speed_mm_s")?;
    let p = t.take_provenance()?;
    t.finish()?;
    Ok((p.calibration_spec, p.calibration_target))
}

#[test]
fn full_form_is_accepted() {
    assert_eq!(load("CT-004-1").unwrap(), (Some(4), Some(1)));
    assert_eq!(load("CT-005-9").unwrap(), (Some(5), Some(9)));
    assert_eq!(load("CT-011-1101").unwrap(), (Some(11), Some(1101)));
    assert_eq!(load("CT-016-11").unwrap(), (Some(16), Some(11)));
    assert_eq!(load("CT-000-0").unwrap(), (Some(0), Some(0)));
}

#[test]
fn bare_and_malformed_ids_are_refused() {
    let refused = [
        "CT-1",
        "CT-12",
        "CT-4-1",
        "CT-04-1",
        "CT-0004-1",
        "CT-00a-1",
        "CT-a04-1",
        "CT-004-",
        "CT-004",
        "CT-004-1a",
        "CT-004--1",
        "CT-004-+1",
        "CT-004-1-2",
        "CT--004-1",
        "ct-004-1",
        "XX-004-1",
        "CT-004-99999999999",
        " CT-004-1",
        "CT-004-1 ",
        "CT-004-1\\n",
        "",
        "first",
    ];
    for id in refused {
        assert!(
            matches!(load(id), Err(DataError::Provenance(_))),
            "{id:?} must be refused"
        );
    }
}

#[test]
fn shipped_rows_carry_the_prefix_of_their_file() {
    let loco = LocomotionTable::from_toml_str(LOCOMOTION).unwrap();
    for g in loco.gaits() {
        assert_eq!(g.provenance.calibration_spec, Some(4), "{:?}", g.gait);
    }
    let terrain = TerrainTable::from_toml_str(TERRAIN).unwrap();
    for c in terrain.classes() {
        assert_eq!(c.provenance.calibration_spec, Some(4), "{}", c.id);
    }
    let drill = DrillTable::from_toml_str(DRILL).unwrap();
    assert_eq!(drill.globals.provenance.calibration_spec, Some(4));
    for r in &drill.training {
        assert_eq!(r.provenance.calibration_spec, Some(4));
    }
    for r in &drill.formations {
        assert_eq!(r.provenance.calibration_spec, Some(4));
    }
    for r in &drill.transitions {
        assert_eq!(r.provenance.calibration_spec, Some(4));
    }
    for r in AmmoTable::from_toml_str(AMMO).unwrap().rows() {
        assert_eq!(r.provenance.calibration_spec, Some(5), "{}", r.id);
    }
    for r in WeaponTable::from_toml_str(WEAPONS).unwrap().rows() {
        assert_eq!(r.provenance.calibration_spec, Some(5), "{}", r.id);
    }
    let damage = DamageTable::from_toml_str(DAMAGE).unwrap();
    assert_eq!(
        damage
            .posture(battle_rules::Posture::Prone)
            .provenance
            .calibration_spec,
        Some(5)
    );
}

#[test]
fn shipped_files_hold_no_bare_id() {
    for (name, text) in [
        ("locomotion", LOCOMOTION),
        ("terrain", TERRAIN),
        ("drill", DRILL),
        ("ammo", AMMO),
        ("weapons", WEAPONS),
        ("damage", DAMAGE),
    ] {
        let mut rest = text;
        while let Some(i) = rest.find("CT-") {
            let tail = &rest[i + 3..];
            let ok = tail.len() > 4
                && tail.as_bytes()[..3].iter().all(u8::is_ascii_digit)
                && tail.as_bytes()[3] == b'-'
                && tail.as_bytes()[4].is_ascii_digit();
            assert!(
                ok,
                "{name}: bare or malformed id near {:?}",
                &rest[i..i + 8.min(rest.len() - i)]
            );
            rest = tail;
        }
    }
}
