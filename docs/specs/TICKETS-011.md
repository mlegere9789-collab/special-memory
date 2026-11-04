# TICKETS-011: historical replay and calibration, tickets T-111 to T-116

Spec: SPEC-011. One ticket, one session, under about 400 changed lines (code, tests, data together). Each ticket writes its tests first, ends with the Q0 commands from `CLAUDE.md`, and appends one row to `ledger/usage.csv`. A ticket that cannot fit splits at the data/code boundary; it never drops or weakens a listed test. Ids T-111 to T-116 are reserved for this spec. A ticket starts only when everything in `Needs` is merged.

| Ticket | Title | Needs |
|---|---|---|
| T-111 | Script schema, loader and compiler | T-015, T-018 |
| T-112 | Replay driver, payload tags 16 and 17, history hash | T-111, T-023 |
| T-113 | Metrics, band evaluation, observations | T-112 |
| T-114 | Gettysburg band, timeline and conflict data | T-111, T-113 |
| T-115 | Suite runner, reports, `hd-sim cal`, CI steps | T-024, T-113, T-114 |
| T-116 | Fit and hold-out tooling | T-115 |

## T-111: Script schema, loader, compiler
- Spec: SPEC-011 sections 3 (script, units, policy), 4.1, 5 (`ReplayScript`, `compile_script`, `UnitRecord`).
- Work: new crate `crates/calibration` (workspace entry, skeleton), script and policy readers on the battle-rules integer-only reader, `data/schemas/replay-script.schema.json`, `qc/calibration/policy.toml` with every threshold as a placeholder row (CT-1101 to CT-1105), `tools/validate_data.py` extended to `data/scenarios/*/script.toml` and `policy.toml`, one toy script under `data/scenarios/toy_replay/`.
- Acceptance: **T11-01** (loader errors, exact ticks, ordering, per-tick limit, parent expansion).
- Files: `crates/calibration/**`, `data/schemas/`, `data/scenarios/toy_replay/`, `qc/calibration/policy.toml`, `tools/validate_data.py`, `tools/test_validate_data.py`, workspace `Cargo.toml`.

## T-112: Replay driver and history hash
- Spec: SPEC-011 sections 4.1 (payloads), 4.2, 5 (`ReplayDriver`, `TriggerHook`, codec functions).
- Work: `encode_arrive`/`decode_arrive`/`encode_trigger`/`decode_trigger`, driver with the apply-then-step loop, `timeline_log`, `history_hash`, replay writing through SPEC-002 with actor 0, and `hd-sim verify` support for replay-mode headers (`<scenario>/<variant>`).
- Acceptance: **T11-02**, **T11-03**, the scripted half of **T11-04**.
- Files: `crates/calibration/src/{codec,driver}.rs`, `shells/sim-cli/src/{golden,cli}.rs` (verify path only), tests.

## T-113: Metrics, band evaluation, observations
- Spec: SPEC-011 sections 3 (band file), 4.3, 4.4, 4.5 (observed), 5 (`BandSet`, `measure`, `evaluate`, `Capabilities`).
- Work: band reader and provenance rules, casualty-log metrics (`count`, `rate_pm`, `rate_pm_median`), `evaluate` with lower median, spread, excess, `upper_only`, `weak`; the four observation kinds; variant runs.
- Acceptance: **T11-05**; the observed half of **T11-04** (`reached` against the SPEC-004 T4-03 arrival tick).
- Files: `crates/calibration/src/{bands,metrics,observe}.rs`, tests, `data/schemas/bands.schema.json`.

## T-114: Gettysburg band, timeline and conflict data
- Spec: SPEC-011 section 7 (all three tables), section 3 (fixtures file).
- Work: `qc/calibration/gettysburg-bands.toml` (GB-01 to GB-24, widened by the stated rules, confidence and page tags as tabulated), the TL rows as `[[arrival]]`, `[[order]]`, `[[trigger]]`, `[[observe]]` rows in a scenario skeleton `data/scenarios/gb_timeline/script.toml`, `qc/fixtures/history-conflicts.toml` (CF-01 to CF-10), a validator rule that recomputes widening and checks ABT sums, a Rust fixture-rule checker.
- Acceptance: **T11-06**, **T11-07**. No value is changed to suit a simulation result; a wrong value changes only with a page citation.
- Files: `qc/calibration/`, `qc/fixtures/`, `data/scenarios/gb_timeline/`, `tools/validate_data.py`, `crates/calibration/src/fixtures.rs`, tests.

## T-115: Suite runner, reports, `hd-sim cal`, CI
- Spec: SPEC-011 sections 4.7, 4.8, 5 (`run_suite`, `render_report`, CLI), 6.
- Work: `SuiteSpec`/`SuiteReport`, deterministic renderer, `baseline.toml` trend, `hd-sim cal run|report` with exit codes 0, 1, 2, smoke suite on the `gb_v_minnesota` vignette (needs the scenario files from the scenario spec; if absent, a stand-in vignette with the same ids), golden replay and `expected.json` rows, CI: Q1 runs `cal run --suite smoke`, Q2 matrix verifies the golden and `report_hash`, Q3 nightly runs `--suite full` with 16 seeds and uploads the report; first `docs/cycle-reports/CAL-01.md` including measured soldier-tick cost against H11.
- Acceptance: **T11-08**, **T11-11**, **T11-12**.
- Files: `crates/calibration/src/{suite,report}.rs`, `shells/sim-cli/src/cal.rs`, `.github/workflows/ci.yml` (calibration steps only), `qc/golden-replays/`, `qc/calibration/golden/`, `docs/cycle-reports/CAL-01.md`.

## T-116: Fit and hold-out tooling
- Spec: SPEC-011 sections 4.6, 5 (`fit`, `holdout`), 9.
- Work: `tunables.toml`, `split.toml`, `accepted-ranges.toml`, `holdout-log.toml` readers; coordinate-descent `fit`, accepted-run computation, free-parameter and SPEC-005 band-suite guards; `holdout` with the look log and limit; `hd-sim cal fit|holdout`.
- Acceptance: **T11-09**, **T11-10**. Tuning writes placeholder rows only; no band file is touched.
- Files: `crates/calibration/src/{fit,holdout}.rs`, `shells/sim-cli/src/cal.rs`, `qc/calibration/*.toml`, tests.
