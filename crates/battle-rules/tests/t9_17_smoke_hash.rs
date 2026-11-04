//! T9-17 (smoke part): every hashed environment field moves the hash, and with the environment
//! off none of them does (SPEC-009 section 9).

mod t022_support;
mod t93_support;

use battle_rules::{BattleState, SmokeGrid};
use t022_support::field;
use t93_support::{env_table, put, weather};

fn battle(rain: u32, vis: i32) -> BattleState {
    let mut b = field(1);
    b.smoke = SmokeGrid::new(8000, 8, 8, core_math::Vec2Fx::ZERO).unwrap();
    put(&mut b.smoke, 3, 3, 40_000);
    let mut text = weather(&[(0, 700, 500, 1500, -500, rain)]);
    if vis != 16_000_000 {
        text = visibility(vis);
    }
    b.set_environment(env_table(), text);
    b
}

fn visibility(vis: i32) -> battle_rules::WeatherTable {
    let text = format!(
        "schema = \"weather/1\"\nstart_minute = 0\nkey = [\n  {{ t_min = 0, temp_f10 = 700, humidity_pm = 500, \
         wind_x_mm_s = 1500, wind_y_mm_s = -500, rain_pm = 0, clear_vis_mm = {vis}, light_pm = 1000, \
         confidence = \"unverified\", placeholder = true, calibration_target = \"CT-009-6\" }},\n]\n"
    );
    battle_rules::WeatherTable::from_toml_str(&text).unwrap()
}

#[test]
fn each_loss_counter_the_weather_and_the_damp_state_change_the_hash() {
    let base = battle(0, 16_000_000).hash();
    assert_eq!(base, battle(0, 16_000_000).hash());
    for which in 0..3 {
        let mut b = battle(0, 16_000_000);
        match which {
            0 => b.smoke.lost_edge += 1,
            1 => b.smoke.lost_decay += 1,
            _ => b.smoke.lost_cap += 1,
        }
        assert_ne!(b.hash(), base, "counter {which}");
    }
    assert_ne!(battle(0, 15_000_000).hash(), base, "WeatherNow");
    // Rain for two periods raises damp and ground: the hash moves even when density is reset.
    let (mut wet, mut dry) = (battle(900, 16_000_000), battle(0, 16_000_000));
    for _ in 0..12 {
        wet.step_environment();
        dry.step_environment();
        wet.world.advance_tick();
        dry.world.advance_tick();
    }
    assert!(
        wet.env.as_ref().unwrap().powder_damp_ppm() > dry.env.as_ref().unwrap().powder_damp_ppm()
    );
    assert_ne!(wet.hash(), dry.hash());
}

#[test]
fn smoke_density_is_hashed_and_the_environment_block_is_absent_when_off() {
    let mut b = battle(0, 16_000_000);
    let h = b.hash();
    put(&mut b.smoke, 4, 4, 1);
    assert_ne!(b.hash(), h);

    // With `env = None` the counters are not part of the hash.
    let mut off = battle(0, 16_000_000);
    off.env = None;
    let h = off.hash();
    off.smoke.lost_edge += 5;
    off.smoke.lost_decay += 5;
    off.smoke.lost_cap += 5;
    assert_eq!(off.hash(), h);
    assert_ne!(h, battle(0, 16_000_000).hash());
}

#[test]
fn updates_that_move_smoke_change_the_hash_each_period() {
    let mut b = battle(0, 16_000_000);
    let mut seen = vec![b.hash()];
    for _ in 0..4 {
        b.step();
        seen.push(b.hash());
    }
    let mut sorted = seen.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), seen.len());
}
