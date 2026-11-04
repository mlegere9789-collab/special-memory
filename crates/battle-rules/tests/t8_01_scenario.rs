//! T8-01 and the `scenario_hash` half of T8-03 (SPEC-008 sections 2.3, 3, 11): `fid-small`
//! loads, every rule V1 to V12 has a fixture returning its variant, and the hash ignores file
//! order and whitespace.

use battle_rules::{
    scenario_hash, Arm, Confidence, Scenario, ScenarioError, Tables, TriggerKind, TriggerOrder,
    WeaponTable,
};
use std::path::{Path, PathBuf};

const FID_SMALL: &str = include_str!("../../../data/scenarios/fid-small.toml");
const WEAPONS: &str = include_str!("../../../data/rules/weapons.toml");

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn tables() -> Tables {
    Tables::load_dir(root().join("data/rules").to_str().unwrap()).unwrap()
}

fn load(text: &str) -> Result<Scenario, ScenarioError> {
    Scenario::from_toml_str(text, &tables(), None)
}

fn fid_small() -> Scenario {
    load(FID_SMALL).unwrap()
}

/// The variant name of an error.
fn kind(e: &ScenarioError) -> &'static str {
    match e {
        ScenarioError::Data(_) => "Data",
        ScenarioError::Reference(_) => "Reference",
        ScenarioError::Duplicate(_) => "Duplicate",
        ScenarioError::Range(_) => "Range",
        ScenarioError::Time(_) => "Time",
        ScenarioError::Cycle(_) => "Cycle",
        ScenarioError::Strength(_) => "Strength",
        ScenarioError::Class(_) => "Class",
        ScenarioError::Terrain(_) => "Terrain",
        ScenarioError::Cap(_) => "Cap",
    }
}

fn expect(text: &str, want: &str) {
    match load(text) {
        Err(e) => assert_eq!(kind(&e), want, "{e}"),
        Ok(_) => panic!("expected {want}, but the scenario loaded"),
    }
}

fn replace(from: &str, to: &str) -> String {
    assert!(FID_SMALL.contains(from), "fid-small lacks {from:?}");
    FID_SMALL.replacen(from, to, 1)
}

#[test]
fn fid_small_loads() {
    let s = fid_small();
    assert_eq!(
        (s.id.as_str(), s.family.as_str()),
        ("fid-small", "fid-small")
    );
    assert_eq!(
        (s.tick_hz, s.fidelity_class, s.f0_cap, s.agent_cap),
        (20, 0, 15000, 35000)
    );
    assert_eq!(
        (
            s.units.len(),
            s.arrivals.len(),
            s.triggers.len(),
            s.weather.len()
        ),
        (2, 2, 1, 2)
    );
    let b = s.unit("buford_div").unwrap();
    assert_eq!(
        (b.men, b.arm, b.regiment.map(|r| r.0)),
        (2700, Arm::Cavalry, Some(0))
    );
    assert_eq!(b.provenance.confidence, Confidence::Partial);
    assert_eq!(
        (
            b.provenance.sources[0].document.as_str(),
            b.provenance.sources[0].page
        ),
        ("CWV", 80)
    );
    assert_eq!(s.unit("heth_div").unwrap().regiment.map(|r| r.0), Some(1));
    assert_eq!(s.triggers[0].kind, TriggerKind::Order(TriggerOrder::Hold));
    assert_eq!(
        (s.arrivals[1].unit.as_str(), s.arrivals[1].at_s),
        ("heth_div", 600)
    );
    s.check_weapons(&WeaponTable::from_toml_str(WEAPONS).unwrap())
        .unwrap();
}

#[test]
fn every_rule_has_a_fixture() {
    let dir = root().join("qc/fixtures/scenario-bad");
    let want = [
        ("v01", "Data"),
        ("v02", "Duplicate"),
        ("v03", "Reference"),
        ("v04", "Cycle"),
        ("v05", "Time"),
        ("v06", "Range"),
        ("v07", "Strength"),
        ("v08", "Class"),
        ("v09", "Range"),
        ("v10", "Terrain"),
        ("v11", "Range"),
        ("v12", "Data"),
    ];
    let mut seen = 0;
    for (prefix, variant) in want {
        let mut files: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| {
                let n = p.file_name().unwrap().to_str().unwrap();
                n.starts_with(prefix) && n.ends_with(".toml")
            })
            .collect();
        files.sort();
        assert!(!files.is_empty(), "no fixture for {prefix}");
        for path in files {
            let text = std::fs::read_to_string(&path).unwrap();
            let rows = Scenario::terrain_source(&text)
                .unwrap_or(None)
                .map(|s| std::fs::read_to_string(dir.join(s)).unwrap());
            let got = Scenario::from_toml_str(&text, &tables(), rows.as_deref());
            match got {
                Err(e) => assert_eq!(kind(&e), variant, "{}: {e}", path.display()),
                Ok(_) => panic!("{} loaded", path.display()),
            }
            seen += 1;
        }
    }
    assert!(seen >= 12);
}

#[test]
fn v1_unknown_key_and_provenance() {
    expect(
        &replace("agent_cap = 35000", "agent_cap = 35000\nflavour = 1"),
        "Data",
    );
    expect(&replace("[map]", "extra = 1\n\n[map]"), "Data");
    expect(&replace("heading_deg = 180", "heading_deg = 180.5"), "Data");
    let no_sources = replace(
        "confidence = \"partial\"\nsources = [{ document = \"CWV\", page = 80, note = \"holds until relieved\" }]",
        "confidence = \"partial\"",
    );
    expect(&no_sources, "Data");
}

#[test]
fn v2_to_v5_variants() {
    expect(
        &replace("id = \"heth_div\"", "id = \"buford_div\""),
        "Duplicate",
    );
    expect(
        &replace(
            "id = \"buford_hold\"\nat_s = 1200",
            "id = \"buford_hold\"\nat_s = 99999",
        ),
        "Time",
    );
    expect(&replace("high_s = 2400", "high_s = 500"), "Time");
    expect(&replace("low_s = 600", "low_s = 700"), "Time");
    expect(
        &replace(
            "unit = \"heth_div\"\nat_s = 600",
            "unit = \"nobody\"\nat_s = 600",
        ),
        "Reference",
    );
    expect(
        &replace(
            "parent = \"\"\narm = \"infantry\"",
            "parent = \"buford_div\"\narm = \"infantry\"",
        ),
        "Cycle",
    );
    expect(&replace("from_s = 0\n", "from_s = 5\n"), "Time");
    let departs = format!("{FID_SMALL}\n[[departure]]\nunit = \"buford_div\"\nat_s = 0\nconfidence = \"partial\"\nsources = [{{ document = \"CWV\", page = 80 }}]\n");
    expect(&departs, "Time");
    let departs = departs.replace("at_s = 0\nconfidence", "at_s = 2000\nconfidence");
    let s = load(&departs).unwrap();
    assert_eq!(s.departures.len(), 1);
}

#[test]
fn v6_positions() {
    expect(&replace("x_mm = 2000000", "x_mm = 4000000"), "Range");
    expect(
        &replace(
            "y_mm = 1500000\nheading_deg = 180",
            "y_mm = -1\nheading_deg = 180",
        ),
        "Range",
    );
    // Row 0 is the southern row; woods become deep water (class 7, blocking) at cell (3, 1).
    let wet = replace("{ ch = \"W\", class = 3 }", "{ ch = \"W\", class = 7 }");
    load(&wet).unwrap();
    expect(
        &wet.replace(
            "x_mm = 200000\ny_mm = 1500000",
            "x_mm = 1750000\ny_mm = 750000",
        ),
        "Range",
    );
}

#[test]
fn v7_to_v9_and_v11() {
    expect(
        &replace("men = 2700\ntol_men", "men = 2000\ntol_men"),
        "Strength",
    );
    expect(
        &replace("fidelity_class = 0", "fidelity_class = 3"),
        "Class",
    );
    expect(&replace("f0_cap = 15000", "f0_cap = 40000"), "Cap");
    expect(&replace("agent_cap = 35000", "agent_cap = 200001"), "Cap");
    expect(&replace("tick_hz = 20", "tick_hz = 25"), "Range");
    expect(&replace("men = 3000", "men = 1000001"), "Range");
}

#[test]
fn v11_unit_count_is_checked_before_rows_are_read() {
    let mut text = String::from(&FID_SMALL[..FID_SMALL.find("[[unit]]").unwrap()]);
    for i in 0..4097 {
        text.push_str(&format!(
            "[[unit]]\nid = \"h{i}\"\nconfidence = \"nonsense\"\n"
        ));
    }
    match load(&text) {
        Err(ScenarioError::Range(m)) => assert!(m.contains("4097"), "{m}"),
        other => panic!("expected Range, got {other:?}"),
    }
}

#[test]
fn v12_verified_needs_second_source() {
    let bad = replace("confidence = \"partial\"\nsources = [{ document = \"CWV\", page = 80, note = \"about 2,700 troopers\" }]\n\n[[unit]]", "confidence = \"verified\"\nsources = [{ document = \"CWV\", page = 80 }]\n\n[[unit]]");
    assert!(matches!(
        load(&bad),
        Err(ScenarioError::Data(battle_rules::DataError::Provenance(_)))
    ));
    let good = bad.replace(
        "confidence = \"verified\"",
        "confidence = \"verified\"\nsecond_source = \"order of battle audit\"",
    );
    load(&good).unwrap();
}

#[test]
fn v10_terrain_source_file() {
    let rows = ".W\n..\n";
    let base = replace(
        "cell_mm = 500000",
        "cell_mm = 500000\nsource = \"terrain/pike.rows\"\nsource_hash = \"HASH\"",
    );
    // Cut the inline rows, which a source replaces.
    let a = base.find("rows = \"\"\"").unwrap();
    let b = base[a + 10..].find("\"\"\"").unwrap() + a + 13;
    let base = format!("{}{}", &base[..a], &base[b..]);
    assert_eq!(
        Scenario::terrain_source(&base).unwrap().as_deref(),
        Some("terrain/pike.rows")
    );
    let want = battle_rules_fnv(rows);
    // The same literal is asserted by tools/test_scenario_validator.py.
    assert_eq!(want, 0xD51F_0FCC_896B_EF1A);
    let text = base.replace("HASH", &format!("{want:016x}"));
    let err = Scenario::from_toml_str(&text, &tables(), None).unwrap_err();
    assert_eq!(kind(&err), "Terrain");
    let wide = ".W..\n....\n";
    let err = Scenario::from_toml_str(&text, &tables(), Some(wide)).unwrap_err();
    assert_eq!(kind(&err), "Terrain");
    let ok = Scenario::from_toml_str(&text, &tables(), Some(rows)).unwrap();
    assert_eq!(ok.terrain.grid.width(), 2);
    // CRLF line ends and a missing final newline hash the same rows.
    Scenario::from_toml_str(&text, &tables(), Some(".W\r\n..")).unwrap();
}

/// FNV-1a 64 of rows each followed by a newline (the canonical form of SPEC-008 section 3).
fn battle_rules_fnv(rows: &str) -> u64 {
    let mut bytes = Vec::new();
    for line in rows.lines() {
        bytes.extend_from_slice(line.as_bytes());
        bytes.push(b'\n');
    }
    sim_state::fnv1a64(&bytes)
}

/// Splits a document into the blocks that start at a `[` line.
fn blocks(text: &str) -> Vec<String> {
    let mut out: Vec<String> = vec![String::new()];
    for line in text.lines() {
        if line.starts_with('[') {
            out.push(String::new());
        }
        let last = out.last_mut().unwrap();
        last.push_str(line);
        last.push('\n');
    }
    out
}

#[test]
fn hash_ignores_row_order_and_whitespace() {
    let base = scenario_hash(&fid_small());
    // Reverse every repeated table, keeping the header and the single tables first.
    let parts = blocks(FID_SMALL);
    let (single, repeated): (Vec<_>, Vec<_>) =
        parts.into_iter().partition(|b| !b.starts_with("[["));
    let reversed: String = single
        .iter()
        .cloned()
        .chain(repeated.iter().rev().cloned())
        .collect();
    assert_ne!(reversed, FID_SMALL);
    assert_eq!(scenario_hash(&load(&reversed).unwrap()), base);
    let spaced = FID_SMALL.replace("\n\n", "\n\n\n").replace(" = ", "  =  ");
    assert_eq!(scenario_hash(&load(&spaced).unwrap()), base);
    // Prose is not hashed; a number is.
    assert_eq!(
        scenario_hash(&load(&replace("title = \"Pike contact\"", "title = \"Other\"")).unwrap()),
        base
    );
    for edit in [
        ("men = 3000", "men = 3001"),
        ("at_s = 600", "at_s = 601"),
        ("visibility_m = 4000", "visibility_m = 4001"),
        ("rounds_per_man = 40", "rounds_per_man = 41"),
    ] {
        let h = scenario_hash(&load(&replace(edit.0, edit.1)).unwrap());
        assert_ne!(h, base, "{edit:?}");
    }
}

#[test]
fn hash_literal_is_pinned() {
    assert_eq!(scenario_hash(&fid_small()), 15_110_017_277_725_047_471);
}

#[test]
fn headquarters_form_the_hierarchy_and_take_no_regiment_id() {
    let hq = "[[unit]]\nid = \"a_corps\"\nside = \"confederate\"\nparent = \"\"\narm = \"hq\"\nmen = 0\nguns = 0\ntraining = \"trained\"\nweapon = \"\"\nrounds_per_man = 0\ncommander = \"ap_hill\"\nconfidence = \"partial\"\nsources = [{ document = \"CWV\", page = 80 }]\n\n";
    let with_hq = replace(
        "[[unit]]\nid = \"heth_div\"",
        &format!("{hq}[[unit]]\nid = \"heth_div\""),
    )
    .replace(
        "id = \"heth_div\"\nside = \"confederate\"\nparent = \"\"",
        "id = \"heth_div\"\nside = \"confederate\"\nparent = \"a_corps\"",
    );
    let s = load(&with_hq).unwrap();
    assert_eq!(
        s.units.iter().map(|u| u.id.as_str()).collect::<Vec<_>>(),
        ["a_corps", "buford_div", "heth_div"]
    );
    assert_eq!(
        s.units
            .iter()
            .map(|u| u.regiment.map(|r| r.0))
            .collect::<Vec<_>>(),
        [None, Some(0), Some(1)]
    );
    // A headquarters cannot arrive, and a parent on the other side is refused.
    let arrives = format!("{with_hq}\n[[arrival]]\nunit = \"a_corps\"\nat_s = 0\nx_mm = 100\ny_mm = 100\nheading_deg = 0\nformation = \"line\"\nlow_s = 0\nhigh_s = 0\nconfidence = \"partial\"\nsources = [{{ document = \"CWV\", page = 80 }}]\n");
    expect(&arrives, "Reference");
    expect(
        &with_hq.replace("parent = \"a_corps\"", "parent = \"buford_div\""),
        "Cycle",
    );
}
