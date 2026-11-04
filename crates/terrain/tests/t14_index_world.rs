//! Index parsing and hashing (SPEC-014 section 4.2) and residency, insertion order and tile
//! enumeration of `TerrainWorld` (sections 8 and 11).
mod common;

use common::*;
use terrain::{fnv1a64, TerrainError, TerrainIndex, TerrainWorld, TileId};

fn tid(x: u16, y: u16) -> TileId {
    TileId { x, y }
}

fn is_index_err(text: &str) -> bool {
    matches!(
        TerrainIndex::from_toml_str(text),
        Err(TerrainError::Index(_))
    )
}

#[test]
fn index_fields_and_extras_parse() {
    let mut text = index_toml(&synth_tiles(), TILES_X, TILES_Y);
    text.push_str("\n# comment\n[[sector]]\nid = \"south\"\ntiles = [\"0,0\", \"1,0\", \"2,0\"]\n");
    text.push_str("\n[[slab]]\nid = \"first\"\nx0 = 0\ny0 = 0\nx1 = 1\ny1 = 1\n");
    let ix = TerrainIndex::from_toml_str(&text).unwrap();
    assert_eq!((ix.cell_mm(), ix.tile_cells()), (4000, 128));
    assert_eq!((ix.tiles_x(), ix.tiles_y()), (3, 2));
    assert_eq!(
        (ix.origin_easting_mm(), ix.origin_northing_mm()),
        (300_000_000, 4_400_000_000)
    );
    assert_eq!(ix.z_origin_cm(), 0);
    assert_eq!(ix.crs(), "UTM 18N");
    assert_eq!(ix.legend().get(&1).map(String::as_str), Some("road"));
    assert_eq!(ix.sectors().len(), 1);
    assert_eq!(ix.sectors()[0].tiles, vec![tid(0, 0), tid(1, 0), tid(2, 0)]);
    assert_eq!((ix.slabs()[0].id.as_str(), ix.slabs()[0].x1), ("first", 1));
    let t = synth_tile(2, 1);
    assert_eq!(ix.tile_hash(tid(2, 1)), Some(t.hash()));
    assert_eq!(ix.tile_hash(tid(2, 2)), None);
}

#[test]
fn content_hash_follows_the_specified_byte_layout() {
    let tiles = synth_tiles();
    let ix = synth_index();
    let mut bytes = Vec::new();
    for v in [4000u64, 3, 2] {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    bytes.extend_from_slice(&0i64.to_le_bytes());
    // Tiles are listed (y, x) ascending by construction of `synth_tiles`.
    for t in &tiles {
        bytes.extend_from_slice(&t.hash().to_le_bytes());
    }
    assert_eq!(ix.content_hash(), fnv1a64(&bytes));
}

#[test]
fn content_hash_ignores_row_order_and_sees_every_input() {
    let base = synth_index().content_hash();
    let tiles = synth_tiles();
    let text = index_toml(&tiles, 3, 2);
    // Shuffle the tile rows.
    let mut rev = tiles.clone();
    rev.reverse();
    assert_eq!(
        TerrainIndex::from_toml_str(&index_toml(&rev, 3, 2))
            .unwrap()
            .content_hash(),
        base
    );
    // Change z origin, extent, and one tile.
    let z = text.replace("z_origin_cm = 0", "z_origin_cm = 5");
    assert_ne!(
        TerrainIndex::from_toml_str(&z).unwrap().content_hash(),
        base
    );
    let wider = index_toml(&tiles, 4, 2);
    assert_ne!(
        TerrainIndex::from_toml_str(&wider).unwrap().content_hash(),
        base
    );
    let mut changed = tiles;
    changed[4].heights_mut()[5] ^= 1;
    assert_ne!(
        TerrainIndex::from_toml_str(&index_toml(&changed, 3, 2))
            .unwrap()
            .content_hash(),
        base
    );
}

#[test]
fn bad_indexes_are_refused() {
    let good = index_toml(&synth_tiles(), 3, 2);
    assert!(TerrainIndex::from_toml_str(&good).is_ok());
    let h0 = format!("hash_hex = \"{:016x}\"", synth_tile(0, 0).hash());
    let cases: Vec<(&str, String)> = vec![
        ("schema", good.replace("terrain-index/1", "terrain-index/2")),
        (
            "float",
            good.replace("z_origin_cm = 0", "z_origin_cm = 1.5"),
        ),
        (
            "exponent",
            good.replace("z_origin_cm = 0", "z_origin_cm = 1e2"),
        ),
        ("cell_mm", good.replace("cell_mm = 4000", "cell_mm = 8000")),
        (
            "tile_cells",
            good.replace("tile_cells = 128", "tile_cells = 64"),
        ),
        ("zero extent", good.replace("tiles_x = 3", "tiles_x = 0")),
        (
            "unknown key",
            good.replace("z_origin_cm = 0", "z_origin_cm = 0\nextra = 1"),
        ),
        ("missing key", good.replace("crs = \"UTM 18N\"\n", "")),
        (
            "duplicate key",
            good.replace("z_origin_cm = 0", "z_origin_cm = 0\nz_origin_cm = 0"),
        ),
        (
            "no equals",
            good.replace("z_origin_cm = 0", "z_origin_cm 0"),
        ),
        ("unknown table", format!("{good}\n[[mystery]]\nx = 1\n")),
        (
            "unterminated string",
            good.replace("\"UTM 18N\"", "\"UTM 18N"),
        ),
        ("bad hex", good.replace(&h0, "hash_hex = \"zz\"")),
        ("empty hex", good.replace(&h0, "hash_hex = \"\"")),
        (
            "hex too long",
            good.replace(
                &h0,
                &format!("hash_hex = \"1{:016x}\"", synth_tile(0, 0).hash()),
            ),
        ),
        (
            "tile outside extent",
            format!("{good}\n[[tile]]\nx = 3\ny = 0\nhash_hex = \"00\"\n"),
        ),
        (
            "duplicate tile",
            format!("{good}\n[[tile]]\nx = 0\ny = 0\nhash_hex = \"00\"\n"),
        ),
        (
            "negative tile",
            format!("{good}\n[[tile]]\nx = -1\ny = 0\nhash_hex = \"00\"\n"),
        ),
        (
            "tile missing field",
            format!("{good}\n[[tile]]\nx = 0\ny = 1\n"),
        ),
        (
            "bad legend key",
            good.replace("0 = \"open\"", "300 = \"open\""),
        ),
        (
            "bad sector tile",
            format!("{good}\n[[sector]]\nid = \"s\"\ntiles = [\"9,9\"]\n"),
        ),
        ("empty text", String::new()),
    ];
    for (name, text) in cases {
        assert!(is_index_err(&text), "{name}");
    }
}

fn some_world(order: &[(u16, u16)]) -> TerrainWorld {
    let mut w = TerrainWorld::new(synth_index());
    for &(x, y) in order {
        assert_eq!(
            w.insert_tile(&synth_tile(x, y).encode()).unwrap(),
            tid(x, y)
        );
    }
    w
}

#[test]
fn residency_insert_evict_and_listing() {
    let mut w = some_world(&[(2, 1), (0, 0), (1, 1), (2, 0)]);
    assert!(w.is_resident(tid(2, 1)) && !w.is_resident(tid(1, 0)));
    assert_eq!(
        w.resident_ids(),
        vec![tid(0, 0), tid(2, 0), tid(1, 1), tid(2, 1)]
    );
    assert!(w.evict_tile(tid(0, 0)));
    assert!(!w.evict_tile(tid(0, 0)));
    assert!(!w.evict_tile(tid(9, 9)));
    assert_eq!(w.resident_ids(), vec![tid(2, 0), tid(1, 1), tid(2, 1)]);
    // Inserting a resident tile again keeps one copy.
    w.insert_tile(&synth_tile(2, 0).encode()).unwrap();
    assert_eq!(w.resident_ids().len(), 3);
}

#[test]
fn tile_ids_order_by_row_then_column() {
    let mut ids = vec![tid(1, 1), tid(5, 0), tid(0, 1), tid(0, 0), tid(65535, 0)];
    ids.sort();
    assert_eq!(
        ids,
        vec![tid(0, 0), tid(5, 0), tid(65535, 0), tid(0, 1), tid(1, 1)]
    );
}

#[test]
fn insert_refuses_bad_tiles() {
    let mut w = TerrainWorld::new(synth_index());
    assert!(matches!(
        w.insert_tile(&[1, 2, 3]),
        Err(TerrainError::Format(_))
    ));
    // A well-formed tile outside the extent.
    let outside = synth_tile(3, 0).encode();
    assert_eq!(w.insert_tile(&outside), Err(TerrainError::OutOfExtent));
    // A well-formed tile whose content differs from the index entry.
    let mut other = synth_tile(1, 0);
    other.heights_mut()[0] ^= 1;
    assert_eq!(w.insert_tile(&other.encode()), Err(TerrainError::Hash));
    // A tile the index does not list.
    let partial = TerrainIndex::from_toml_str(&index_toml(&synth_tiles()[..2], 3, 2)).unwrap();
    let mut w = TerrainWorld::new(partial);
    assert!(matches!(
        w.insert_tile(&synth_tile(2, 1).encode()),
        Err(TerrainError::Index(_))
    ));
    assert!(w.resident_ids().is_empty());
}

#[test]
fn missing_tiles_are_reported_and_residency_never_changes_an_answer() {
    let full = synth_world();
    let one = some_world(&[(1, 0)]);
    let mut s = Sm(99);
    let (mut hits, mut misses) = (0, 0);
    for _ in 0..4000 {
        let (x, y) = (
            s.below(EXT_X_MM as u64) as i64,
            s.below(EXT_Y_MM as u64) as i64,
        );
        let p = pos(x, y);
        let tile = tid((x / 512_000) as u16, (y / 512_000) as u16);
        match one.height_mm(p) {
            Ok(h) => {
                hits += 1;
                assert_eq!(tile, tid(1, 0));
                assert_eq!(h, full.height_mm(p).unwrap());
                assert_eq!(one.class_at(p).unwrap(), full.class_at(p).unwrap());
                assert_eq!(one.occ_mm(p).unwrap(), full.occ_mm(p).unwrap());
                assert_eq!(one.flags_at(p).unwrap(), full.flags_at(p).unwrap());
                assert_eq!(one.slope_pm(p).unwrap(), full.slope_pm(p).unwrap());
            }
            Err(e) => {
                misses += 1;
                assert_eq!(e, TerrainError::TileMissing(tile));
                assert_eq!(one.class_at(p), Err(TerrainError::TileMissing(tile)));
                assert_eq!(one.occ_mm(p), Err(TerrainError::TileMissing(tile)));
                assert_eq!(one.flags_at(p), Err(TerrainError::TileMissing(tile)));
                assert_eq!(one.slope_pm(p), Err(TerrainError::TileMissing(tile)));
            }
        }
    }
    assert!(hits > 100 && misses > 100);
    // A clamped position reports the edge tile it clamps to.
    assert_eq!(
        one.class_at(pos(-5, -5)),
        Err(TerrainError::TileMissing(tid(0, 0)))
    );
    assert_eq!(
        one.class_at(pos(EXT_X_MM + 5, EXT_Y_MM + 5)),
        Err(TerrainError::TileMissing(tid(2, 1)))
    );
}

#[test]
fn insertion_and_eviction_order_never_change_an_answer() {
    let a = some_world(&[(0, 0), (1, 0), (2, 0), (0, 1), (1, 1), (2, 1)]);
    let mut b = some_world(&[(2, 1), (1, 1), (0, 1), (2, 0), (1, 0), (0, 0)]);
    b.evict_tile(tid(1, 1));
    b.insert_tile(&synth_tile(1, 1).encode()).unwrap();
    assert_eq!(a.resident_ids(), b.resident_ids());
    let mut s = Sm(5);
    for _ in 0..3000 {
        let p = pos(
            s.below(EXT_X_MM as u64) as i64,
            s.below(EXT_Y_MM as u64) as i64,
        );
        assert_eq!(a.height_mm(p), b.height_mm(p));
        assert_eq!(a.class_at(p), b.class_at(p));
    }
}

#[test]
fn tiles_for_rect_is_ascending_and_clamped() {
    let w = synth_world();
    let r = |a: (i64, i64), b: (i64, i64)| w.tiles_for_rect(pos(a.0, a.1), pos(b.0, b.1));
    assert_eq!(r((0, 0), (0, 0)), vec![tid(0, 0)]);
    assert_eq!(r((511_999, 511_999), (511_999, 511_999)), vec![tid(0, 0)]);
    assert_eq!(r((512_000, 512_000), (512_000, 512_000)), vec![tid(1, 1)]);
    assert_eq!(
        r((500_000, 500_000), (600_000, 600_000)),
        vec![tid(0, 0), tid(1, 0), tid(0, 1), tid(1, 1)]
    );
    assert_eq!(
        r((-9_000_000, -9_000_000), (9_000_000, 9_000_000)),
        vec![
            tid(0, 0),
            tid(1, 0),
            tid(2, 0),
            tid(0, 1),
            tid(1, 1),
            tid(2, 1)
        ]
    );
    assert_eq!(
        r((EXT_X_MM + 100, 0), (EXT_X_MM + 900, 10)),
        vec![tid(2, 0)]
    );
    assert_eq!(r((-900, -100), (-100, -10)), vec![tid(0, 0)]);
    // An inverted rectangle selects nothing.
    assert!(r((600_000, 0), (100_000, 10)).is_empty());
    // Listing does not depend on residency.
    let empty = TerrainWorld::new(synth_index());
    assert_eq!(
        empty
            .tiles_for_rect(pos(0, 0), pos(EXT_X_MM, EXT_Y_MM))
            .len(),
        6
    );
}
