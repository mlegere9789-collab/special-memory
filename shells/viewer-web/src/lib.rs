//! Debug viewer host: runs the toy scenario of `hd-sim` and exposes read-only snapshots.
//!
//! [`Snapshot`] is plain Rust with no browser dependency, so native tests exercise it directly.
//! [`Viewer`] is the thin `wasm-bindgen` wrapper that JavaScript calls. No floating point is
//! used here: positions leave as whole centimetres in `i32`, and the page converts for drawing.
//! The simulation is never touched by the page except through [`Viewer::advance_to`].
#![deny(missing_docs)]

use hd_sim::scenario::{check_limits, Scenario, ToyParams};
use wasm_bindgen::prelude::*;

/// Centimetres per metre, for the integer position export.
const CM_PER_M: i64 = 100;

/// A read-only copy of the live agents at one tick, in ascending slot order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    /// Tick the snapshot was taken at.
    pub tick: u64,
    /// Canonical state hash at that tick.
    pub hash: u64,
    /// Interleaved `x, y` positions in whole centimetres.
    pub positions_cm: Vec<i32>,
    /// Team number per agent.
    pub teams: Vec<u8>,
    /// Hit points per agent.
    pub hp: Vec<u16>,
}

/// Position component in centimetres: `raw * 100 / 2^32`, rounded toward negative infinity.
fn to_cm(raw: i64) -> i32 {
    ((i128::from(raw) * i128::from(CM_PER_M)) >> 32) as i32
}

/// A scenario plus the tick it has reached.
pub struct Session {
    scenario: Scenario,
    params: ToyParams,
    tick: u64,
}

impl Session {
    /// Builds the scenario at tick 0 with the default arena and tick rate.
    pub fn new(seed: u64, agents: u32) -> Result<Session, String> {
        let params = ToyParams::new(agents, seed);
        check_limits(agents, 0).map_err(|e| e.to_string())?;
        let scenario = Scenario::new(&params).map_err(|e| e.to_string())?;
        Ok(Session {
            scenario,
            params,
            tick: 0,
        })
    }

    /// Steps forward until `tick`; a tick in the past is an error (the run never rewinds).
    pub fn advance_to(&mut self, tick: u64) -> Result<(), String> {
        if tick < self.tick {
            return Err(format!("cannot rewind from tick {} to {tick}", self.tick));
        }
        check_limits(self.params.agents, tick).map_err(|e| e.to_string())?;
        while self.tick < tick {
            self.scenario.step();
            self.tick += 1;
        }
        Ok(())
    }

    /// Current tick.
    pub fn tick(&self) -> u64 {
        self.tick
    }

    /// Arena width and height in whole metres.
    pub fn arena_m(&self) -> (u32, u32) {
        (self.params.width_m, self.params.height_m)
    }

    /// Copies the live agents out of the authoritative state.
    pub fn snapshot(&self) -> Snapshot {
        let agents = &self.scenario.world().agents;
        let n = agents.len();
        let mut s = Snapshot {
            tick: self.tick,
            hash: self.scenario.hash(),
            positions_cm: Vec::with_capacity(n * 2),
            teams: Vec::with_capacity(n),
            hp: Vec::with_capacity(n),
        };
        for id in agents.iter_alive() {
            if let Some(a) = agents.get(id) {
                s.positions_cm.push(to_cm(a.pos.x.raw()));
                s.positions_cm.push(to_cm(a.pos.y.raw()));
                s.teams.push(a.team);
                s.hp.push(a.hp);
            }
        }
        s
    }
}

/// The JavaScript-facing viewer: build it, advance it, read the snapshot.
#[wasm_bindgen]
pub struct Viewer {
    session: Session,
    snap: Snapshot,
}

#[wasm_bindgen]
impl Viewer {
    /// Builds the toy scenario for `seed` and `agents` at tick 0.
    #[wasm_bindgen(constructor)]
    pub fn new(seed: u64, agents: u32) -> Result<Viewer, JsError> {
        let session = Session::new(seed, agents).map_err(|e| JsError::new(&e))?;
        let snap = session.snapshot();
        Ok(Viewer { session, snap })
    }

    /// Steps the simulation forward to `tick` and refreshes the snapshot.
    pub fn advance_to(&mut self, tick: u32) -> Result<(), JsError> {
        self.session
            .advance_to(u64::from(tick))
            .map_err(|e| JsError::new(&e))?;
        self.snap = self.session.snapshot();
        Ok(())
    }

    /// Tick of the current snapshot.
    pub fn tick(&self) -> u32 {
        self.snap.tick as u32
    }

    /// State hash of the current snapshot as 16 lowercase hex digits (same format as `hd-sim hash`).
    pub fn hash_hex(&self) -> String {
        format!("{:016x}", self.snap.hash)
    }

    /// Number of live agents in the snapshot.
    pub fn agent_count(&self) -> u32 {
        self.snap.teams.len() as u32
    }

    /// Arena width in metres.
    pub fn arena_width_m(&self) -> u32 {
        self.session.arena_m().0
    }

    /// Arena height in metres.
    pub fn arena_height_m(&self) -> u32 {
        self.session.arena_m().1
    }

    /// Interleaved `x, y` positions in whole centimetres.
    pub fn positions_cm(&self) -> Vec<i32> {
        self.snap.positions_cm.clone()
    }

    /// Team per agent.
    pub fn teams(&self) -> Vec<u8> {
        self.snap.teams.clone()
    }

    /// Hit points per agent.
    pub fn hp(&self) -> Vec<u16> {
        self.snap.hp.clone()
    }

    /// Hit points every agent starts with (for shading).
    pub fn start_hp(&self) -> u16 {
        hd_sim::scenario::START_HP
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hd_sim::scenario::final_hash;

    #[test]
    fn snapshot_hash_matches_hd_sim_hash() {
        for (seed, agents, tick) in [(1u64, 100u32, 200u64), (7, 300, 123), (42, 50, 0)] {
            let mut s = Session::new(seed, agents).unwrap();
            s.advance_to(tick).unwrap();
            let want = final_hash(&ToyParams::new(agents, seed), tick).unwrap();
            assert_eq!(
                s.snapshot().hash,
                want,
                "seed {seed} agents {agents} tick {tick}"
            );
        }
    }

    #[test]
    fn incremental_advance_equals_direct() {
        let mut a = Session::new(3, 80).unwrap();
        a.advance_to(40).unwrap();
        a.advance_to(90).unwrap();
        let mut b = Session::new(3, 80).unwrap();
        b.advance_to(90).unwrap();
        assert_eq!(a.snapshot(), b.snapshot());
    }

    #[test]
    fn snapshot_shape_and_bounds() {
        let mut s = Session::new(5, 120).unwrap();
        s.advance_to(300).unwrap();
        let snap = s.snapshot();
        let n = snap.teams.len();
        assert_eq!(snap.positions_cm.len(), 2 * n);
        assert_eq!(snap.hp.len(), n);
        assert!(n <= 120);
        let (w, h) = s.arena_m();
        for p in snap.positions_cm.chunks(2) {
            assert!((0..=(w as i32) * 100).contains(&p[0]));
            assert!((0..=(h as i32) * 100).contains(&p[1]));
        }
        assert!(snap.teams.iter().all(|&t| t <= 1));
        assert!(snap
            .hp
            .iter()
            .all(|h| (1..=hd_sim::scenario::START_HP).contains(h)));
    }

    #[test]
    fn rewind_and_oversize_are_errors() {
        let mut s = Session::new(1, 10).unwrap();
        s.advance_to(10).unwrap();
        assert!(s.advance_to(9).is_err());
        assert!(Session::new(1, hd_sim::scenario::MAX_AGENTS + 1).is_err());
    }

    #[test]
    fn centimetre_conversion() {
        assert_eq!(to_cm(0), 0);
        assert_eq!(to_cm(1i64 << 32), 100);
        assert_eq!(to_cm(5i64 << 31), 250);
    }
}
