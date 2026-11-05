//! From the simulation's sound log to sound events (SPEC-012 section 4, amendment A10).

use crate::aggregate::SoundEvent;
use battle_rules::{AmmoKind, AmmoTable, FireRecord, FireRecordKind};

/// Source rows of `data/rules/acoustics.toml` the adapter picks from.
const MUSKET: u8 = 0;
const CARBINE: u8 = 1;
const REVOLVER: u8 = 2;
const GUN_LAUNCH: u8 = 3;
const BURST_SHELL: u8 = 4;
const BURST_CASE: u8 = 5;

/// The sound of one fire record, or `None` when the record makes none: an ammunition number
/// outside `ammo`, a canister burst, or a burst of a round that does not burst.
///
/// A shot of a ball or conical round is a musket, except that the ammunition ids `carbine_52`
/// and `revolver_44` use their own rows; a shot of any shot, shell, case or canister round is a gun
/// launch. A shell burst and a case burst have their own rows. The event stands for one sound; the
/// height is `z_mm` clamped to `0..=65535`.
pub fn adapt_fire(r: &FireRecord, ammo: &AmmoTable) -> Option<SoundEvent> {
    let row = ammo.rows().get(usize::from(r.ammo))?;
    let source = match (r.kind, row.kind) {
        (FireRecordKind::Shot, AmmoKind::Ball | AmmoKind::Conical) => match row.id.as_str() {
            "carbine_52" => CARBINE,
            "revolver_44" => REVOLVER,
            _ => MUSKET,
        },
        (FireRecordKind::Shot, _) => GUN_LAUNCH,
        (FireRecordKind::Burst, AmmoKind::Shell) => BURST_SHELL,
        (FireRecordKind::Burst, AmmoKind::Case) => BURST_CASE,
        (FireRecordKind::Burst, _) => return None,
    };
    Some(SoundEvent {
        seq: r.seq,
        source,
        unit: r.unit,
        team: r.team,
        pos: r.pos,
        height_mm: r.z_mm.clamp(0, i32::from(u16::MAX)) as u16,
        count: 1,
    })
}
