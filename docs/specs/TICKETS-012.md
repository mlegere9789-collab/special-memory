# TICKETS-012: acoustics model, tickets T12-1 to T12-7

Spec: SPEC-012 (acoustics-model). One ticket, one session, under about 400 changed lines (code, tests, data together). Each ticket writes its tests first, ends with the Q0 commands from `CLAUDE.md`, and appends one row to `ledger/usage.csv`. A ticket that cannot fit splits at the data/code boundary; it never drops or weakens a listed test. Order is dependency order; a ticket starts only when everything in `Needs` is merged. Tests are named by the SPEC-012 ids (`T12-nn`).

| Ticket | Title | Needs |
|---|---|---|
| T12-1 | Sound log in battle-rules (SPEC-005 amendment A5) | SPEC-005 tickets merged |
| T12-2 | Crate skeleton, integer math, data tables | none |
| T12-3 | Delay, gradient, horizon, flat-ground loss | T12-2 |
| T12-4 | Terrain: barrier and foliage | T12-3 |
| T12-5 | Aggregation and camera packets | T12-3 |
| T12-6 | Hearing reports, scenes, calibration bands, goldens | T12-4, T12-5 |
| T12-7 | Adapter, engine facade, performance report | T12-1, T12-6 |

## T12-1: Sound log
- Spec: SPEC-012 section 4 (Amendment A5; write the amendment text into SPEC-005 first, then tests, then code).
- Work: `FireRecord`, `FireRecordKind`, `BattleState::sound_log`, clearing at the start of `step`, appends in ascending `seq` for shots (not misfires) and bursts, exclusion from `battle_hash`.
- Files: `crates/battle-rules/src/battle.rs`, a new `crates/battle-rules/src/sound_log.rs`, `lib.rs` exports, `docs/specs/SPEC-005-weapons-and-fire.md` (amendment only).
- Acceptance: the log half of **T12-14** (record count equals `shots_fired - misfires`, empty after a silent tick, hash unchanged); every existing SPEC-005 test and golden hash unchanged.

## T12-2: Skeleton, integer math, tables
- Spec: SPEC-012 sections 3, 5.
- Work: crate and workspace entry; `intmath::{isqrt_u128, log2_q16, cdb_of_power, power_of_cdb}` with the 257-entry table built at first use by the section 5 recurrence; `AcousticTables::from_toml_str` through the `battle-rules` reader; `data/rules/acoustics.toml` and `data/schemas/acoustics.schema.json`; `tools/validate_data.py` picks the schema up; a short ADR adding the units `_cdb`, `_dc`, `_mk_per_m`, `_hz`, `_ms` to ADR 0004's list.
- Files: `crates/acoustics-model/**`, root `Cargo.toml`, `data/rules/acoustics.toml`, `data/schemas/acoustics.schema.json`, `docs/adr/00NN-acoustics-units.md`, `tools/validate_data.py`.
- Acceptance: **T12-01**, **T12-02**.

## T12-3: Propagation on flat ground
- Spec: SPEC-012 section 6 items 1 to 4, 5 (without terrain).
- Work: `propagation::{sound_speed_mm_s, delay_ms, gradient, horizon_mm, path_result}`, `Weather`, spherical and cylindrical spreading, absorption, shadow; `qc/fixtures/ref_acoustics.py` (independent `f64`) with committed `ref_acoustics.csv`.
- Files: `crates/acoustics-model/src/propagation.rs`, `weather.rs`, `qc/fixtures/ref_acoustics.*`.
- Acceptance: **T12-03**, **T12-04**, **T12-05** (the flat-ground part).

## T12-4: Terrain
- Spec: SPEC-012 section 6 items 4b and 4c.
- Work: `Relief`, nearest-cell lookup, profile sampling, knife-edge loss with the `Q8` path difference, foliage accumulation, `path_result` accepting `Option<&Relief>`.
- Files: `crates/acoustics-model/src/relief.rs`, `propagation.rs`.
- Acceptance: **T12-06**, **T12-12**.

## T12-5: Aggregation and camera packets
- Spec: SPEC-012 sections 7, 8 (camera packet).
- Work: `SoundEvent`, `Aggregator`, `Cluster`, cluster level, `render_packet`, budgets, hysteresis through `ListenerState`, bed sectors, `packet_hash`.
- Files: `crates/acoustics-model/src/{aggregate,listener,packet}.rs`.
- Acceptance: **T12-07**, **T12-08**, **T12-09**, **T12-10**.

## T12-6: Hearing reports, scenes, bands
- Spec: SPEC-012 sections 8 (hearing report), 11 (T12-11, T12-13), 12.
- Work: `hearing_report`, `report_hash`, `scenes.rs` (`seven_pines`, `perryville`, `bombardment`, `ridge_pair`, `volley_20k`), `qc/calibration/acoustics-bands.toml` (CB1 to CB6 with their tags), the `expected-acoustics.json` goldens recorded once on Linux. Tuning is done on data rows (levels, shear, caps), never on a band; a band changes only through a SPEC-012 amendment citing evidence.
- Files: `crates/acoustics-model/src/{report,scenes}.rs`, `qc/calibration/acoustics-bands.toml`, `qc/golden-replays/expected-acoustics.json`, `tools/verify_goldens.sh` (extra rows).
- Acceptance: **T12-11**, **T12-13**. If this overruns 400 lines, move the golden and CI wiring to a follow-up ticket inserted before T12-7.

## T12-7: Adapter, engine facade, performance
- Spec: SPEC-012 sections 4 (adapter), 9, 10, 11 (T12-14, T12-15).
- Work: `adapt_fire`, `AcousticsEngine::{new, push, end_tick}` with two aggregators and the headquarters stagger, an ignored performance test, `docs/cycle-reports/CYCLE-NN-acoustics.md` with mean, p99 and max against H12.
- Files: `crates/acoustics-model/src/{adapter,engine}.rs`, `docs/cycle-reports/`.
- Acceptance: the adapter half of **T12-14**, **T12-15**; an engine test that `end_tick` over a recorded `sound_log` equals the direct pipeline results.
