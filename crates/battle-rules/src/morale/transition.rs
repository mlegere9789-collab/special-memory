//! The five-state machine of SPEC-006 section 5.5 as a pure function: which state a regiment
//! moves to in one slow period, if any.

use super::table::{MoraleGlobals, MoraleKind};

/// What one regiment's transition depends on.
#[derive(Clone, Copy, Debug)]
pub struct TransitionIn {
    /// Current state.
    pub kind: MoraleKind,
    /// Cohesion, ppm.
    pub c: u32,
    /// Ticks since the state was entered.
    pub state_ticks: u32,
    /// Per-regiment offset added to every threshold, ppm.
    pub nerve_offset: i32,
    /// This period's roll allows a downward step.
    pub hit: bool,
    /// An enemy anchor is within the pressure radius.
    pub pressure: bool,
    /// At least one officer is alive.
    pub officers: bool,
    /// Strength of the leader in reach, permille.
    pub leader_pm: u16,
}

/// The state entered this period, or `None` to stay. At most one step is taken; every downward
/// step (to FallingBack or Broken) needs `hit`; thresholds are `X + nerve_offset`.
pub fn next_kind(g: &MoraleGlobals, t: &TransitionIn) -> Option<MoraleKind> {
    let c = i64::from(t.c);
    let at = |x: u32| i64::from(x) + i64::from(t.nerve_offset);
    let long = t.state_ticks;
    match t.kind {
        MoraleKind::Steady => (c < at(g.waver)).then_some(MoraleKind::Wavering),
        MoraleKind::Wavering => {
            if c >= at(g.steady) {
                Some(MoraleKind::Steady)
            } else {
                (c < at(g.fall) && t.hit).then_some(MoraleKind::FallingBack)
            }
        }
        MoraleKind::FallingBack => {
            if c < at(g.break_) && t.hit {
                Some(MoraleKind::Broken)
            } else {
                (c >= at(g.fall_exit) && long >= g.min_fall && !t.pressure)
                    .then_some(MoraleKind::Rallied)
            }
        }
        MoraleKind::Broken => {
            let led = t.officers || t.leader_pm > 0 || long >= g.officerless_rally;
            (c >= at(g.rally) && !t.pressure && long >= g.min_broken && led)
                .then_some(MoraleKind::Rallied)
        }
        MoraleKind::Rallied => {
            if c < at(g.fall) && t.hit {
                Some(MoraleKind::FallingBack)
            } else if long >= g.min_rallied {
                if c >= at(g.steady) {
                    Some(MoraleKind::Steady)
                } else if c >= at(g.fall) {
                    Some(MoraleKind::Wavering)
                } else {
                    None
                }
            } else {
                None
            }
        }
    }
}
