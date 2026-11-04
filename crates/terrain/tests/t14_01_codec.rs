//! T14-01: tile codec round trip, every rejection of SPEC-014 section 4.1, fuzzed bytes, and
//! byte-equal encodes.
mod common;

use proptest::prelude::*;
use std::collections::BTreeMap;
use terrain::{fnv1a64, ObstacleSeg, TerrainError, Tile, TileId};

const HEADER: usize = 17;

fn seal(bytes: &mut [u8]) {
    let n = bytes.len() - 8;
    let h = fnv1a64(&bytes[..n]);
    bytes[n..].copy_from_slice(&h.to_le_bytes());
}

/// `(id, offset of the id byte, payload start, payload length)` per section.
fn sections(bytes: &[u8]) -> Vec<(u8, usize, usize, usize)> {
    let mut out = Vec::new();
    let mut at = HEADER;
    while at + 5 <= bytes.len() - 8 {
        let len = u32::from_le_bytes(bytes[at + 1..at + 5].try_into().unwrap()) as usize;
        out.push((bytes[at], at, at + 5, len));
        at += 5 + len;
    }
    out
}

fn is_format(r: Result<Tile, TerrainError>) -> bool {
    matches!(r, Err(TerrainError::Format(_)))
}

fn seg(id: u32) -> ObstacleSeg {
    ObstacleSeg {
        id,
        kind: (id % 6) as u8,
        flags: 1,
        height_mm: 1000 + id as u16,
        a: [-5 * id as i32, 7],
        b: [100, -9 * id as i32],
    }
}

fn tile_with_obstacles() -> Tile {
    let mut t = common::synth_tile(1, 1);
    t.set_obstacles(vec![seg(2), seg(9), seg(40)]).unwrap();
    t
}

#[test]
fn fnv_known_vectors() {
    assert_eq!(fnv1a64(b""), 0xcbf2_9ce4_8422_2325);
    assert_eq!(fnv1a64(b"a"), 0xaf63_dc4c_8601_ec8c);
    assert_eq!(fnv1a64(b"foobar"), 0x8594_4171_f739_67e8);
}

#[test]
fn round_trip_and_equal_encodes() {
    let t = tile_with_obstacles();
    let a = t.encode();
    assert_eq!(a, t.encode());
    let back = Tile::decode(&a).unwrap();
    assert_eq!(back, t);
    assert_eq!(back.id(), TileId { x: 1, y: 1 });
    assert_eq!(back.encode(), a);
    assert_eq!(
        t.hash(),
        u64::from_le_bytes(a[a.len() - 8..].try_into().unwrap())
    );
    // Header layout of section 4.1.
    assert_eq!(&a[..4], b"HDTL");
    assert_eq!(&a[4..6], &1u16.to_le_bytes());
    assert_eq!(a[16], 5);
    let ids: Vec<u8> = sections(&a).iter().map(|s| s.0).collect();
    assert_eq!(ids, vec![1, 2, 3, 4, 5]);
    let lens: Vec<usize> = sections(&a).iter().map(|s| s.3).collect();
    assert_eq!(
        lens,
        vec![129 * 129 * 2, 128 * 128, 128 * 128, 128 * 128, 4 + 3 * 24]
    );
}

#[test]
fn segment_endpoints_are_millimetres() {
    let (a, b) = seg(3).endpoints();
    assert_eq!(terrain::fx_to_mm(a.x), -15);
    assert_eq!(terrain::fx_to_mm(a.y), 7);
    assert_eq!(terrain::fx_to_mm(b.x), 100);
    assert_eq!(terrain::fx_to_mm(b.y), -27);
}

fn arb_tile() -> impl Strategy<Value = Tile> {
    (
        any::<u16>(),
        any::<u16>(),
        prop::collection::vec(any::<u16>(), 129 * 129),
        prop::collection::vec(any::<u8>(), 128 * 128),
        prop::collection::vec(any::<u8>(), 128 * 128),
        prop::collection::vec(any::<u8>(), 128 * 128),
        prop::collection::btree_map(
            any::<u32>(),
            (
                any::<u8>(),
                any::<u8>(),
                any::<u16>(),
                any::<[i32; 2]>(),
                any::<[i32; 2]>(),
            ),
            0..12,
        ),
    )
        .prop_map(|(x, y, h, c, o, f, obs)| {
            let obs: BTreeMap<_, _> = obs;
            let obs = obs
                .into_iter()
                .map(|(id, (kind, flags, height_mm, a, b))| ObstacleSeg {
                    id,
                    kind,
                    flags,
                    height_mm,
                    a,
                    b,
                })
                .collect();
            Tile::from_parts(TileId { x, y }, h, c, o, f, obs).unwrap()
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]

    #[test]
    fn prop_round_trip(t in arb_tile()) {
        let bytes = t.encode();
        let back = Tile::decode(&bytes).unwrap();
        prop_assert_eq!(&back, &t);
        prop_assert_eq!(back.encode(), bytes);
    }

    #[test]
    fn prop_any_flipped_byte_is_rejected(pos in 0usize..66_000, bit in 0u8..8) {
        let mut bytes = tile_with_obstacles().encode();
        let at = pos % bytes.len();
        bytes[at] ^= 1 << bit;
        prop_assert!(Tile::decode(&bytes).is_err());
    }

    #[test]
    fn prop_random_bytes_never_panic(bytes in prop::collection::vec(any::<u8>(), 0..400)) {
        let _ = Tile::decode(&bytes);
    }

    #[test]
    fn prop_random_tail_after_valid_header_never_panics(tail in prop::collection::vec(any::<u8>(), 0..300)) {
        let good = tile_with_obstacles().encode();
        let mut bytes = good[..HEADER].to_vec();
        bytes.extend_from_slice(&tail);
        let _ = Tile::decode(&bytes);
        let mut bytes = good[..HEADER + 5].to_vec();
        bytes.extend_from_slice(&tail);
        let _ = Tile::decode(&bytes);
    }

    #[test]
    fn prop_resealed_garbage_sections_never_panic(
        id in 0usize..5, cut in 0usize..40, fill in prop::collection::vec(any::<u8>(), 0..40)
    ) {
        // Garbage inside one section, with a valid hash, so the content checks run.
        let mut bytes = tile_with_obstacles().encode();
        let (_, _, start, len) = sections(&bytes)[id];
        for (i, b) in fill.iter().enumerate() {
            let at = start + (cut + i) % len;
            bytes[at] = *b;
        }
        seal(&mut bytes);
        let _ = Tile::decode(&bytes);
    }
}

#[test]
fn rejects_bad_header_fields() {
    let good = tile_with_obstacles().encode();
    let cases: [(&str, usize, Vec<u8>); 5] = [
        ("magic", 0, b"HDTX".to_vec()),
        ("version", 4, 2u16.to_le_bytes().to_vec()),
        ("cells", 10, 64u16.to_le_bytes().to_vec()),
        ("cell_mm", 12, 2000u32.to_le_bytes().to_vec()),
        ("section count", 16, vec![4]),
    ];
    for (name, at, new) in cases {
        let mut b = good.clone();
        b[at..at + new.len()].copy_from_slice(&new);
        seal(&mut b);
        assert!(is_format(Tile::decode(&b)), "{name}");
    }
}

#[test]
fn rejects_truncation_at_every_boundary() {
    let good = tile_with_obstacles().encode();
    let mut cuts: Vec<usize> = (0..60).collect();
    for (_, at, start, len) in sections(&good) {
        cuts.extend([
            at,
            at + 1,
            at + 4,
            start,
            start + 1,
            start + len - 1,
            start + len,
        ]);
    }
    cuts.extend([good.len() - 9, good.len() - 8, good.len() - 1]);
    for cut in cuts {
        assert!(cut < good.len());
        assert!(is_format(Tile::decode(&good[..cut])), "cut {cut}");
        // The same cut with a fresh hash appended is still malformed, except the cut that
        // removes exactly the hash.
        if cut != good.len() - 8 {
            let mut b = good[..cut].to_vec();
            b.extend_from_slice(&[0; 8]);
            seal(&mut b);
            assert!(Tile::decode(&b).is_err(), "resealed cut {cut}");
        }
    }
}

#[test]
fn rejects_trailing_bytes() {
    let good = tile_with_obstacles().encode();
    let mut after = good.clone();
    after.push(0);
    assert!(is_format(Tile::decode(&after)));
    let mut inside = good.clone();
    inside.insert(good.len() - 8, 0);
    seal(&mut inside);
    assert!(is_format(Tile::decode(&inside)));
}

#[test]
fn rejects_missing_duplicate_and_misordered_sections() {
    let good = tile_with_obstacles().encode();
    let secs = sections(&good);
    // Missing section 3 (count fixed up, hash resealed).
    let (_, at, start, len) = secs[2];
    let mut missing = good.clone();
    missing.drain(at..start + len);
    missing[16] = 4;
    seal(&mut missing);
    assert!(is_format(Tile::decode(&missing)));
    // Section 3 dropped but the count still says five.
    let mut short = good.clone();
    short.drain(at..start + len);
    seal(&mut short);
    assert!(is_format(Tile::decode(&short)));
    // Duplicate: section 3 labelled 2.
    let mut dup = good.clone();
    dup[at] = 2;
    seal(&mut dup);
    assert!(is_format(Tile::decode(&dup)));
    // Out of order: ids 3 and 4 swapped.
    let mut order = good.clone();
    order[secs[2].1] = 4;
    order[secs[3].1] = 3;
    seal(&mut order);
    assert!(is_format(Tile::decode(&order)));
    // Unknown id.
    let mut unknown = good.clone();
    unknown[secs[0].1] = 9;
    seal(&mut unknown);
    assert!(is_format(Tile::decode(&unknown)));
}

#[test]
fn rejects_wrong_section_lengths() {
    let good = tile_with_obstacles().encode();
    for (i, (_, at, _, len)) in sections(&good).into_iter().enumerate() {
        for new_len in [0u32, len as u32 - 1, len as u32 + 1, u32::MAX] {
            let mut b = good.clone();
            b[at + 1..at + 5].copy_from_slice(&new_len.to_le_bytes());
            seal(&mut b);
            assert!(is_format(Tile::decode(&b)), "section {i} length {new_len}");
        }
    }
}

#[test]
fn rejects_bad_obstacle_tables() {
    let good = tile_with_obstacles().encode();
    let (_, _, start, len) = sections(&good)[4];
    let rec = |i: usize| start + 4 + i * 24;
    // Count says more records than the section holds, including a huge count.
    for n in [4u32, 1000, u32::MAX] {
        let mut b = good.clone();
        b[start..start + 4].copy_from_slice(&n.to_le_bytes());
        seal(&mut b);
        assert!(is_format(Tile::decode(&b)), "count {n}");
    }
    // Count smaller than the data present.
    let mut b = good.clone();
    b[start..start + 4].copy_from_slice(&2u32.to_le_bytes());
    seal(&mut b);
    assert!(is_format(Tile::decode(&b)));
    // Unsorted: swap the ids of records 0 and 1.
    let mut unsorted = good.clone();
    let (id0, id1) = (seg(2).id.to_le_bytes(), seg(9).id.to_le_bytes());
    unsorted[rec(0)..rec(0) + 4].copy_from_slice(&id1);
    unsorted[rec(1)..rec(1) + 4].copy_from_slice(&id0);
    seal(&mut unsorted);
    assert!(is_format(Tile::decode(&unsorted)));
    // Duplicate ids.
    let mut dup = good.clone();
    dup[rec(1)..rec(1) + 4].copy_from_slice(&id0);
    seal(&mut dup);
    assert!(is_format(Tile::decode(&dup)));
    assert_eq!(len, 4 + 3 * 24);
    // The writer side refuses the same inputs.
    let mut t = common::synth_tile(0, 0);
    assert!(t.set_obstacles(vec![seg(9), seg(2)]).is_err());
    assert!(t.set_obstacles(vec![seg(2), seg(2)]).is_err());
}

#[test]
fn rejects_hash_mismatch() {
    let good = tile_with_obstacles().encode();
    let n = good.len();
    let mut payload = good.clone();
    payload[sections(&good)[0].2 + 100] ^= 0x40;
    assert_eq!(Tile::decode(&payload), Err(TerrainError::Hash));
    let mut hash = good.clone();
    hash[n - 1] ^= 1;
    assert_eq!(Tile::decode(&hash), Err(TerrainError::Hash));
    // A tile id edit in the header without a new hash is a hash failure too.
    let mut id = good;
    id[6] ^= 1;
    assert_eq!(Tile::decode(&id), Err(TerrainError::Hash));
}

#[test]
fn from_parts_checks_lengths() {
    let id = TileId { x: 0, y: 0 };
    let ok = |h: usize, c: usize, o: usize, f: usize| {
        Tile::from_parts(id, vec![0; h], vec![0; c], vec![0; o], vec![0; f], vec![]).is_ok()
    };
    assert!(ok(16641, 16384, 16384, 16384));
    assert!(!ok(16640, 16384, 16384, 16384));
    assert!(!ok(16641, 16383, 16384, 16384));
    assert!(!ok(16641, 16384, 16385, 16384));
    assert!(!ok(16641, 16384, 16384, 0));
}
