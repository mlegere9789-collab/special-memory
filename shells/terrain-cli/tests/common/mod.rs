//! Shared helpers: build a survey directory from the committed synthetic fixtures plus generated
//! elevation files (post `(i, j)` is `5 i + 3 j` centimetres, source 1).
#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

pub const POSTS: usize = 513;
pub const FILES: [&str; 5] = [
    "survey.toml",
    "landcover.toml",
    "linear.toml",
    "buildings.toml",
    "names.toml",
];

pub fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../qc/fixtures/terrain")
}

pub fn dem_bytes(f: impl Fn(usize, usize) -> i32) -> Vec<u8> {
    let mut v = Vec::with_capacity(POSTS * POSTS * 4);
    for j in 0..POSTS {
        for i in 0..POSTS {
            v.extend_from_slice(&f(i, j).to_le_bytes());
        }
    }
    v
}

pub fn plane(i: usize, j: usize) -> i32 {
    5 * i as i32 + 3 * j as i32
}

/// A fresh survey directory named `tag`; `replace` swaps the text of a fixture file.
pub fn make_survey(tag: &str, replace: &[(&str, String)]) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(tag);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    for name in FILES {
        let text = match replace.iter().find(|(n, _)| *n == name) {
            Some((_, t)) => t.clone(),
            None => fs::read_to_string(fixtures().join("synthetic").join(name)).unwrap(),
        };
        fs::write(dir.join(name), text).unwrap();
    }
    fs::write(dir.join("dem_cm.i32"), dem_bytes(plane)).unwrap();
    fs::write(dir.join("coverage.u8"), vec![1u8; POSTS * POSTS]).unwrap();
    dir
}

pub fn fixture_text(sub: &str, name: &str) -> String {
    fs::read_to_string(fixtures().join(sub).join(name)).unwrap()
}

/// Class at cell `(cx, cy)` of tile (0, 0).
pub fn class(pack: &hd_terrain::compile::Pack, cx: usize, cy: usize) -> u8 {
    pack.tiles[0].classes()[cy * 128 + cx]
}

pub fn occ(pack: &hd_terrain::compile::Pack, cx: usize, cy: usize) -> u8 {
    pack.tiles[0].occ()[cy * 128 + cx]
}

/// Cells of tile (0, 0) holding `class_id`, as `(cx, cy)` in ascending `(cy, cx)` order.
pub fn cells_of(pack: &hd_terrain::compile::Pack, class_id: u8) -> Vec<(usize, usize)> {
    let mut v = Vec::new();
    for cy in 0..128 {
        for cx in 0..128 {
            if class(pack, cx, cy) == class_id {
                v.push((cx, cy));
            }
        }
    }
    v
}

pub fn build(dir: &Path) -> hd_terrain::compile::Pack {
    let s = hd_terrain::survey::load(dir).unwrap();
    hd_terrain::compile::build_pack(&s).unwrap()
}
