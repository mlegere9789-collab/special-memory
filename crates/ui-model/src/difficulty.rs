//! The difficulty record (SPEC-015 section 10), as a struct only. The behaviour keys scale the
//! AI and the information model; no key scales a weapon, damage, ammunition, locomotion, drill
//! or terrain value. Its encoding, presets and checks belong to a later ticket.

/// Difficulty settings, in permille except `assists`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Difficulty {
    /// AI initiative.
    pub ai_initiative_pm: u16,
    /// AI risk tolerance.
    pub ai_risk_pm: u16,
    /// Scale of sighting error.
    pub info_noise_pm: u16,
    /// Scale of courier transit spread.
    pub courier_spread_pm: u16,
    /// Assist bits: 0 ETA window, 1 auto-pause, 2 order preview, 3 full view, 4 historical
    /// restrictions.
    pub assists: u8,
}
