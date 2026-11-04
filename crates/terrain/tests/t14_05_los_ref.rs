//! T14-05 (synthetic pack): `los` equals the independent reference `qc/fixtures/ref_los.py`
//! (committed output `qc/fixtures/ref_los.csv`) on 1,000 seeded pairs in both modes.
mod common;

use common::lospack::{los_world, read_csv};
use common::pos;
use terrain::{Blocker, LosMode};

#[test]
fn los_matches_the_reference_on_seeded_pairs() {
    let w = los_world();
    let rows = read_csv("ref_los.csv");
    assert_eq!(rows.len(), 2000);
    let mut counts = [0usize; 4];
    for (i, v) in rows.iter().enumerate() {
        let mode = if v[6] == 0 {
            LosMode::Fire
        } else {
            LosMode::Sight
        };
        let r = w
            .los(pos(v[0], v[1]), v[2], pos(v[3], v[4]), v[5], mode)
            .unwrap();
        let (blocker, seg) = match r.blocker {
            Blocker::None => (0, 0),
            Blocker::Ground => (1, 0),
            Blocker::Occluder => (2, 0),
            Blocker::Obstacle(id) => (3, i64::from(id)),
        };
        counts[blocker as usize] += 1;
        let got = [i64::from(r.clear), r.blocked_at_mm, blocker, seg];
        assert_eq!(got, [v[7], v[8], v[9], v[10]], "row {i}: {v:?}");
    }
    // The fixture exercises every outcome.
    assert!(counts.iter().all(|&c| c > 20), "{counts:?}");
}
