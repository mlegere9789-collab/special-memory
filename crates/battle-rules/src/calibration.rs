//! Calibration harnesses (SPEC-005 section 9; later specs add one file each).
//!
//! A harness runs scenarios over a list of seeds, reduces each run to integer metrics and compares
//! the mean over seeds with a band from `qc/calibration/bands.toml`. Nothing here changes a rule
//! or a table: a missed band is reported, never tuned away.

pub mod bands;
pub mod fire;
pub mod report;

pub use bands::{parse_bands, Band, Point};
pub use fire::{run_band_suite, run_cal_line, BandRow, LineRun, LineSet, SuiteReport, SEEDS};
