//! The fixed scenes of the acoustics bands and goldens (SPEC-012 sections 11 and 12).
//!
//! Each scene is a list of sound events around the origin inside one window of 20 ticks, plus the
//! weather and listener distances its golden rows use. The listener stands on the +x axis. Event
//! positions are whole metres, so every scene is the same on every platform.

use crate::aggregate::{Aggregator, Cluster, SoundEvent};
use crate::listener::{render_packet, Listener, ListenerKind, ListenerState};
use crate::packet::MixPacket;
use crate::relief::Relief;
use crate::report::{hearing_report, HearingReport};
use crate::tables::AcousticTables;
use crate::weather::Weather;
use core_math::{Fx, Vec2Fx};

const MUSKET: u8 = 0;
const GUN_LAUNCH: u8 = 3;
const COMMAND: u8 = 6;
const CRY: u8 = 8;
/// Window of every scene, ticks.
const WINDOW_TICKS: u32 = 20;

/// One hearing case of a scene: a weather preset id and the listener's distance, mm.
pub type Case = (&'static str, i64);

/// A scene: events, window, optional terrain and the cases its goldens cover.
#[derive(Clone, Debug)]
pub struct Scene {
    /// Scene name.
    pub name: &'static str,
    /// The sound events, in no particular order.
    pub events: Vec<SoundEvent>,
    /// Window length, ticks.
    pub window_ticks: u32,
    /// Last tick of the window.
    pub window_end_tick: u64,
    /// Terrain, if the scene has any.
    pub relief: Option<Relief>,
    /// Weather and distance pairs the golden rows cover.
    pub cases: Vec<Case>,
}

/// What a listener hears of a scene in one case.
#[derive(Clone, Debug)]
pub struct Outcome {
    /// The camera packet.
    pub packet: MixPacket,
    /// The headquarters report.
    pub report: HearingReport,
}

/// Position `mm` millimetres along an axis, as a fixed-point metre value that converts back to
/// exactly `mm` through `to_mm` (the raw value is rounded up).
fn fx_mm(mm: i64) -> Fx {
    Fx::from_raw((((i128::from(mm) << 32) + 999).div_euclid(1000)) as i64)
}

fn at(x: i32, y: i32) -> Vec2Fx {
    Vec2Fx {
        x: Fx::from_int(x),
        y: Fx::from_int(y),
    }
}

/// Appends `shots` single events of one source for `unit`, spread over 8 by `shots / 8` metres
/// inside one 100 m cell whose south-west corner is `(x0, y0)` (offsets from 10 m).
fn volley(ev: &mut Vec<SoundEvent>, source: u8, unit: u32, shots: u32, x0: i32, y0: i32) {
    for j in 0..shots {
        let seq = ev.len() as u64;
        ev.push(SoundEvent {
            seq,
            source,
            unit,
            team: (unit % 2) as u8,
            pos: at(x0 + 10 + (j % 8) as i32 * 5, y0 + 10 + (j / 8) as i32 * 4),
            height_mm: 1500,
            count: 1,
        });
    }
}

fn scene(name: &'static str, events: Vec<SoundEvent>, cases: Vec<Case>) -> Scene {
    Scene {
        name,
        events,
        window_ticks: WINDOW_TICKS,
        window_end_tick: u64::from(WINDOW_TICKS) - 1,
        relief: None,
        cases,
    }
}

/// Six units of 40 musket shots, 200 m apart, about 3 km from the origin of the first battle
/// (the listener distances of bands CB2, CB3 and CB5).
pub fn seven_pines() -> Scene {
    let mut ev = Vec::new();
    for k in 0..6 {
        volley(&mut ev, MUSKET, k, 40, -50, k as i32 * 300);
    }
    scene(
        "seven_pines",
        ev,
        vec![
            ("calm", 3_218_688),
            ("toward_minus_x", 3_218_688),
            ("toward_plus_x_inversion", 8_046_720),
            ("toward_plus_x_inversion", 16_093_440),
            ("calm", 16_093_440),
        ],
    )
}

/// Twenty units of 40 musket shots (band CB4).
pub fn perryville() -> Scene {
    let mut ev = Vec::new();
    for k in 0..20 {
        volley(&mut ev, MUSKET, k, 40, -50, k as i32 * 200);
    }
    scene(
        "perryville",
        ev,
        vec![("toward_minus_x", 4_828_032), ("calm", 4_828_032)],
    )
}

/// One battery of three guns firing once in the window (band CB6).
pub fn bombardment() -> Scene {
    let mut ev = Vec::new();
    volley(&mut ev, GUN_LAUNCH, 0, 3, -50, 0);
    scene(
        "bombardment",
        ev,
        vec![
            ("calm", 15_000_000),
            ("toward_minus_x", 15_000_000),
            ("toward_plus_x_inversion", 15_000_000),
        ],
    )
}

/// Musketry and a battery behind a ridge pair: ridges 25 m high at 500 m and 1300 m from the
/// origin, and a 200 m strip of woods (700 to 900 m) between them, over a 2.6 km strip of 10 m cells.
pub fn ridge_pair() -> Scene {
    let mut ev = Vec::new();
    volley(&mut ev, MUSKET, 0, 40, -50, 0);
    volley(&mut ev, GUN_LAUNCH, 1, 3, -50, 200);
    let width = 260usize;
    let mut elev_mm = vec![0i32; width];
    let mut class = vec![0u8; width];
    for cell in [60, 140] {
        elev_mm[cell] = 25_000;
    }
    for c in &mut class[80..100] {
        *c = 3;
    }
    let mut s = scene(
        "ridge_pair",
        ev,
        vec![
            ("calm", 1_000_000),
            ("calm", 2_000_000),
            ("toward_minus_x", 2_000_000),
        ],
    );
    s.relief = Some(Relief {
        origin: at(-100, -5),
        cell_mm: 10_000,
        width: width as u32,
        height: 1,
        elev_mm,
        class,
    });
    s
}

/// 20,000 events of four sources from 40 units spread over a 2 km square; positions come from a
/// fixed xorshift sequence.
pub fn volley_20k() -> Scene {
    let mut state = 0x1861_0412_u64;
    let mut next = |n: u64| {
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        (state.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) % n
    };
    let mut ev = Vec::with_capacity(20_000);
    for seq in 0..20_000u64 {
        let unit = next(40) as u32;
        let source = match next(20) {
            0..=15 => MUSKET,
            16..=17 => GUN_LAUNCH,
            18 => COMMAND,
            _ => CRY,
        };
        let (bx, by) = ((unit % 8) as i32 * 250, (unit / 8) as i32 * 250);
        ev.push(SoundEvent {
            seq,
            source,
            unit,
            team: (unit % 2) as u8,
            pos: at(bx + next(80) as i32 - 40, by + next(80) as i32 - 40),
            height_mm: 1500,
            count: 1,
        });
    }
    scene(
        "volley_20k",
        ev,
        vec![
            ("calm", 3_000_000),
            ("toward_minus_x", 3_000_000),
            ("calm", 12_000_000),
        ],
    )
}

/// The five scenes in the order of the goldens.
pub fn all() -> Vec<Scene> {
    vec![
        seven_pines(),
        perryville(),
        bombardment(),
        ridge_pair(),
        volley_20k(),
    ]
}

/// The scene called `name`.
pub fn by_name(name: &str) -> Option<Scene> {
    all().into_iter().find(|s| s.name == name)
}

impl Scene {
    /// The clusters of the scene's window.
    pub fn clusters(&self) -> Vec<Cluster> {
        let mut a = Aggregator::new(self.window_ticks);
        for e in &self.events {
            a.push(e);
        }
        a.flush(self.window_end_tick)
    }

    /// A headquarters listener on the +x axis `distance_mm` from the origin, with the ambient
    /// level of each band from the tables.
    pub fn listener(&self, t: &AcousticTables, distance_mm: i64) -> Listener {
        Listener {
            id: 1,
            kind: ListenerKind::Headquarters,
            pos: Vec2Fx {
                x: fx_mm(distance_mm),
                y: Fx::ZERO,
            },
            height_mm: 1500,
            ambient_cdb: [0, 1, 2].map(|b| t.bands()[b].ambient_cdb),
        }
    }

    /// The camera packet and headquarters report for the weather preset `weather` and a listener
    /// `distance_mm` away.
    ///
    /// # Panics
    /// If `weather` is not a preset of `t`.
    pub fn run(&self, t: &AcousticTables, weather: &str, distance_mm: i64) -> Outcome {
        let w = Weather::from_preset(t.weather(weather).expect("weather preset exists"));
        let cl = self.clusters();
        let lis = self.listener(t, distance_mm);
        let rel = self.relief.as_ref();
        Outcome {
            packet: render_packet(&lis, &mut ListenerState::default(), &cl, t, &w, rel),
            report: hearing_report(&lis, &cl, t, &w, rel, self.window_end_tick + 1),
        }
    }
}
