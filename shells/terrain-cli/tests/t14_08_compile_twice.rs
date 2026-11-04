//! T14-08 (compile-twice half): the same survey compiles to one pack in one process, in two
//! processes, and the written pack loads through `TerrainWorld`.
mod common;

use common::*;
use core_math::Vec2Fx;
use std::fs;
use std::path::Path;
use std::process::Command;
use terrain::{mm_to_fx, TerrainIndex, TerrainWorld};

fn read_tree(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .chain(fs::read_dir(dir.join("tiles")).unwrap())
        .map(|e| e.unwrap().path())
        .filter(|p| p.is_file())
        .map(|p| p.strip_prefix(dir).unwrap().display().to_string())
        .collect();
    names.sort();
    names
        .into_iter()
        .map(|n| {
            let b = fs::read(dir.join(&n)).unwrap();
            (n, b)
        })
        .collect()
}

#[test]
fn two_builds_in_one_process_are_equal() {
    let dir = make_survey("twice_a", &[]);
    let (a, b) = (build(&dir), build(&dir));
    assert_eq!(a.content_hash, b.content_hash);
    assert_eq!(a.index_text, b.index_text);
    assert_eq!(a.tiles[0].encode(), b.tiles[0].encode());
}

#[test]
fn two_processes_write_identical_packs() {
    let dir = make_survey("twice_b", &[]);
    let exe = env!("CARGO_BIN_EXE_hd-terrain");
    let mut hashes = Vec::new();
    let mut trees = Vec::new();
    for n in 0..2 {
        let out = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("twice_out_{n}"));
        let _ = fs::remove_dir_all(&out);
        let o = Command::new(exe)
            .args(["compile", "--survey"])
            .arg(&dir)
            .arg("--out")
            .arg(&out)
            .output()
            .unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        hashes.push(String::from_utf8(o.stdout).unwrap());
        trees.push(read_tree(&out));
    }
    assert_eq!(hashes[0], hashes[1]);
    assert!(hashes[0].contains("content_hash"), "{}", hashes[0]);
    assert_eq!(trees[0], trees[1]);
    let names: Vec<_> = trees[0].iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, vec!["index.toml", "tiles/0_0.hdt"]);
}

#[test]
fn written_pack_loads_and_answers_queries() {
    let pack = build(&make_survey("twice_c", &[]));
    let index = TerrainIndex::from_toml_str(&pack.index_text).unwrap();
    assert_eq!(index.content_hash(), pack.content_hash);
    assert_eq!(index.slabs()[0].id, "synthetic_slab");
    let mut w = TerrainWorld::new(index);
    w.insert_tile(&pack.tiles[0].encode()).unwrap();
    // Vertex (10, 10) is at 40,000 mm: 316 cm of elevation, 312 cm above the origin (4 cm).
    let p = Vec2Fx::new(mm_to_fx(40_000), mm_to_fx(40_000));
    assert_eq!(w.height_mm(p).unwrap(), 10 * (316 - 4));
    let sq = Vec2Fx::new(mm_to_fx(82_000), mm_to_fx(82_000));
    assert_eq!(w.class_at(sq).unwrap(), 8);
}
