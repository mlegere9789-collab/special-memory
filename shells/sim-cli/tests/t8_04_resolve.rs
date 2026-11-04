//! T8-04 (SPEC-008 section 8, SPEC-003 amendment A2): `resolve` writes `fidelity_class == fc`
//! and the tag-0x20 input; `verify` exits 2 on a different class or hash; a source scan of the
//! new modules finds no wall clock, thread-count query or floating-point type.

mod common;

use battle_rules::{scenario_hash, Scenario, Tables};
use common::{checkpoint_hash_offset, cli, fix_body_hash, process, tmp_path};
use sim_state::{Replay, ReplayReader, ReplayWriter};
use std::path::{Path, PathBuf};

const REPO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
const PROV: &str =
    "confidence = \"partial\"\nsources = [{ document = \"CWV\", page = 80, note = \"test\" }]\n";

/// A scenario on a 160 m by 80 m map: two regiments that fight, one objective, fidelity class `fc`.
fn scenario_text(id: &str, fc: u8) -> String {
    let unit = |id: &str, side: &str, x: i64, heading: u16| {
        format!(
            "[[unit]]\nid = \"{id}\"\nside = \"{side}\"\nparent = \"\"\narm = \"infantry\"\nmen = 120\nguns = 0\n\
             training = \"trained\"\nweapon = \"rifle_58\"\nrounds_per_man = 40\ncommander = \"\"\n{PROV}\n\
             [[arrival]]\nunit = \"{id}\"\nat_s = 0\nx_mm = {x}\ny_mm = 40000\nheading_deg = {heading}\n\
             formation = \"line\"\nlow_s = 0\nhigh_s = 0\n{PROV}\n"
        )
    };
    format!(
        "schema = \"scenario/1\"\nid = \"{id}\"\ntitle = \"Test\"\ndate = \"1863-07-01\"\nclock_start_s = 26400\n\
         duration_s = 30\ntick_hz = 20\nfidelity_class = {fc}\nf0_cap = 15000\nagent_cap = 35000\n\n\
         [map]\nwidth_mm = 160000\nheight_mm = 80000\norigin_x_mm = 0\norigin_y_mm = 0\n\n\
         [terrain]\ncell_mm = 40000\nlegend = [{{ ch = \".\", class = 0 }}]\nrows = \"\"\"\n....\n....\n\"\"\"\n\n\
         {}{}\
         [[trigger]]\nid = \"fire\"\nat_s = 1\nkind = \"fire_mode\"\nunit = \"blue\"\ntarget = \"grey\"\n{PROV}\n\
         [[objective]]\nid = \"ridge\"\nside = \"union\"\nx_mm = 50000\ny_mm = 40000\nradius_mm = 20000\npoints = 1\n{PROV}\n\
         [[weather]]\nfrom_s = 0\nwind_x_mm_s = 0\nwind_y_mm_s = 0\nwet_pm = 0\nground_pm = 0\nvisibility_m = 5000\n\
         confidence = \"unverified\"\nplaceholder = true\ncalibration_target = \"CT-008-88\"\n\
         sources = [{{ document = \"CWV\", page = 79, note = \"test\" }}]\n",
        unit("blue", "union", 50000, 0),
        unit("grey", "confederate", 110000, 180),
    )
}

/// A data directory holding the repository rules and the given scenarios.
fn data_dir(tag: &str, scenarios: &[(&str, String)]) -> PathBuf {
    let dir = tmp_path(tag).with_extension("");
    std::fs::create_dir_all(dir.join("rules")).unwrap();
    std::fs::create_dir_all(dir.join("scenarios")).unwrap();
    for e in std::fs::read_dir(Path::new(REPO).join("data/rules")).unwrap() {
        let e = e.unwrap();
        if e.path().extension().is_some_and(|x| x == "toml") {
            std::fs::copy(e.path(), dir.join("rules").join(e.file_name())).unwrap();
        }
    }
    for (id, text) in scenarios {
        std::fs::write(dir.join("scenarios").join(format!("{id}.toml")), text).unwrap();
    }
    dir
}

fn s(p: &Path) -> &str {
    p.to_str().unwrap()
}

fn resolve_to(dir: &Path, id: &str, seed: &str, ticks: &str) -> (PathBuf, String) {
    let out = tmp_path("resolve");
    let (code, stdout, stderr) = cli(&[
        "resolve",
        "--scenario",
        id,
        "--seed",
        seed,
        "--ticks",
        ticks,
        "--out",
        s(&out),
        "--data-dir",
        s(dir),
    ]);
    assert_eq!(code, 0, "{stderr}");
    (out, stdout)
}

fn read(path: &Path) -> Replay {
    ReplayReader::parse(&std::fs::read(path).unwrap()).unwrap()
}

fn verify(dir: &Path, file: &Path) -> (i32, String, String) {
    cli(&["verify", s(file), "--data-dir", s(dir)])
}

/// The replay with `class` in its header and `hash` in its binding, same checkpoints.
fn rebuilt(r: &Replay, class: u8, hash: u64) -> PathBuf {
    let mut w = ReplayWriter::new(
        r.scenario_id(),
        r.seed(),
        r.tick_hz(),
        class,
        r.checkpoint_interval(),
    );
    let mut payload = vec![0x20u8];
    payload.extend_from_slice(&hash.to_le_bytes());
    w.push_input(0, 0xFF, 0, payload);
    for c in r.checkpoints() {
        w.push_checkpoint(c.tick, c.hash);
    }
    let path = tmp_path("rebuilt");
    std::fs::write(
        &path,
        w.finish(r.total_ticks(), r.final_hash())
            .unwrap()
            .to_bytes(),
    )
    .unwrap();
    path
}

#[test]
fn resolve_writes_the_class_and_the_binding_input() {
    for fc in [0u8, 2] {
        let id = format!("pair{fc}");
        let text = scenario_text(&id, fc);
        let dir = data_dir("bind", &[(&id, text.clone())]);
        let (path, stdout) = resolve_to(&dir, &id, "3", "250");
        let r = read(&path);
        assert_eq!(r.scenario_id(), id);
        assert_eq!((r.seed(), r.tick_hz(), r.total_ticks()), (3, 20, 250));
        assert_eq!(r.fidelity_class(), fc);
        assert_eq!(r.checkpoints().len(), 2);
        let [input] = r.inputs() else {
            panic!("one input")
        };
        assert_eq!((input.tick, input.actor, input.seq), (0, 0xFF, 0));
        let scenario = Scenario::from_toml_str(
            &text,
            &Tables::load_dir(s(&dir.join("rules"))).unwrap(),
            None,
        )
        .unwrap();
        let mut want = vec![0x20u8];
        want.extend_from_slice(&scenario_hash(&scenario).to_le_bytes());
        assert_eq!(input.payload, want);
        assert!(
            stdout.contains(&format!("fidelity_class={fc}\n")),
            "{stdout}"
        );
        assert!(stdout.contains(&format!("final_hash={:016x}\n", r.final_hash())));
        assert!(
            stdout.contains("ledger.union present=120 killed=0 arrived=120"),
            "{stdout}"
        );
        assert!(stdout.contains("casualties.confederate killed="));
        assert!(
            stdout.contains(&format!("objective.ridge={}\n", "union")),
            "{stdout}"
        );
        // The replay verifies.
        let (code, out, err) = verify(&dir, &path);
        assert_eq!(code, 0, "{out}{err}");
    }
}

#[test]
fn verify_exits_2_on_a_different_class_or_hash() {
    let dir = data_dir("refuse", &[("pair0", scenario_text("pair0", 0))]);
    let (path, _) = resolve_to(&dir, "pair0", "1", "200");
    let r = read(&path);
    let good: u64 = u64::from_le_bytes(r.inputs()[0].payload[1..9].try_into().unwrap());
    assert_eq!(verify(&dir, &rebuilt(&r, 0, good)).0, 0);
    for class in [1u8, 2] {
        let (code, _, err) = verify(&dir, &rebuilt(&r, class, good));
        assert_eq!(code, 2, "{err}");
        assert!(err.contains("class"), "{err}");
    }
    let (code, _, err) = verify(&dir, &rebuilt(&r, 0, good ^ 1));
    assert_eq!(code, 2, "{err}");
    assert!(err.contains("hash"), "{err}");
}

#[test]
fn verify_exits_1_on_a_changed_checkpoint_and_2_on_a_malformed_binding() {
    let dir = data_dir("tamper", &[("pair0", scenario_text("pair0", 0))]);
    let (path, _) = resolve_to(&dir, "pair0", "2", "200");
    let original = std::fs::read(&path).unwrap();
    let mut bytes = original.clone();
    let at = checkpoint_hash_offset(bytes.len(), 2, 1) + 2;
    bytes[at] ^= 0x04;
    fix_body_hash(&mut bytes);
    std::fs::write(&path, &bytes).unwrap();
    let (code, out, err) = verify(&dir, &path);
    assert_eq!(code, 1, "{out}{err}");
    assert!(err.contains("tick 200"), "{err}");
    // A replay without the binding, with another actor, or with an extra input is malformed.
    let r = ReplayReader::parse(&original).unwrap();
    let mut w = ReplayWriter::new(r.scenario_id(), r.seed(), 20, 0, 100);
    w.push_input(0, 0xFE, 0, r.inputs()[0].payload.clone());
    let none = tmp_path("nobind");
    std::fs::write(&none, w.finish(0, 0).unwrap().to_bytes()).unwrap();
    assert_eq!(verify(&dir, &none).0, 2);
    let mut w = ReplayWriter::new(r.scenario_id(), r.seed(), 20, 0, 100);
    w.push_input(0, 0xFF, 0, r.inputs()[0].payload.clone());
    w.push_input(5, 0xFF, 1, vec![1]);
    let extra = tmp_path("extra");
    std::fs::write(&extra, w.finish(0, 0).unwrap().to_bytes()).unwrap();
    assert_eq!(verify(&dir, &extra).0, 2);
    // An unknown scenario id, and one that is not an id at all, never read a file.
    let mut w = ReplayWriter::new("../rules/ammo", 1, 20, 0, 100);
    w.push_input(0, 0xFF, 0, r.inputs()[0].payload.clone());
    let odd = tmp_path("odd");
    std::fs::write(&odd, w.finish(0, 0).unwrap().to_bytes()).unwrap();
    assert_eq!(verify(&dir, &odd).0, 2);
}

#[test]
fn resolve_refuses_bad_arguments() {
    let dir = data_dir("args", &[("pair0", scenario_text("pair0", 0))]);
    let d = s(&dir).to_string();
    for args in [
        vec!["resolve", "--seed", "1", "--data-dir", &d],
        vec!["resolve", "--scenario", "pair0", "--data-dir", &d],
        vec![
            "resolve",
            "--scenario",
            "nope",
            "--seed",
            "1",
            "--data-dir",
            &d,
        ],
        vec![
            "resolve",
            "--scenario",
            "../x",
            "--seed",
            "1",
            "--data-dir",
            &d,
        ],
        vec![
            "resolve",
            "--scenario",
            "pair0",
            "--seed",
            "1",
            "--ticks",
            "10000001",
            "--data-dir",
            &d,
        ],
        vec![
            "resolve",
            "--scenario",
            "pair0",
            "--seed",
            "1",
            "--bogus",
            "1",
            "--data-dir",
            &d,
        ],
    ] {
        let (code, _, err) = cli(&args);
        assert_eq!(code, 2, "{args:?}: {err}");
    }
}

#[test]
fn a_second_process_prints_the_same_report() {
    let dir = data_dir("proc", &[("pair0", scenario_text("pair0", 0))]);
    let (_, here) = resolve_to(&dir, "pair0", "9", "300");
    let (code, there, err) = process(&[
        "resolve",
        "--scenario",
        "pair0",
        "--seed",
        "9",
        "--ticks",
        "300",
        "--data-dir",
        s(&dir),
    ]);
    assert_eq!(code, 0, "{err}");
    assert_eq!(here, there);
    // The default length is the scenario's: 30 s.
    let (code, out, _) = cli(&[
        "resolve",
        "--scenario",
        "pair0",
        "--seed",
        "9",
        "--data-dir",
        s(&dir),
    ]);
    assert_eq!(code, 0);
    assert!(out.contains("ticks=600\n"), "{out}");
}

#[test]
fn fid_small_resolves_from_the_repository_data() {
    let data = Path::new(REPO).join("data");
    let out = tmp_path("fid");
    let (code, stdout, err) = cli(&[
        "resolve",
        "--scenario",
        "fid-small",
        "--seed",
        "7",
        "--ticks",
        "100",
        "--out",
        s(&out),
        "--data-dir",
        s(&data),
    ]);
    assert_eq!(code, 0, "{err}");
    assert!(stdout.contains("fidelity_class=0\n"));
    assert_eq!(verify(&data, &out).0, 0);
}

#[test]
fn the_new_modules_use_no_clock_thread_query_or_float() {
    let root = Path::new(REPO);
    let mut files = vec![
        "shells/sim-cli/src/resolve.rs".to_string(),
        "shells/sim-cli/src/bound.rs".to_string(),
        "crates/battle-rules/src/runner.rs".to_string(),
        "crates/battle-rules/src/scenarios/fidelity.rs".to_string(),
    ];
    for e in std::fs::read_dir(root.join("crates/battle-rules/src/runner")).unwrap() {
        files.push(format!(
            "crates/battle-rules/src/runner/{}",
            e.unwrap().file_name().to_str().unwrap()
        ));
    }
    let banned = [
        "Instant",
        "SystemTime",
        "available_parallelism",
        "f32",
        "f64",
    ];
    for f in &files {
        let text = std::fs::read_to_string(root.join(f)).unwrap();
        for word in banned {
            assert!(!text.contains(word), "{f} mentions {word}");
        }
    }
}
