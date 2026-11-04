//! Shared helpers for the aggregation and packet tests.
#![allow(dead_code)]

use acoustics_model::{AcousticTables, Listener, ListenerKind, SoundEvent, Weather};
use core_math::{Fx, Vec2Fx};

pub fn tables() -> AcousticTables {
    AcousticTables::from_toml_str(include_str!("../../../../data/rules/acoustics.toml")).unwrap()
}

pub fn preset(t: &AcousticTables, id: &str) -> Weather {
    Weather::from_preset(t.weather(id).unwrap())
}

/// Position in whole metres.
pub fn pos(x: i32, y: i32) -> Vec2Fx {
    Vec2Fx {
        x: Fx::from_int(x),
        y: Fx::from_int(y),
    }
}

pub fn event(seq: u64, source: u8, unit: u32, p: Vec2Fx, count: u16) -> SoundEvent {
    SoundEvent {
        seq,
        source,
        unit,
        team: (unit % 2) as u8,
        pos: p,
        height_mm: 1500,
        count,
    }
}

pub fn listener(x: i32, y: i32) -> Listener {
    Listener {
        id: 1,
        kind: ListenerKind::Camera,
        pos: pos(x, y),
        height_mm: 1500,
        ambient_cdb: [4000, 3500, 3000],
    }
}

/// Small deterministic generator for test data (xorshift64*).
pub struct Rng(pub u64);

impl Rng {
    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    pub fn below(&mut self, n: u64) -> u64 {
        (self.next() >> 11) % n
    }

    pub fn shuffle<T>(&mut self, v: &mut [T]) {
        for i in (1..v.len()).rev() {
            let j = self.below(i as u64 + 1) as usize;
            v.swap(i, j);
        }
    }
}
