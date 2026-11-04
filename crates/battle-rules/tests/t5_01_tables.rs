//! T5-01: the ammo, weapon and damage tables load; floats, unknown keys and provenance
//! violations are refused; step sums match (SPEC-005 sections 1 and 11).

use battle_rules::{
    AmmoKind, AmmoTable, BurstShape, Confidence, DamageTable, DataError, Posture, Region,
    WeaponClass, WeaponTable,
};

const AMMO: &str = include_str!("../../../data/rules/ammo.toml");
const WEAPONS: &str = include_str!("../../../data/rules/weapons.toml");
const DAMAGE: &str = include_str!("../../../data/rules/damage.toml");

/// Replaces the first occurrence of `from`, which must exist.
fn mutate(text: &str, from: &str, to: &str) -> String {
    assert!(text.contains(from), "fixture text lacks {from:?}");
    text.replacen(from, to, 1)
}

#[test]
fn ammo_rows_load_in_file_order() {
    let t = AmmoTable::from_toml_str(AMMO).unwrap();
    let ids: Vec<&str> = t.rows().iter().map(|r| r.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "ball_69",
            "minie_58",
            "carbine_52",
            "revolver_44",
            "shot_12",
            "shell_12",
            "case_12",
            "canister_12"
        ]
    );
    let kinds: Vec<AmmoKind> = t.rows().iter().map(|r| r.kind).collect();
    use AmmoKind::*;
    assert_eq!(
        kinds,
        [Ball, Conical, Conical, Ball, Shot, Shell, Case, Canister]
    );
    let minie = t.get("minie_58").unwrap();
    assert_eq!(
        (
            minie.mass_g,
            minie.diameter_mm,
            minie.cd_pm,
            minie.muzzle_mm_s
        ),
        (32, 15, 300, 290000)
    );
    assert_eq!(t.index_of("shot_12"), Some(4));
    assert!(t.get("nope").is_none());
    assert_eq!(t.get("shot_12").unwrap().restitution_pm, Some(400));
    assert_eq!(t.get("ball_69").unwrap().restitution_pm, None);
    assert!(t.get("ball_69").unwrap().burst.is_none());
}

#[test]
fn bursts_match_the_spec() {
    let t = AmmoTable::from_toml_str(AMMO).unwrap();
    let c = t.get("canister_12").unwrap().burst.as_ref().unwrap();
    assert_eq!(
        (c.count, c.pellet_diameter_mm, c.shape, c.spread_sigma_urad),
        (27, 38, BurstShape::Cone, 10000)
    );
    assert_eq!((c.pellet_speed_pm, c.fuse_sigma_substeps), (850, 0));
    assert_eq!(c.dud_ppm, 0);
    let k = t.get("case_12").unwrap().burst.as_ref().unwrap();
    assert_eq!(
        (k.count, k.pellet_diameter_mm, k.spread_sigma_urad),
        (76, 17, 25000)
    );
    assert_eq!((k.fuse_sigma_substeps, k.dud_ppm), (15, 30000));
    let s = t.get("shell_12").unwrap().burst.as_ref().unwrap();
    assert_eq!(
        (s.count, s.pellet_mass_g, s.shape),
        (40, 60, BurstShape::Sphere)
    );
    // Sphere fragments leave at 150 m/s: 375 permille of the 400 m/s muzzle speed.
    assert_eq!(400_000 * i64::from(s.pellet_speed_pm) / 1000, 150_000);
    assert_eq!((s.fuse_sigma_substeps, s.dud_ppm), (15, 30000));
}

#[test]
fn constants_load() {
    let c = AmmoTable::from_toml_str(AMMO).unwrap().constants().clone();
    assert_eq!(c.air_density_g_m3, 1225);
    assert_eq!(c.iron_density_kg_m3, 7200);
    assert_eq!(c.gravity, battle_rules::fx_ratio(980665, 100000));
}

#[test]
fn weapon_rows_and_step_sums() {
    let w = WeaponTable::from_toml_str(WEAPONS).unwrap();
    let want: [(&str, WeaponClass, u32); 5] = [
        ("smoothbore_69", WeaponClass::Smoothbore, 480),
        ("rifle_58", WeaponClass::Rifled, 500),
        ("carbine_52", WeaponClass::Carbine, 150),
        ("revolver_44", WeaponClass::Revolver, 1200),
        ("napoleon_12", WeaponClass::FieldGun, 1200),
    ];
    assert_eq!(w.rows().len(), want.len());
    for (row, (id, class, sum)) in w.rows().iter().zip(want) {
        assert_eq!(row.id, id);
        assert_eq!(row.class, class);
        assert_eq!(row.load_sum_ticks(), sum, "{id}");
        assert_eq!(row.reach_mm, 2 * row.extended_mm, "{id}");
        assert!(row.effective_mm < row.extended_mm);
    }
    let r = w.get("rifle_58").unwrap();
    let steps: Vec<(&str, u32)> = r
        .load_steps
        .iter()
        .map(|s| (s.id.as_str(), s.ticks))
        .collect();
    assert_eq!(
        steps,
        [
            ("handle", 120),
            ("seat", 60),
            ("ram", 160),
            ("prime", 100),
            ("ready", 60)
        ]
    );
    assert_eq!(r.misfire_restart_step, 3);
    assert_eq!(r.ram_step(), Some(2));
    assert_eq!((r.sigma0_urad, r.sigma_slope_urad_per_100m), (700, 5000));
    let rev = w.get("revolver_44").unwrap();
    assert_eq!((rev.magazine, rev.cycle_ticks), (6, 20));
    assert_eq!(rev.ram_step(), None);
    assert_eq!(rev.rounds_for(0), Some(18));
    let gun = w.get("napoleon_12").unwrap();
    let rounds: Vec<u16> = gun.ammo.iter().map(|a| a.rounds).collect();
    assert_eq!(rounds, [20, 20, 10, 6]);
    assert_eq!(gun.powder_mg, 907000);
    assert_eq!(w.index_of("revolver_44"), Some(3));
}

#[test]
fn every_weapon_ammo_id_exists_in_the_ammo_table() {
    let a = AmmoTable::from_toml_str(AMMO).unwrap();
    let w = WeaponTable::from_toml_str(WEAPONS).unwrap();
    w.check_ammo(&a).unwrap();
    let bad = WeaponTable::from_toml_str(&mutate(WEAPONS, "id = \"ball_69\"", "id = \"ball_99\""))
        .unwrap();
    assert!(matches!(bad.check_ammo(&a), Err(DataError::Parse(m)) if m.contains("ball_99")));
}

#[test]
fn damage_rows_load() {
    let d = DamageTable::from_toml_str(DAMAGE).unwrap();
    let p = d.posture(Posture::Standing);
    assert_eq!(
        (p.width_mm, p.height_mm, p.muzzle_height_mm, p.cover_pm),
        (500, 1700, 1450, 500)
    );
    let k = d.posture(Posture::Kneeling);
    assert_eq!(
        (k.height_mm, k.muzzle_height_mm, k.cover_pm),
        (1200, 1000, 800)
    );
    let g = d.posture(Posture::Prone);
    assert_eq!(
        (g.height_mm, g.muzzle_height_mm, g.cover_pm),
        (450, 300, 1000)
    );
    let gun = d.posture(Posture::Gun);
    assert_eq!((gun.width_mm, gun.height_mm), (1400, 1300));
    let cell = d.cell(2, Region::Torso);
    assert_eq!(
        (cell.killed_pm, cell.wounded_pm, cell.disabled_pm),
        (72, 240, 120)
    );
    let cell = d.cell(3, Region::Head);
    assert_eq!(
        (cell.killed_pm, cell.wounded_pm, cell.disabled_pm),
        (350, 300, 30)
    );
    for band in 0..4 {
        for region in [Region::Legs, Region::Torso, Region::Head] {
            let c = d.cell(band, region);
            assert!(c.killed_pm + c.wounded_pm + c.disabled_pm <= 1000);
        }
    }
    assert_eq!(d.absorb_pellet_j(), 600);
    assert_eq!(d.absorb_j(AmmoKind::Ball), 1100);
    assert_eq!(d.absorb_j(AmmoKind::Conical), 1100);
    assert_eq!(d.absorb_j(AmmoKind::Shot), 40000);
}

#[test]
fn every_row_carries_provenance() {
    let a = AmmoTable::from_toml_str(AMMO).unwrap();
    for r in a.rows() {
        let p = &r.provenance;
        assert_ne!(p.confidence, Confidence::Verified, "{}", r.id);
        assert!(p.placeholder && p.calibration_target.is_some(), "{}", r.id);
    }
    let shot = &a.get("shot_12").unwrap().provenance;
    assert_eq!(shot.calibration_target, Some(2));
    assert_eq!(shot.sources[0].page, 459);
    assert_eq!(shot.sources[0].tag.as_deref(), Some("V S19"));
    let w = WeaponTable::from_toml_str(WEAPONS).unwrap();
    for r in w.rows() {
        assert!(r.provenance.calibration_target.is_some(), "{}", r.id);
    }
    assert_eq!(
        w.get("rifle_58").unwrap().provenance.sources[0]
            .tag
            .as_deref(),
        Some("V S21")
    );
    let d = DamageTable::from_toml_str(DAMAGE).unwrap();
    assert_eq!(
        d.cell(0, Region::Legs).provenance.calibration_target,
        Some(6)
    );
    assert_eq!(
        d.posture(Posture::Prone).provenance.calibration_target,
        Some(5)
    );
}

/// Applies one corruption to each table and checks the matching error class.
#[test]
fn floats_are_refused_in_every_table() {
    let e = AmmoTable::from_toml_str(&mutate(AMMO, "mass_g = 27\n", "mass_g = 27.0\n"));
    assert_eq!(e.unwrap_err(), DataError::Float("ammo[0].mass_g".into()));
    let e = WeaponTable::from_toml_str(&mutate(
        WEAPONS,
        "sigma0_urad = 22000",
        "sigma0_urad = 2.2e4",
    ));
    assert_eq!(
        e.unwrap_err(),
        DataError::Float("weapon[0].sigma0_urad".into())
    );
    let e = DamageTable::from_toml_str(&mutate(DAMAGE, "cover_pm = 500", "cover_pm = 0.5"));
    assert_eq!(
        e.unwrap_err(),
        DataError::Float("posture[0].cover_pm".into())
    );
}

#[test]
fn unknown_keys_are_refused_in_every_table() {
    let e = AmmoTable::from_toml_str(&mutate(
        AMMO,
        "kind = \"ball\"\n",
        "kind = \"ball\"\nx = 1\n",
    ));
    assert_eq!(e.unwrap_err(), DataError::UnknownKey("ammo[0].x".into()));
    let e = AmmoTable::from_toml_str(&mutate(
        AMMO,
        "shape = \"cone\"\n",
        "shape = \"cone\"\nx = 1\n",
    ));
    assert!(matches!(e, Err(DataError::UnknownKey(p)) if p.ends_with("burst.x")));
    let e = WeaponTable::from_toml_str(&mutate(WEAPONS, "magazine = 1\n", "magazine = 1\nx = 1\n"));
    assert_eq!(e.unwrap_err(), DataError::UnknownKey("weapon[0].x".into()));
    let e = WeaponTable::from_toml_str(&mutate(
        WEAPONS,
        "{ id = \"handle\", ticks = 120 }",
        "{ id = \"handle\", ticks = 120, x = 1 }",
    ));
    assert!(matches!(e, Err(DataError::UnknownKey(p)) if p.ends_with(".x")));
    let e = DamageTable::from_toml_str(&mutate(DAMAGE, "[disabled]\n", "[disabled]\nx = 1\n"));
    assert_eq!(e.unwrap_err(), DataError::UnknownKey("disabled.x".into()));
    // A burst on a plain ball is not a known key either.
    let e = AmmoTable::from_toml_str(&mutate(
        AMMO,
        "[[ammo]]\nid = \"minie_58\"",
        "[ammo.burst]\ncount = 2\n\n[[ammo]]\nid = \"minie_58\"",
    ));
    assert!(e.is_err());
}

#[test]
fn provenance_violations_are_refused_in_every_table() {
    let no_target = |text: &str| mutate(text, "calibration_target = \"CT-005-1\"\n", "");
    assert!(matches!(
        AmmoTable::from_toml_str(&no_target(AMMO)),
        Err(DataError::Provenance(_))
    ));
    let e = WeaponTable::from_toml_str(&mutate(WEAPONS, "calibration_target = \"CT-005-3\"\n", ""));
    assert!(matches!(e, Err(DataError::Provenance(_))));
    let e = DamageTable::from_toml_str(&mutate(DAMAGE, "calibration_target = \"CT-005-5\"\n", ""));
    assert!(matches!(e, Err(DataError::Provenance(_))));
    // A sourced row with no placeholder flag and no sources, and a verified row without a second source.
    let e = AmmoTable::from_toml_str(&mutate(
        AMMO,
        "placeholder = true\n",
        "placeholder = false\n",
    ));
    assert!(matches!(e, Err(DataError::Provenance(_))));
    let e = WeaponTable::from_toml_str(&mutate(
        WEAPONS,
        "confidence = \"partial\"",
        "confidence = \"verified\"",
    ));
    assert!(matches!(e, Err(DataError::Provenance(_))));
    let e = DamageTable::from_toml_str(&mutate(DAMAGE, "confidence = \"unverified\"\n", ""));
    assert!(matches!(e, Err(DataError::Provenance(_))));
}

#[test]
fn structural_violations_are_refused() {
    let wrong_schema = |t: &str, a: &str, b: &str| mutate(t, a, b);
    assert!(matches!(
        AmmoTable::from_toml_str(&wrong_schema(AMMO, "ammo/1", "ammo/2")),
        Err(DataError::Parse(_))
    ));
    // Duplicate id.
    assert!(
        AmmoTable::from_toml_str(&mutate(AMMO, "id = \"minie_58\"", "id = \"ball_69\"")).is_err()
    );
    // A shot needs restitution, a shell needs a burst.
    assert!(AmmoTable::from_toml_str(&mutate(AMMO, "restitution_pm = 400\n", "")).is_err());
    assert!(
        AmmoTable::from_toml_str(&mutate(AMMO, "kind = \"ball\"", "kind = \"shell\"")).is_err()
    );
    assert!(
        AmmoTable::from_toml_str(&mutate(AMMO, "kind = \"ball\"", "kind = \"musket\"")).is_err()
    );
    // Range.
    assert!(matches!(
        AmmoTable::from_toml_str(&mutate(AMMO, "mass_g = 27\n", "mass_g = 0\n")),
        Err(DataError::Range(_))
    ));
    // Weapons: restart step must exist, ids unique, effective not beyond extended.
    assert!(WeaponTable::from_toml_str(&mutate(
        WEAPONS,
        "misfire_restart_step = \"prime\"",
        "misfire_restart_step = \"fire\""
    ))
    .is_err());
    assert!(WeaponTable::from_toml_str(&mutate(
        WEAPONS,
        "id = \"rifle_58\"",
        "id = \"smoothbore_69\""
    ))
    .is_err());
    assert!(WeaponTable::from_toml_str(&mutate(
        WEAPONS,
        "effective_mm = 45720",
        "effective_mm = 999999"
    ))
    .is_err());
    assert!(WeaponTable::from_toml_str(&mutate(
        WEAPONS,
        "class = \"smoothbore\"",
        "class = \"pike\""
    ))
    .is_err());
    // Damage: a missing or repeated outcome cell, a missing posture.
    assert!(DamageTable::from_toml_str(&mutate(
        DAMAGE,
        "band = 3\nregion = \"head\"",
        "band = 3\nregion = \"torso\""
    ))
    .is_err());
    assert!(
        DamageTable::from_toml_str(&mutate(DAMAGE, "id = \"prone\"", "id = \"kneeling\"")).is_err()
    );
}

#[test]
fn a_row_summing_above_1000_is_refused() {
    let bad = mutate(
        DAMAGE,
        "killed_pm = 350\nwounded_pm = 300\ndisabled_pm = 30\n",
        "killed_pm = 350\nwounded_pm = 300\ndisabled_pm = 351\n",
    );
    assert!(matches!(
        DamageTable::from_toml_str(&bad),
        Err(DataError::Range(m)) if m.contains("1001")
    ));
}
