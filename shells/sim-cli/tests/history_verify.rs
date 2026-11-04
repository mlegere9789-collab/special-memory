//! `verify` on replay-mode headers (`<scenario>/<variant>`): scripted inputs from actor 0xFD,
//! checkpoints and final hash are the history hash (SPEC-011 section 4.2, T11-03).

mod common;

use battle_rules::Tables;
use common::{cli, tmp_path};
use history_cal::{
    apply_variant, compile_script, stage_battle, units_from_toml_str, ReplayDriver, ReplayScript,
    Variant,
};
use sim_state::ReplayWriter;

const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
const TOY: &str = "data/scenarios/toy_replay";

fn read(rel: &str) -> String {
    std::fs::read_to_string(format!("{ROOT}/{rel}")).unwrap()
}

fn toy_bytes(variant: Variant, ticks: u64) -> Vec<u8> {
    let script = apply_variant(
        &ReplayScript::from_toml_str(&read(&format!("{TOY}/script.toml"))).unwrap(),
        variant,
    );
    let units = units_from_toml_str(&read(&format!("{TOY}/units.toml"))).unwrap();
    let tables = Tables::load_dir(&format!("{ROOT}/data/rules")).unwrap();
    let battle = stage_battle(&units, &script, &tables, 3).unwrap();
    let inputs = compile_script(&script, &units).unwrap();
    let driver = ReplayDriver::new(battle, inputs, &script).unwrap();
    let id = format!("toy_replay/{}", variant.name());
    driver.record(&id, 3, 0, ticks, 100).unwrap().to_bytes()
}

fn verify(bytes: &[u8]) -> (i32, String) {
    let path = tmp_path("history");
    std::fs::write(&path, bytes).unwrap();
    let (code, out, err) = cli(&["verify", path.to_str().unwrap(), "--root", ROOT]);
    let _ = std::fs::remove_file(path);
    (code, format!("{out}{err}"))
}

#[test]
fn every_variant_of_the_toy_replay_verifies() {
    for v in [Variant::Nominal, Variant::Early, Variant::Late] {
        let (code, text) = verify(&toy_bytes(v, 200));
        assert_eq!(code, 0, "{text}");
        assert!(text.contains("OK"), "{text}");
    }
}

#[test]
fn a_dropped_trigger_input_is_a_mismatch() {
    let replay = sim_state::ReplayReader::parse(&toy_bytes(Variant::Nominal, 200)).unwrap();
    let mut w = ReplayWriter::new(
        replay.scenario_id(),
        replay.seed(),
        replay.tick_hz(),
        replay.fidelity_class(),
        replay.checkpoint_interval(),
    );
    for i in replay.inputs().iter().filter(|i| i.payload[0] != 0x31) {
        w.push_input(i.tick, i.actor, i.seq, i.payload.clone());
    }
    for c in replay.checkpoints() {
        w.push_checkpoint(c.tick, c.hash);
    }
    let bytes = w
        .finish(replay.total_ticks(), replay.final_hash())
        .unwrap()
        .to_bytes();
    let (code, text) = verify(&bytes);
    assert_eq!(code, 1, "{text}");
    assert!(text.contains("MISMATCH"), "{text}");
}

#[test]
fn bad_bindings_are_malformed_not_mismatches() {
    let good = sim_state::ReplayReader::parse(&toy_bytes(Variant::Nominal, 200)).unwrap();
    for id in [
        "toy_replay/sideways",
        "no_such_scenario/nominal",
        "toy_replay/",
    ] {
        let mut w = ReplayWriter::new(id, 3, 20, 0, 100);
        w.push_checkpoint(100, 0).push_checkpoint(200, 0);
        let bytes = w.finish(good.total_ticks(), 0).unwrap().to_bytes();
        let (code, text) = verify(&bytes);
        assert_eq!(code, 2, "{id}: {text}");
    }
}

#[test]
fn a_missing_root_is_malformed() {
    let path = tmp_path("history");
    std::fs::write(&path, toy_bytes(Variant::Nominal, 200)).unwrap();
    let (code, _, _) = cli(&[
        "verify",
        path.to_str().unwrap(),
        "--root",
        "/nonexistent/root",
    ]);
    let _ = std::fs::remove_file(path);
    assert_eq!(code, 2);
}
