//! Interface state that is never hashed or recorded (SPEC-015 section 3, item 3, and section
//! 10): mode, pause, speed and the integer ticks-per-frame accumulator.

use battle_rules::Order;

/// Which screen the interface is in. Pause is accepted in every mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiMode {
    /// Live battle.
    Battle,
    /// Replay of a recorded battle.
    Replay,
    /// Photo mode.
    Photo,
    /// Overnight planning.
    Overnight,
    /// Tutorial.
    Tutorial,
    /// Briefing before a scenario.
    Briefing,
    /// Menu.
    Menu,
}

impl UiMode {
    /// Every mode.
    pub const ALL: [UiMode; 7] = [
        UiMode::Battle,
        UiMode::Replay,
        UiMode::Photo,
        UiMode::Overnight,
        UiMode::Tutorial,
        UiMode::Briefing,
        UiMode::Menu,
    ];
}

/// The seven speeds, in permille of one tick per frame.
pub const SPEEDS: [u16; 7] = [250, 500, 1000, 2000, 4000, 8000, 16000];

/// Highest speed in live play.
pub const LIVE_SPEED_CAP: u16 = 4000;

/// Why a UI action was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiError {
    /// The speed is not one of [`SPEEDS`].
    BadSpeed(u16),
    /// The speed is above [`LIVE_SPEED_CAP`] in live play.
    SpeedAboveLiveCap(u16),
}

/// A change to the interface state. None of these reaches the simulation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UiAction {
    /// Select a regiment or clear the selection.
    Select(Option<u16>),
    /// Set or clear the order being drafted.
    Draft(Option<Order>),
    /// Move the camera, mm.
    Pan([i64; 2]),
    /// Toggle overlay number `k` (bit in the overlay mask).
    ToggleOverlay(u8),
    /// Change the mode.
    SetMode(UiMode),
    /// Pause or resume.
    Pause(bool),
    /// Set the speed (one of [`SPEEDS`]).
    SetSpeed(u16),
}

/// The interface state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiState {
    /// Current mode.
    pub mode: UiMode,
    /// Paused: no ticks are due.
    pub paused: bool,
    /// Speed, permille of one tick per frame.
    pub speed_pm: u16,
    /// Live play caps the speed at [`LIVE_SPEED_CAP`].
    pub live: bool,
    /// Selected regiment.
    pub selected: Option<u16>,
    /// Order being drafted.
    pub draft: Option<Order>,
    /// Camera position, mm.
    pub camera_mm: [i64; 2],
    /// Overlay bit mask.
    pub overlays: u8,
    acc: u32,
}

impl UiState {
    /// A battle-mode state at speed 1000, unpaused.
    pub fn new(live: bool) -> UiState {
        UiState {
            mode: UiMode::Battle,
            paused: false,
            speed_pm: 1000,
            live,
            selected: None,
            draft: None,
            camera_mm: [0, 0],
            overlays: 0,
            acc: 0,
        }
    }

    /// Applies one action.
    pub fn apply(&mut self, a: UiAction) -> Result<(), UiError> {
        match a {
            UiAction::Select(s) => self.selected = s,
            UiAction::Draft(d) => self.draft = d,
            UiAction::Pan([x, y]) => {
                self.camera_mm = [
                    self.camera_mm[0].saturating_add(x),
                    self.camera_mm[1].saturating_add(y),
                ]
            }
            UiAction::ToggleOverlay(k) => self.overlays ^= 1 << (k & 7),
            UiAction::SetMode(m) => self.mode = m,
            UiAction::Pause(p) => self.paused = p,
            UiAction::SetSpeed(s) => {
                if !SPEEDS.contains(&s) {
                    return Err(UiError::BadSpeed(s));
                }
                if self.live && s > LIVE_SPEED_CAP {
                    return Err(UiError::SpeedAboveLiveCap(s));
                }
                self.speed_pm = s;
            }
        }
        Ok(())
    }

    /// Ticks due this frame: the accumulator gains `speed_pm` and yields whole thousandths. A
    /// paused frame yields none and keeps the remainder.
    pub fn frame_ticks(&mut self) -> u32 {
        if self.paused {
            return 0;
        }
        self.acc += u32::from(self.speed_pm);
        let n = self.acc / 1000;
        self.acc %= 1000;
        n
    }
}
