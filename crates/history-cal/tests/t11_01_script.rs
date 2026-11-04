//! T11-01: script loader errors, exact ticks, ordering, per-tick limit, parent expansion (SPEC-011
//! sections 3, 4.1 and amendment A1).

use battle_rules::registry::{ACTOR_HISTORY_SCRIPT, TAG_TRIGGER};
use battle_rules::{decode_order, DataError, Formation, Gait, Order, RegimentId};
use core_math::{Angle, Fx};
use history_cal::{
    compile_script, units_from_toml_str, ReplayScript, ScriptError, ScriptInput, UnitRecord,
};

const TOY_SCRIPT: &str = include_str!("../../../data/scenarios/toy_replay/script.toml");
const TOY_UNITS: &str = include_str!("../../../data/scenarios/toy_replay/units.toml");
const PROV: &str =
    "confidence = \"unverified\"\nplaceholder = true\ncalibration_target = \"CT-011-1101\"\n";

fn units() -> Vec<UnitRecord> {
    units_from_toml_str(TOY_UNITS).unwrap()
}

/// A script with a given clock start and the rows in `rows`.
fn script(start: i64, rows: &str) -> String {
    format!(
        "schema = \"replay-script/1\"\n[meta]\nscenario = \"t\"\nfidelity_class = 0\nclock_start_s = {start}\nend_s = 100000\n{rows}"
    )
}

fn order(id: &str, unit: &str, at: i64, kind: &str) -> String {
    format!("[[order]]\nid = \"{id}\"\nunit = \"{unit}\"\nat_s = {at}\nkind = \"{kind}\"\n{PROV}")
}

fn compile(text: &str) -> Result<Vec<ScriptInput>, ScriptError> {
    compile_script(&ReplayScript::from_toml_str(text)?, &units())
}

#[test]
fn toy_files_load_and_compile() {
    let s = ReplayScript::from_toml_str(TOY_SCRIPT).unwrap();
    let inputs = compile_script(&s, &units()).unwrap();
    // One brigade move expands to two regiments, plus one form and one trigger.
    assert_eq!(inputs.len(), 4);
    assert_eq!(inputs.iter().filter(|i| i.tick == 200).count(), 3);
}

#[test]
fn float_is_refused() {
    let t = script(
        0,
        &order("o1", "toy_r1", 10, "hold").replace("at_s = 10", "at_s = 10.5"),
    );
    assert!(matches!(
        ReplayScript::from_toml_str(&t),
        Err(ScriptError::Data(DataError::Float(_)))
    ));
}

#[test]
fn unknown_key_is_refused() {
    let t = script(
        0,
        &format!("{}bogus = 1\n", order("o1", "toy_r1", 10, "hold")),
    );
    assert!(matches!(
        ReplayScript::from_toml_str(&t),
        Err(ScriptError::Data(DataError::UnknownKey(k))) if k.ends_with("bogus")
    ));
}

#[test]
fn provenance_rules_are_enforced() {
    let bare = "[[order]]\nid = \"o1\"\nunit = \"toy_r1\"\nat_s = 10\nkind = \"hold\"\n";
    let no_ct = format!("{bare}confidence = \"unverified\"\nplaceholder = true\n");
    let verified =
        format!("{bare}confidence = \"verified\"\nsources = [{{ document = \"X\", page = 1 }}]\n");
    for rows in [bare.to_string(), no_ct, verified] {
        assert!(
            matches!(
                ReplayScript::from_toml_str(&script(0, &rows)),
                Err(ScriptError::Data(DataError::Provenance(_)))
            ),
            "{rows}"
        );
    }
}

#[test]
fn bad_rows_are_named() {
    let dup = [
        order("o1", "toy_r1", 10, "hold"),
        order("o1", "toy_r2", 11, "hold"),
    ]
    .concat();
    assert!(matches!(
        ReplayScript::from_toml_str(&script(0, &dup)),
        Err(ScriptError::Duplicate(_))
    ));
    let early = order("o1", "toy_r1", 5, "hold");
    assert!(matches!(
        ReplayScript::from_toml_str(&script(10, &early)),
        Err(ScriptError::Time(_))
    ));
    let late = order("o1", "toy_r1", 100001, "hold");
    assert!(matches!(
        ReplayScript::from_toml_str(&script(0, &late)),
        Err(ScriptError::Time(_))
    ));
    let after = format!(
        "[[observe]]\nid = \"ob\"\nkind = \"loss_rate_at_least\"\nunit = \"toy_r1\"\nrate_pm = 100\nlo_s = 1\nhi_s = 9\nafter = \"nope\"\n{PROV}"
    );
    assert!(matches!(
        ReplayScript::from_toml_str(&script(0, &after)),
        Err(ScriptError::Reference(_))
    ));
    let ghost = order("o1", "ghost", 10, "hold");
    assert!(matches!(
        compile(&script(0, &ghost)),
        Err(ScriptError::Reference(_))
    ));
}

#[test]
fn ticks_are_exact_from_the_clock_start() {
    let trigger = format!("[[trigger]]\nid = \"t1\"\nat_s = 3601\n{PROV}");
    let rows = [order("o1", "toy_r1", 3610, "hold"), trigger].concat();
    let inputs = compile(&script(3600, &rows)).unwrap();
    assert_eq!(inputs[0].tick, 20);
    assert_eq!(inputs[1].tick, 200);
    assert_eq!(inputs[0].payload, vec![TAG_TRIGGER, 0, 0]);
    assert_eq!(ACTOR_HISTORY_SCRIPT, 0xFD);
}

#[test]
fn inputs_sort_by_tick_and_keep_file_order_within_a_tick() {
    let rows = [
        order("o_late", "toy_r1", 30, "hold"),
        order("o_a", "toy_r2", 10, "hold"),
        order("o_b", "toy_r1", 10, "hold"),
        order("o_c", "toy_b1", 10, "hold"),
    ]
    .concat();
    let inputs = compile(&script(0, &rows)).unwrap();
    let key: Vec<(u64, u16)> = inputs.iter().map(|i| (i.tick, i.seq)).collect();
    assert_eq!(key, [(200, 0), (200, 1), (200, 2), (600, 0)]);
    let regs: Vec<u16> = inputs
        .iter()
        .map(|i| decode_order(&i.payload).unwrap().0 .0)
        .collect();
    assert_eq!(regs, [2, 1, 0, 1]);
}

#[test]
fn per_tick_limit_is_65535() {
    let many = |n: usize| {
        let rows: String = (0..n)
            .map(|i| order(&format!("o{i}"), "toy_r1", 10, "hold"))
            .collect();
        compile(&script(0, &rows))
    };
    assert_eq!(many(65_535).unwrap().len(), 65_535);
    assert!(matches!(
        many(65_536),
        Err(ScriptError::TooManyInputs { tick: 200 })
    ));
}

#[test]
fn parent_orders_expand_with_offsets_in_ascending_regiment_id() {
    let mv = order("o1", "toy_bde", 10, "move").replace(
        "kind = \"move\"",
        "kind = \"move\"\nx_m = 500\ny_m = 0\ngait = \"quick\"",
    );
    let inputs = compile(&script(0, &mv)).unwrap();
    assert_eq!(inputs.len(), 2);
    // Regiment ids rank the regiments by id: toy_b1 0, toy_r1 1, toy_r2 2.
    for (input, (reg, y)) in inputs.iter().zip([(1u16, -50), (2u16, 50)]) {
        let (id, o) = decode_order(&input.payload).unwrap();
        assert_eq!(id, RegimentId(reg));
        let Order::Move { to, gait } = o else {
            panic!("not a move")
        };
        assert_eq!(
            (to.x, to.y, gait),
            (Fx::from_int(500), Fx::from_int(y), Gait::Quick)
        );
    }
}

#[test]
fn order_kinds_encode() {
    let rows = [
        order("o1", "toy_r1", 1, "form").replace("\"form\"", "\"form\"\nformation = \"skirmish\""),
        order("o2", "toy_r1", 2, "face").replace("\"face\"", "\"face\"\nheading_deg = 90"),
        order("o3", "toy_r1", 3, "fire_at")
            .replace("\"fire_at\"", "\"fire_at\"\ntarget_unit = \"toy_b1\""),
    ]
    .concat();
    let inputs = compile(&script(0, &rows)).unwrap();
    assert_eq!(
        decode_order(&inputs[0].payload).unwrap().1,
        Order::Form(Formation::Skirmish)
    );
    assert_eq!(
        decode_order(&inputs[1].payload).unwrap().1,
        Order::Face(Angle::from_degrees_int(90))
    );
    assert_eq!(inputs[2].payload, vec![5, 1, 0, 0, 0]);
}

#[test]
fn missing_order_field_is_refused() {
    let t = script(0, &order("o1", "toy_r1", 10, "move"));
    assert!(matches!(
        ReplayScript::from_toml_str(&t),
        Err(ScriptError::Data(DataError::Parse(_)))
    ));
}

#[test]
fn units_load_in_ascending_id() {
    let ids: Vec<String> = units().into_iter().map(|r| r.id).collect();
    assert_eq!(ids, ["toy_army", "toy_b1", "toy_bde", "toy_r1", "toy_r2"]);
    let bad = TOY_UNITS.replace("parent = \"toy_army\"", "parent = \"missing\"");
    assert!(matches!(
        units_from_toml_str(&bad),
        Err(ScriptError::Reference(_))
    ));
    let dup = TOY_UNITS.replace("id = \"toy_r2\"", "id = \"toy_r1\"");
    assert!(matches!(
        units_from_toml_str(&dup),
        Err(ScriptError::Duplicate(_))
    ));
}
