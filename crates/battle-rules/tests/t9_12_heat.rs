//! T9-12: apparent temperature, the heat curve and the multiplied fatigue drain and recovery
//! (SPEC-009 section 7, SPEC-004 amendment A5, SPEC-005 amendment A8).

mod t019_support;
mod t022_support;
mod t9_support;

use battle_rules::{
    apparent_f10, fatigue_step, fatigue_step_scaled, scenarios, Gait, HeatNow, Training,
};
use core_math::Fx;
use t9_support::{env_table, weather};

#[test]
fn apparent_temperature_adds_humidity_above_the_floor() {
    let h = env_table().heat;
    // At or below the floor, or at or below the humidity floor, nothing is added.
    assert_eq!(apparent_f10(&h, 700, 1000), 700);
    assert_eq!(apparent_f10(&h, 650, 900), 650);
    assert_eq!(apparent_f10(&h, 900, 400), 900);
    assert_eq!(apparent_f10(&h, 900, 100), 900);
    // 880 + 180 * 250 / 1500 = 910; 900 + 200 * 600 / 1500 = 980.
    assert_eq!(apparent_f10(&h, 880, 650), 910);
    assert_eq!(apparent_f10(&h, 900, 1000), 980);
    assert_eq!(apparent_f10(&h, 1000, 700), 1000 + 300 * 300 / 1500);
}

#[test]
fn the_heat_curve_interpolates_and_clamps() {
    let e = env_table();
    assert_eq!(e.heat_multipliers(0), (1000, 1000));
    assert_eq!(e.heat_multipliers(700), (1000, 1000));
    assert_eq!(e.heat_multipliers(750), (1050, 975));
    assert_eq!(e.heat_multipliers(800), (1100, 950));
    assert_eq!(e.heat_multipliers(900), (1300, 850));
    assert_eq!(e.heat_multipliers(910), (1340, 835));
    assert_eq!(e.heat_multipliers(950), (1500, 775));
    assert_eq!(e.heat_multipliers(1000), (1700, 700));
    assert_eq!(e.heat_multipliers(1300), (1700, 700));
    // Truncation toward zero on the falling rest curve: 1000 - 50 * 3 / 100 = 1000 - 1.
    assert_eq!(e.heat_multipliers(703), (1003, 999));
}

#[test]
fn quick_march_drain_equals_the_floor_formula_at_apparent_900() {
    let t = t019_support::tables();
    let drain = t.locomotion.gait(Gait::Quick).drain_ppm_per_tick;
    let moving = Fx::from_int(1);
    for effort in [0u32, 1000, 1100, 1600, 2000] {
        let want = u64::from(drain) * u64::from(effort) * 1300 / 1_000_000;
        let got = fatigue_step_scaled(0, Gait::Quick, effort, moving, &t.locomotion, 1300, 850);
        assert_eq!(u64::from(got), want, "effort {effort}");
    }
    // Recovery: rest_recover * 850 / 1000, saturating at zero.
    let rec = t.locomotion.globals().rest_recover_ppm_per_tick;
    let start = 500_000;
    let got = fatigue_step_scaled(start, Gait::Quick, 1000, Fx::ZERO, &t.locomotion, 1300, 850);
    assert_eq!(got, start - rec * 850 / 1000);
    let got = fatigue_step_scaled(3, Gait::Quick, 1000, Fx::ZERO, &t.locomotion, 1300, 850);
    assert_eq!(got, 0);
}

#[test]
fn neutral_multipliers_equal_the_plain_step() {
    let t = t019_support::tables();
    for gait in [Gait::Walk, Gait::Quick, Gait::DoubleQuick, Gait::Run] {
        for (f, v) in [(0u32, 1i32), (123_456, 2), (999_999, 3), (1_000_000, 0)] {
            for effort in [900u32, 1000, 1333] {
                let a = fatigue_step(f, gait, effort, Fx::from_int(v), &t.locomotion);
                let b = fatigue_step_scaled(
                    f,
                    gait,
                    effort,
                    Fx::from_int(v),
                    &t.locomotion,
                    1000,
                    1000,
                );
                assert_eq!(a, b);
            }
        }
    }
}

fn fatigue_of(b: &battle_rules::BattleState) -> Vec<u32> {
    b.loco.iter().map(|l| l.fatigue).collect()
}

#[test]
fn neutral_weather_march_single_equals_the_none_run() {
    let t = t019_support::tables();
    let mut plain = scenarios::march_single(&t, 1);
    let mut env = scenarios::march_single(&t, 1);
    // 68 F, humid, calm, dry: every multiplier is 1000 and the weather stays dry.
    env.set_environment(env_table(), weather(&[(0, 680, 800, 0, 0, 0)]));
    for _ in 0..800 {
        plain.step();
        env.step();
        assert_eq!(fatigue_of(&plain), fatigue_of(&env));
    }
    assert!(plain.loco.iter().any(|l| l.fatigue > 0));
    // The environment block (SPEC-009 section 9) is the only difference between the two hashes.
    let with_block = env.hash();
    let mut stripped = env.clone();
    stripped.env = None;
    assert_eq!(plain.hash(), stripped.hash());
    assert_ne!(plain.hash(), with_block);
    assert_eq!(env.weather.wet_pm, 1000);
    assert_eq!(env.weather.ground_pm, 1000);
}

#[test]
fn hot_march_drains_by_the_multiplier_from_the_first_moving_tick() {
    let t = t019_support::tables();
    let mut plain = scenarios::march_single(&t, 1);
    let mut hot = scenarios::march_single(&t, 1);
    // 90 F at humidity 300: apparent 900, drain 1300, rest 850.
    hot.set_environment(env_table(), weather(&[(0, 900, 300, 0, 0, 0)]));
    let h = hot.env.as_ref().unwrap().heat();
    assert_eq!(
        h,
        HeatNow {
            apparent_f10: 900,
            drain_pm: 1300,
            rest_pm: 850
        }
    );
    let drain = t.locomotion.gait(Gait::Quick).drain_ppm_per_tick;
    let effort = t.terrain.class(0).unwrap().effort_pm;
    let want_plain = u64::from(drain) * u64::from(effort) / 1000;
    let want_hot = u64::from(drain) * u64::from(effort) * 1300 / 1_000_000;
    assert!(want_hot > want_plain);
    let mut checked = false;
    for _ in 0..400 {
        let (a0, b0) = (fatigue_of(&plain), fatigue_of(&hot));
        plain.step();
        hot.step();
        let (a1, b1) = (fatigue_of(&plain), fatigue_of(&hot));
        if a0.iter().all(|&f| f == 0) && a1.iter().any(|&f| f > 0) {
            let i = a1.iter().position(|&f| f > 0).unwrap();
            assert_eq!(u64::from(a1[i] - a0[i]), want_plain);
            assert_eq!(u64::from(b1[i] - b0[i]), want_hot);
            checked = true;
            break;
        }
    }
    assert!(checked, "no soldier started to move");
    for _ in 0..800 {
        plain.step();
        hot.step();
    }
    let (p, q): (u64, u64) = (
        fatigue_of(&plain).iter().map(|&f| u64::from(f)).sum(),
        fatigue_of(&hot).iter().map(|&f| u64::from(f)).sum(),
    );
    assert!(q > p, "heat must tire the men more: {q} against {p}");
}

#[test]
fn firing_drain_is_multiplied_by_the_heat_drain() {
    let weapon = "smoothbore_69";
    let want = |drain_pm: u64, fire: u32| u32::try_from(u64::from(fire) * drain_pm / 1000).unwrap();
    let mut seen = Vec::new();
    for hot in [false, true] {
        let (mut b, _, _) = t022_support::duel(5, weapon, 0, Training::Trained, 1, 60);
        let fire = b.tables.weapons.get(weapon).unwrap().fire_drain_ppm;
        if hot {
            b.set_environment(env_table(), weather(&[(0, 900, 300, 0, 0, 0)]));
        }
        let mut fired = false;
        for _ in 0..4000 {
            t022_support::tick(&mut b);
            if b.loco[0].fatigue != 0 {
                fired = true;
                break;
            }
        }
        assert!(fired, "the soldier never fired");
        seen.push((b.loco[0].fatigue, want(if hot { 1300 } else { 1000 }, fire)));
    }
    for (got, expected) in seen {
        assert_eq!(got, expected);
    }
}
