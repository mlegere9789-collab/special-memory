//! T15-03 pause and speed: speed and pauses never change what the simulation computes.

mod common;

use battle_rules::{battle_hash, Formation};
use common::battle;
use proptest::prelude::*;
use ui_model::{UiAction, UiError, UiMode, UiState, SPEEDS};

const TARGET: u64 = 120;

fn straight() -> u64 {
    let mut b = battle(
        3,
        &[
            (0, 10, Formation::Column, 0, 0),
            (1, 10, Formation::Line, 0, 90),
        ],
    );
    (0..TARGET).for_each(|_| b.step());
    battle_hash(&b)
}

fn paced(speed: u16, pauses: &[bool]) -> u64 {
    let mut b = battle(
        3,
        &[
            (0, 10, Formation::Column, 0, 0),
            (1, 10, Formation::Line, 0, 90),
        ],
    );
    let mut ui = UiState::new(false);
    ui.apply(UiAction::SetSpeed(speed)).unwrap();
    let (mut done, mut frame) = (0u64, 0usize);
    while done < TARGET {
        // Even frames may pause, odd frames never do, so the clock always advances.
        ui.apply(UiAction::Pause(
            pauses[frame % pauses.len()] && frame % 2 == 0,
        ))
        .unwrap();
        frame += 1;
        let n = u64::from(ui.frame_ticks()).min(TARGET - done);
        (0..n).for_each(|_| b.step());
        done += n;
        assert!(frame < 100_000, "the clock never advanced");
    }
    battle_hash(&b)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(40))]
    #[test]
    fn all_speeds_with_random_pauses_give_the_same_hash(
        pauses in proptest::collection::vec(any::<bool>(), 1..16)
    ) {
        let want = straight();
        for speed in SPEEDS {
            prop_assert_eq!(paced(speed, &pauses), want, "speed {}", speed);
        }
    }

    #[test]
    fn the_accumulator_loses_no_tick(speed_ix in 0usize..7, frames in 0u32..400) {
        let mut ui = UiState::new(false);
        ui.apply(UiAction::SetSpeed(SPEEDS[speed_ix])).unwrap();
        let total: u64 = (0..frames).map(|_| u64::from(ui.frame_ticks())).sum();
        prop_assert_eq!(total, u64::from(frames) * u64::from(SPEEDS[speed_ix]) / 1000);
    }
}

#[test]
fn pause_is_accepted_in_every_mode_and_freezes_the_clock() {
    for mode in UiMode::ALL {
        let mut ui = UiState::new(true);
        ui.apply(UiAction::SetMode(mode)).unwrap();
        ui.apply(UiAction::Pause(true)).unwrap();
        assert!(ui.paused, "{mode:?}");
        assert_eq!(ui.frame_ticks(), 0, "{mode:?}");
    }
    assert_eq!(UiMode::ALL.len(), 7);
}

#[test]
fn speeds_are_the_seven_values_and_live_play_caps_at_4000() {
    assert_eq!(SPEEDS, [250, 500, 1000, 2000, 4000, 8000, 16000]);
    let mut live = UiState::new(true);
    assert_eq!(live.apply(UiAction::SetSpeed(4000)), Ok(()));
    assert_eq!(
        live.apply(UiAction::SetSpeed(8000)),
        Err(UiError::SpeedAboveLiveCap(8000))
    );
    assert_eq!(
        live.apply(UiAction::SetSpeed(1500)),
        Err(UiError::BadSpeed(1500))
    );
    assert_eq!(live.speed_pm, 4000);
    let mut replay = UiState::new(false);
    assert_eq!(replay.apply(UiAction::SetSpeed(16000)), Ok(()));
}
