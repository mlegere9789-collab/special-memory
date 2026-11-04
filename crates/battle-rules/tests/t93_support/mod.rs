//! Shared set-up for the SPEC-009 smoke grid tests.
#![allow(dead_code)]

use battle_rules::{SmokeGrid, SmokeParams};
use core_math::{Fx, Vec2Fx};

#[path = "../t9_support/mod.rs"]
mod t9;

#[allow(unused_imports)]
pub use t9::{env_table, env_table_with, weather};

/// The shipped `[smoke]` constants with each `(from, to)` text edit applied.
pub fn params(edits: &[(&str, &str)]) -> SmokeParams {
    env_table_with(edits).smoke
}

/// A clear grid of `w` x `h` cells of 8 m with its corner at the origin.
pub fn grid(w: u32, h: u32) -> SmokeGrid {
    SmokeGrid::new(8000, w, h, Vec2Fx::ZERO).unwrap()
}

/// The centre of cell `(x, y)` in metres.
pub fn centre(x: u32, y: u32) -> Vec2Fx {
    Vec2Fx::new(
        Fx::from_int(8 * x as i32) + Fx::from_int(4),
        Fx::from_int(8 * y as i32) + Fx::from_int(4),
    )
}

/// Density of cell `(x, y)`.
pub fn at(g: &SmokeGrid, x: u32, y: u32) -> u32 {
    g.density[(y * g.width + x) as usize]
}

/// Sets cell `(x, y)` and keeps the injected counter consistent.
pub fn put(g: &mut SmokeGrid, x: u32, y: u32, d: u32) {
    let i = (y * g.width + x) as usize;
    g.injected = g.injected + u64::from(d) - u64::from(g.density[i]);
    g.density[i] = d;
}

/// Every counter that must balance: `(injected, density + losses)`.
pub fn balance(g: &SmokeGrid) -> (u64, u64) {
    (
        g.injected,
        g.total() + g.lost_edge + g.lost_decay + g.lost_cap,
    )
}

/// A small deterministic generator for test inputs (not authoritative code).
pub struct Lcg(pub u64);

impl Lcg {
    pub fn next(&mut self, n: u64) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 33) % n
    }
}
