//! `snapshot_hash` golden for a fixed battle. The golden file lists every snapshot field as plain
//! integers plus the hash; `tools/validate_ui.py` re-derives the hash from those integers with its
//! own encoder, so the Rust encoding and the script check each other.

mod common;

use battle_rules::{battle_hash, Formation, Order, RegimentId};
use common::{battle, metres, Canned, CannedMorale};
use std::fmt::Write;
use ui_model::{capture, snapshot_hash, InfoMode, MoraleReport, OrderRecord, Sighting, Snapshot};

fn order_pair(o: &Order) -> (u8, u8) {
    match o {
        Order::Hold => (1, 0),
        Order::Form(f) => (3, f.tag()),
        other => panic!("the golden uses Hold and Form only, got {other:?}"),
    }
}

fn opt(v: Option<u64>) -> i128 {
    v.map_or(-1, i128::from)
}

fn dump(s: &Snapshot, hash: u64) -> String {
    let mut t = String::new();
    writeln!(t, "hash = \"{hash:016x}\"").unwrap();
    writeln!(
        t,
        "tick = {}\nclock_s = {}\nside = {}\nfull_view = {}",
        s.tick,
        s.clock_s,
        s.side,
        u8::from(s.full_view)
    )
    .unwrap();
    for r in &s.regiments {
        let m = &r.morale;
        let factors: Vec<String> = m
            .factors
            .iter()
            .map(|(i, c)| format!("[{i}, {c}]"))
            .collect();
        writeln!(
            t,
            "\n[[regiment]]\nid = {}\nside = {}\npos_x = {}\npos_y = {}\nheading = {}",
            r.id,
            r.side,
            r.pos.x.raw(),
            r.pos.y.raw(),
            r.heading.raw()
        )
        .unwrap();
        writeln!(t, "formation = {}\nstrength = {}\norder = {}\nlevel = {}\nmargin = {}\ntrend = {}\nfactors = [{}]", r.formation, r.strength, r.order, m.level, m.margin, m.trend, factors.join(", ")).unwrap();
    }
    for e in &s.enemies {
        writeln!(
            t,
            "\n[[enemy]]\nunit = {}\nlast_seen = {}\npos_x = {}\npos_y = {}\nerr = {}",
            e.unit,
            e.last_seen_tick,
            e.pos.x.raw(),
            e.pos.y.raw(),
            e.pos_err_mm
        )
        .unwrap();
        writeln!(
            t,
            "lo = {}\nhi = {}\nformation_known = {}",
            e.strength_lo,
            e.strength_hi,
            u8::from(e.formation_known)
        )
        .unwrap();
    }
    for o in &s.orders {
        let ((tag, arg), interp) = (order_pair(&o.order), o.interpreted.as_ref().map(order_pair));
        writeln!(
            t,
            "\n[[order]]\nid = {}\nregiment = {}\ntag = {tag}\narg = {arg}\nsent = {}",
            o.id,
            o.regiment.0,
            opt(o.sent_tick)
        )
        .unwrap();
        writeln!(
            t,
            "eta_lo = {}\neta_hi = {}\neta_hi0 = {}\nack = {}",
            o.eta_lo,
            o.eta_hi,
            o.eta_hi0,
            opt(o.ack_tick)
        )
        .unwrap();
        let (itag, iarg) = interp.unwrap_or((0, 0));
        writeln!(t, "interp_tag = {itag}\ninterp_arg = {iarg}\nexecuted = {}\ncompleted = {}\nlost_known = {}", opt(o.executed_tick), opt(o.completed_tick), u8::from(o.lost_known)).unwrap();
    }
    t
}

fn fixed() -> (Snapshot, u64) {
    let mut b = battle(
        5,
        &[
            (0, 10, Formation::Column, 0, 0),
            (1, 10, Formation::Line, 0, 120),
        ],
    );
    (0..40).for_each(|_| b.step());
    let info = Canned {
        sightings: vec![Sighting {
            unit: 1,
            last_seen_tick: 12,
            pos: metres(3, 118),
            pos_err_mm: 4500,
            strength_lo: 130,
            strength_hi: 260,
            formation_known: false,
        }],
        orders: vec![OrderRecord {
            id: 7,
            regiment: RegimentId(0),
            order: Order::Hold,
            sent_tick: Some(10),
            eta_lo: 60,
            eta_hi: 90,
            eta_hi0: 80,
            ack_tick: Some(35),
            interpreted: Some(Order::Form(Formation::Line)),
            executed_tick: None,
            completed_tick: None,
            lost_known: false,
        }],
    };
    let morale = CannedMorale(MoraleReport {
        level: 2,
        margin: -7,
        trend: -1,
        factors: vec![(3, -120), (1, 45)],
    });
    let s = capture(&b, &info, &morale, InfoMode::Hq { side: 0 }, 36_000);
    let h = snapshot_hash(&s);
    assert_ne!(battle_hash(&b), 0);
    (s, h)
}

#[test]
fn fixed_battle_matches_the_golden() {
    let (s, h) = fixed();
    let want = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/golden/snapshot.toml"
    ))
    .unwrap_or_default();
    let got = dump(&s, h);
    assert!(got == want, "golden mismatch, current dump follows\n{got}");
}
