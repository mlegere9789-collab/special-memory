//! The engine facade (SPEC-012 sections 8 and 9): two aggregators, the listeners' state and the
//! headquarters stagger behind `push` and `end_tick`.

use crate::aggregate::{Aggregator, Cluster, SoundEvent};
use crate::listener::{render_packet, Listener, ListenerKind, ListenerState};
use crate::packet::MixPacket;
use crate::relief::Relief;
use crate::report::{hearing_report, HearingReport};
use crate::tables::AcousticTables;
use crate::weather::Weather;
use std::collections::BTreeMap;

/// What one tick produced.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TickOutput {
    /// Camera packets, one per camera listener in ascending id, on the last tick of a camera window.
    pub packets: Vec<MixPacket>,
    /// Hearing reports of the headquarters whose turn it is, in ascending id.
    pub reports: Vec<HearingReport>,
}

/// Runs the camera and headquarters pipelines over a stream of sound events.
///
/// Events go into both aggregators. At the last tick of a camera window (`tick % w == w - 1`) the
/// camera aggregator is flushed and every camera listener gets a packet. At the last tick of a
/// headquarters window the other aggregator is flushed and kept; headquarters number `j`
/// (ascending id among the headquarters of the call) reports on tick `window_end + 1 + (j % w)`
/// from the last completed window, so the cost is spread over `w` ticks.
pub struct AcousticsEngine {
    tables: AcousticTables,
    weather: Weather,
    relief: Option<Relief>,
    camera: Aggregator,
    hq: Aggregator,
    states: BTreeMap<u32, ListenerState>,
    /// The last completed headquarters window: its end tick and clusters.
    last_hq: Option<(u64, Vec<Cluster>)>,
}

impl AcousticsEngine {
    /// An engine with the window lengths and cell size of `tables`.
    pub fn new(
        tables: AcousticTables,
        weather: Weather,
        relief: Option<Relief>,
    ) -> AcousticsEngine {
        let g = tables.globals();
        let (cell, cw, hw) = (g.cluster_cell_mm, g.camera_window_ticks, g.hq_window_ticks);
        AcousticsEngine {
            camera: Aggregator::with_cell(cw, cell),
            hq: Aggregator::with_cell(hw, cell),
            tables,
            weather,
            relief,
            states: BTreeMap::new(),
            last_hq: None,
        }
    }

    /// Adds one event to the open windows of both pipelines.
    pub fn push(&mut self, e: &SoundEvent) {
        self.camera.push(e);
        self.hq.push(e);
    }

    /// Closes `tick`: call it once per tick, in ascending order, after that tick's events were
    /// pushed. Listeners are taken in ascending id whatever their order in the slice.
    pub fn end_tick(&mut self, tick: u64, listeners: &[Listener]) -> TickOutput {
        let g = self.tables.globals();
        let (cw, hw) = (
            u64::from(g.camera_window_ticks),
            u64::from(g.hq_window_ticks),
        );
        let mut sorted: Vec<&Listener> = listeners.iter().collect();
        sorted.sort_by_key(|l| l.id);
        let mut out = TickOutput::default();

        // Reports first: on the end tick of a window the previous window is still the last
        // completed one for the headquarters whose turn falls on this tick.
        if let Some((end, clusters)) = &self.last_hq {
            let hqs = sorted
                .iter()
                .filter(|l| l.kind == ListenerKind::Headquarters);
            for (j, l) in hqs.enumerate() {
                if end + 1 + (j as u64 % hw) == tick {
                    let (t, w, r) = (&self.tables, &self.weather, self.relief.as_ref());
                    out.reports.push(hearing_report(l, clusters, t, w, r, tick));
                }
            }
        }
        if tick % hw == hw - 1 {
            self.last_hq = Some((tick, self.hq.flush(tick)));
        }
        if tick % cw == cw - 1 {
            let clusters = self.camera.flush(tick);
            for l in sorted.iter().filter(|l| l.kind == ListenerKind::Camera) {
                let state = self.states.entry(l.id).or_default();
                let (t, w, r) = (&self.tables, &self.weather, self.relief.as_ref());
                out.packets
                    .push(render_packet(l, state, &clusters, t, w, r));
            }
        }
        out
    }
}
