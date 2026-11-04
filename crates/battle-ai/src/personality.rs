//! Personality as utility weights (SPEC-010 section 4): a base plus the deltas of the traits a
//! commander record documents, each weighted by its confidence.

use crate::tables::{AiTables, RelationKind};

/// The personality of one commander. Shares are permille; ratios are own:enemy in permille
/// (1500 = 1.5).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Personality {
    /// Willingness to accept risk, permille.
    pub risk_tolerance: u16,
    /// Share of a corps held back, permille.
    pub reserve: u16,
    /// Own:enemy ratio needed to attack.
    pub assault_ratio: u16,
    /// Below this own:enemy ratio a position is given up.
    pub withdraw_ratio: u16,
    /// Weight on prepared ground, permille.
    pub entrench: u16,
    /// Initiative radius share, permille.
    pub discretion: u16,
    /// Extra ticks before an order is submitted.
    pub reaction_ticks: u32,
    /// `risk_tolerance` for cavalry seats (differs only when `cavalry_flair` is documented).
    pub cavalry_risk_tolerance: u16,
    /// Commanders this one feuds or rivals with, ascending, without repeats.
    pub feuds: Vec<String>,
}

impl Personality {
    /// The risk tolerance of a seat: the cavalry variant for a cavalry seat.
    pub fn risk_for(&self, cavalry: bool) -> u16 {
        if cavalry {
            self.cavalry_risk_tolerance
        } else {
            self.risk_tolerance
        }
    }

    /// Whether this seat may commit its reserve to the sector of `partner`'s command. A feud or
    /// rivalry bars it unless `sector_ratio_pm` (own:enemy there) is below `withdraw_ratio / 2`.
    pub fn reserve_commit_allowed(&self, partner: &str, sector_ratio_pm: u32) -> bool {
        let feud = self.feuds.iter().any(|p| p == partner);
        !feud || sector_ratio_pm < u32::from(self.withdraw_ratio) / 2
    }
}

/// `clamp(base + sum of terms, 0, max)` for one value; each term is already truncated.
fn clamped(base: u32, terms: i32, max: u32) -> u32 {
    let v = i64::from(base) + i64::from(terms);
    u32::try_from(v.clamp(0, i64::from(max))).unwrap_or(0)
}

/// Builds the personality of `commander` from the traits its record documents.
///
/// Each trait contributes `weight(confidence) * delta / 1000` per value, truncated toward zero
/// per trait; the terms are summed, added to the base and clamped. A commander without traits
/// gets the base personality. `cavalry_flair` enters only the cavalry risk value.
pub fn personality_for(commander: &str, tables: &AiTables) -> Personality {
    let mut sums = [0i32; 7];
    let mut cavalry_risk = 0i32;
    for fact in tables.trait_facts(commander) {
        let row = tables.trait_row(fact.id);
        let weight = i64::from(tables.weight_pm(fact.provenance.confidence));
        let term = |delta: i32| i32::try_from(weight * i64::from(delta) / 1000).unwrap_or(0);
        if row.cavalry_only {
            cavalry_risk += term(row.risk);
            continue;
        }
        for (sum, delta) in sums.iter_mut().zip(row.deltas()) {
            *sum += term(delta);
        }
    }
    let b = &tables.base;
    let bounds = &tables.bounds;
    let share = u32::from(bounds.share_max_pm);
    let ratio = u32::from(bounds.ratio_max_pm);
    let narrow = |v: u32| u16::try_from(v).unwrap_or(u16::MAX);
    let risk = clamped(u32::from(b.risk_tolerance), sums[0], share);
    let mut feuds: Vec<String> = tables
        .relation_facts(commander)
        .filter(|r| matches!(r.kind, RelationKind::Feud | RelationKind::Rivalry))
        .map(|r| r.partner.clone())
        .collect();
    feuds.sort();
    feuds.dedup();
    Personality {
        risk_tolerance: narrow(risk),
        reserve: narrow(clamped(u32::from(b.reserve), sums[1], share)),
        assault_ratio: narrow(clamped(u32::from(b.assault_ratio), sums[2], ratio)),
        withdraw_ratio: narrow(clamped(u32::from(b.withdraw_ratio), sums[3], ratio)),
        entrench: narrow(clamped(u32::from(b.entrench), sums[4], share)),
        discretion: narrow(clamped(u32::from(b.discretion), sums[5], share)),
        reaction_ticks: clamped(b.reaction_ticks, sums[6], bounds.reaction_max_ticks),
        cavalry_risk_tolerance: narrow(clamped(
            u32::from(b.risk_tolerance),
            sums[0] + cavalry_risk,
            share,
        )),
        feuds,
    }
}
