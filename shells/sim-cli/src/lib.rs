//! `hd-sim`: a headless harness that runs a deliberately simple toy scenario and proves the
//! determinism machinery (SPEC-003).
//!
//! * [`scenario`]: the toy scenario, replay recording and verification. Integer and [`core_math::Fx`]
//!   arithmetic only; randomness only from named streams.
//! * [`golden`]: the parameters of the committed golden replays and the `expected.json` format.
//! * [`history`]: `verify` for replay-mode headers (`<scenario>/<variant>`, SPEC-011).
//! * [`resolve`] and [`bound`]: `resolve` and the data-scenario binding of `verify` (SPEC-003
//!   amendment A2).
//! * [`cli`]: the command-line front end (`run`, `verify`, `hash`, `bench`, `resolve`).
//! * [`bench`]: tick timing, the only place a wall clock is read.
#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod bench;
pub mod bound;
pub mod cli;
pub mod golden;
pub mod history;
pub mod resolve;
pub mod scenario;
