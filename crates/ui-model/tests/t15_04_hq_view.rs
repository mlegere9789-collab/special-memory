//! HQ view hides the truth: changing enemy positions with a fixed information source leaves
//! `snapshot_hash` unchanged (the snapshot half of T15-04).

mod common;

use battle_rules::Formation;
use common::{battle, metres, steady, Canned, CannedMorale};
use ui_model::{capture, snapshot_hash, InfoMode, Sighting};

fn hash_with_enemy_at(x: i32, y: i32) -> u64 {
    let b = battle(
        2,
        &[(0, 6, Formation::Line, 0, 0), (1, 6, Formation::Line, x, y)],
    );
    let info = Canned {
        sightings: vec![Sighting {
            unit: 1,
            last_seen_tick: 0,
            pos: metres(40, 40),
            pos_err_mm: 500,
            strength_lo: 0,
            strength_hi: 50,
            formation_known: true,
        }],
        orders: vec![],
    };
    snapshot_hash(&capture(
        &b,
        &info,
        &CannedMorale(steady()),
        InfoMode::Hq { side: 0 },
        0,
    ))
}

#[test]
fn moving_true_enemy_positions_does_not_change_the_hash() {
    let base = hash_with_enemy_at(60, 60);
    for (x, y) in [(0, 0), (200, 10), (333, 333), (7, 250)] {
        assert_eq!(hash_with_enemy_at(x, y), base);
    }
}
