//! `hd-terrain`, the battlefield data compiler (SPEC-014 section 10): `check` and `compile`.
//!
//! A survey directory holds `survey.toml`, `landcover.toml`, `linear.toml`, `buildings.toml`,
//! `names.toml` and the ingest outputs `dem_cm.i32` and `coverage.u8`. Everything here is integer
//! arithmetic; the output is a pure function of those files.

pub mod check;
pub mod compile;
pub mod geom;
pub mod raster;
pub mod ring;
pub mod survey;

use std::io::Write;
use std::path::PathBuf;

const USAGE: &str = "usage: hd-terrain check --survey DIR [--sources FILE]\n       hd-terrain compile --survey DIR --out DIR [--sources FILE]\n";

struct Args {
    survey: PathBuf,
    out: Option<PathBuf>,
    sources: Option<PathBuf>,
}

fn parse(args: &[String], with_out: bool) -> Result<Args, String> {
    let (mut survey, mut out, mut sources) = (None, None, None);
    let mut it = args.iter();
    while let Some(flag) = it.next() {
        let slot = match flag.as_str() {
            "--survey" => &mut survey,
            "--out" if with_out => &mut out,
            "--sources" => &mut sources,
            other => return Err(format!("unknown argument `{other}`")),
        };
        *slot = Some(PathBuf::from(
            it.next().ok_or_else(|| format!("`{flag}` needs a value"))?,
        ));
    }
    if with_out && out.is_none() {
        return Err("`--out` is required".into());
    }
    Ok(Args {
        survey: survey.ok_or("`--survey` is required")?,
        out,
        sources,
    })
}

/// Runs the command in `args` (without the program name). Exit codes: 0 success, 1 the survey
/// fails its checks, 2 bad usage or unreadable input.
pub fn run_cli(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> i32 {
    let Some((cmd, rest)) = args.split_first() else {
        let _ = err.write_all(USAGE.as_bytes());
        return 2;
    };
    let compiling = cmd == "compile";
    let parsed = match cmd.as_str() {
        "check" | "compile" => parse(rest, compiling),
        _ => Err(format!("unknown command `{cmd}`")),
    };
    let a = match parsed {
        Ok(a) => a,
        Err(e) => {
            let _ = writeln!(err, "hd-terrain: {e}\n{USAGE}");
            return 2;
        }
    };
    let survey = match survey::load(&a.survey) {
        Ok(s) => s,
        Err(e) => {
            let _ = writeln!(err, "hd-terrain: {e}");
            return 2;
        }
    };
    let (problems, pack) = check::check_pack(&survey, a.sources.as_deref());
    for p in &problems {
        let _ = writeln!(err, "{p}");
    }
    if !problems.is_empty() {
        let _ = writeln!(err, "hd-terrain: {} problems", problems.len());
        return 1;
    }
    if a.sources.is_none() {
        let _ = writeln!(out, "check: sources not checked (no --sources)");
    }
    let (Some(pack), Some(dir)) = (pack, a.out.as_ref().filter(|_| compiling)) else {
        let _ = writeln!(out, "check: ok");
        return 0;
    };
    if let Err(e) = pack.write(dir) {
        let _ = writeln!(err, "hd-terrain: {}: {e}", dir.display());
        return 2;
    }
    let _ = writeln!(
        out,
        "compile: {} tiles\ncontent_hash = {:016x}",
        pack.tiles.len(),
        pack.content_hash
    );
    0
}
