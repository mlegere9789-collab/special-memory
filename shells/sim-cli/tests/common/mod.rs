//! Helpers shared by the integration tests.
#![allow(dead_code)]

use hd_sim::cli::run_cli;
use sim_state::fnv1a64;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

static COUNTER: AtomicU32 = AtomicU32::new(0);

/// A unique scratch path in the system temp directory.
pub fn tmp_path(tag: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "hd-sim-test-{}-{}-{}.hdrp",
        std::process::id(),
        tag,
        n
    ))
}

/// Runs the CLI in-process; returns `(exit code, stdout, stderr)`.
pub fn cli(args: &[&str]) -> (i32, String, String) {
    let args: Vec<String> = args.iter().map(|s| (*s).to_string()).collect();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = run_cli(&args, &mut out, &mut err);
    (
        code,
        String::from_utf8(out).expect("stdout is utf-8"),
        String::from_utf8(err).expect("stderr is utf-8"),
    )
}

/// Runs the compiled `hd-sim` binary as a separate process; returns `(exit code, stdout, stderr)`.
pub fn process(args: &[&str]) -> (i32, String, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_hd-sim"))
        .args(args)
        .output()
        .expect("spawn hd-sim");
    (
        output.status.code().expect("exit code"),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// Recomputes the trailing body hash of a replay held in memory.
pub fn fix_body_hash(bytes: &mut [u8]) {
    let n = bytes.len();
    let h = fnv1a64(&bytes[..n - 8]);
    bytes[n - 8..].copy_from_slice(&h.to_le_bytes());
}

/// Byte offset of the hash field of checkpoint `index` in a replay that has `count` checkpoints.
/// The checkpoints sit directly before the 24-byte footer, 16 bytes each (tick, hash).
pub fn checkpoint_hash_offset(len: usize, count: usize, index: usize) -> usize {
    len - 24 - 16 * (count - index) + 8
}

/// Runs the compiled `hd-sim` binary with a wall-clock guard; panics (after killing the child)
/// if it has not exited within `limit`. Returns `(exit code, stdout, stderr)`.
pub fn process_with_timeout(args: &[&str], limit: std::time::Duration) -> (i32, String, String) {
    use std::io::Read;
    use std::process::Stdio;
    let mut child = Command::new(env!("CARGO_BIN_EXE_hd-sim"))
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn hd-sim");
    let started = std::time::Instant::now();
    loop {
        if let Some(status) = child.try_wait().expect("poll child") {
            let mut out = String::new();
            let mut err = String::new();
            child
                .stdout
                .take()
                .unwrap()
                .read_to_string(&mut out)
                .unwrap();
            child
                .stderr
                .take()
                .unwrap()
                .read_to_string(&mut err)
                .unwrap();
            return (status.code().expect("exit code"), out, err);
        }
        if started.elapsed() > limit {
            let _ = child.kill();
            let _ = child.wait();
            panic!("hd-sim {args:?} did not exit within {limit:?}");
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}

/// Hand-assembles a toy-scenario replay (one setup input, no checkpoints unless given) with a
/// correct body hash and no validation, so that crafted and hostile headers can be tested.
pub fn craft_replay(
    agents: u32,
    interval: u32,
    total_ticks: u64,
    checkpoints: &[(u64, u64)],
    setup: (u64, u8, u16),
) -> Vec<u8> {
    let id = hd_sim::scenario::SCENARIO_ID;
    let mut b = b"HDRP".to_vec();
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&(id.len() as u16).to_le_bytes());
    b.extend_from_slice(id.as_bytes());
    b.extend_from_slice(&1u64.to_le_bytes()); // seed
    b.extend_from_slice(&20u16.to_le_bytes()); // tick_hz
    b.push(0); // fidelity
    b.extend_from_slice(&interval.to_le_bytes());
    b.extend_from_slice(&1u32.to_le_bytes()); // one input
    b.extend_from_slice(&setup.0.to_le_bytes());
    b.push(setup.1);
    b.extend_from_slice(&setup.2.to_le_bytes());
    b.extend_from_slice(&12u32.to_le_bytes());
    b.extend_from_slice(&agents.to_le_bytes());
    b.extend_from_slice(&2000u32.to_le_bytes());
    b.extend_from_slice(&2000u32.to_le_bytes());
    b.extend_from_slice(&(checkpoints.len() as u32).to_le_bytes());
    for &(t, h) in checkpoints {
        b.extend_from_slice(&t.to_le_bytes());
        b.extend_from_slice(&h.to_le_bytes());
    }
    b.extend_from_slice(&total_ticks.to_le_bytes());
    b.extend_from_slice(&0u64.to_le_bytes()); // final hash
    let h = fnv1a64(&b);
    b.extend_from_slice(&h.to_le_bytes());
    b
}

/// Writes `bytes` to a scratch file, runs `hd-sim verify` on it under a time guard and removes
/// the file.
pub fn verify_guarded(bytes: &[u8], limit: std::time::Duration) -> (i32, String, String) {
    let path = tmp_path("guarded");
    std::fs::write(&path, bytes).unwrap();
    let result = process_with_timeout(&["verify", path.to_str().unwrap()], limit);
    let _ = std::fs::remove_file(path);
    result
}
