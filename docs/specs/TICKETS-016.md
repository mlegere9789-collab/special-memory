# TICKETS-016: prototype and vendor briefs, tickets T-16-1 to T-16-7

Spec: SPEC-016. One ticket, one session, under about 400 changed lines (code, tests, data together). Each ticket writes its tests first, ends with the Q0 commands from `CLAUDE.md`, and appends one row to `ledger/usage.csv`. A ticket that cannot fit splits at the data/code boundary; it never drops or weakens a listed test. A ticket starts only when everything in `Needs` is merged. GPU, performer, recording and vendor work (T16-14 to T16-18, the clip capture, the recordings, the RFQ rounds) is the human and vendor lane and has no ticket here; T-16-7 prepares its templates.

| Ticket | Title | Needs |
|---|---|---|
| T-16-1 | Provenance, clip and rights checkers, soldier skeleton | none |
| T-16-2 | `perf-kit`: tiers, arrival, clusters, voices | none |
| T-16-3 | Percentile summariser, perf schemas, flights | none |
| T-16-4 | Event drain (SPEC-005 amendment A5) | T-024 |
| T-16-5 | `sim-capi`: layouts, stepping, snapshots | T-16-4 |
| T-16-6 | `sim-capi`: orders, replay, smoke view, error fuzz | T-16-5 |
| T-16-7 | M1 and vendor pack: templates, ADR checker | T-16-1, T-16-3, T-16-6 |

## T-16-1: Provenance, checkers, skeleton
- Spec: SPEC-016 sections 2 (assets, consents, clips, recordings), 6.3 to 6.5, 9.
- Work: JSON schemas under `data/schemas/` for `assets`, `consents`, `clips`, `recordings`; empty-but-valid files under `data/provenance/` and `data/animation/`; `data/animation/skeleton_soldier_v1.toml` (64 joints, 28-joint tier-B subset); `tools/check_clips.py`, `tools/check_rights.py`; extend `tools/validate_data.py` to validate the new files; fixtures under `qc/fixtures/`.
- Acceptance: **T16-11**, **T16-12**, **T16-13**; the validator rejects floats and unknown keys in the new files.
- Files: `tools/`, `data/schemas/`, `data/provenance/`, `data/animation/`, `qc/fixtures/`.

## T-16-2: perf-kit
- Spec: SPEC-016 sections 3.5, 7.2, 4 (perf-kit signatures).
- Work: new workspace crate `shells/perf-kit` (`forbid(unsafe_code)`, no dependencies except `core-math` if needed, integers only): `Tier`, `assign_tiers`, `range_mm` (integer square root), `arrival_sample`, `cluster_key`, `Cluster`, `select_voices`.
- Acceptance: **T16-08**, **T16-09**, **T16-10** (tier hysteresis sequence, cap of 800 and 200, brute-force voice order with a proptest, arrival values); negative coordinates in `cluster_key` use `div_euclid`.
- Files: `shells/perf-kit/`, workspace `Cargo.toml`.

## T-16-3: Summariser, schemas, flights
- Spec: SPEC-016 sections 2 (perf files), 3.3, 3.4, 10.
- Work: `qc/perf/summarise.py` (nearest-rank percentiles in integers, median of three runs, threshold evaluation, PASS or FAIL per row); schemas for `machine.toml`, flights and `thresholds.toml`; `qc/perf/thresholds.toml` holding H16-1 to H16-11 with `locked = false`, `placeholder = true` and their CT ids; flights F1 to F5 as committed waypoint files; a CSV fixture.
- Acceptance: **T16-07**; the summariser fails a thresholds row that lacks a calibration target; flight files load with integer fields only.
- Files: `qc/perf/`, `data/schemas/`, `tools/validate_data.py`.

## T-16-4: Event drain
- Spec: SPEC-016 section 1 (assumptions, A5); amend SPEC-005 first (A5), then the tests, then the code.
- Work: `BattleEvent`, a bounded event buffer appended by the fire, hit and casualty phases, `BattleState::drain_events`; events excluded from `battle_hash`.
- Acceptance: events ordered by `(tick, seq)`, draining twice returns each event once (the event half of **T16-05**); every existing golden hash and **T5-20** unchanged; buffer bound documented and tested (oldest dropped is not allowed: overflow returns an error counter, tested).
- Files: `crates/battle-rules/src/`, `docs/specs/SPEC-005-weapons-and-fire.md`.

## T-16-5: sim-capi layouts, stepping, snapshots
- Spec: SPEC-016 section 4 (layouts, quantisation, hash, C functions except orders, replay, smoke).
- Work: new workspace crate `shells/sim-capi` (`cdylib`, `staticlib`), `include/hd_sim.h`, `hd_abi_version`, create, destroy, step, tick, hash, snapshot acquire, accessors, hash and release, `catch_unwind` on every export; scenarios `toy` and `f0_large`; a C test program built with `cc`.
- Acceptance: **T16-01**, **T16-02**, **T16-03**, **T16-04**, the snapshot-hash half of **T16-05**.
- Files: `shells/sim-capi/`, workspace `Cargo.toml`.

## T-16-6: sim-capi orders, replay, smoke, error fuzz
- Spec: SPEC-016 section 4 (authority, `hd_sim_submit_order`, `hd_sim_write_replay`, `hd_smoke_view`).
- Work: order submission through the SPEC-004 codec into the replay input log, replay export, smoke view over the core `SmokeGrid`, argument validation for every export, the random invalid-argument test.
- Acceptance: **T16-06**; the smoke density pointer matches the core grid cell by cell after 100 ticks of `f0_large` (the CT-3 consistency input); `hd-sim verify` accepts an exported replay.
- Files: `shells/sim-capi/`.

## T-16-7: M1 and vendor pack
- Spec: SPEC-016 sections 3, 5, 6, 7, 8, 9, 11 (T16-18).
- Work: `docs/adr/template-renderer-host.md` and `docs/cycle-reports/CYCLE-M1-renderer.template.md` following section 5; `docs/briefs/` with the animation, audio and four RFQ checklists as issue-ready documents drawn from sections 6 to 9; `tools/check_m1_adr.py` (ADR present, machine records match `machine.toml`, threshold rows locked).
- Acceptance: **T16-18** checker passes on a fixture ADR and fails on three broken fixtures; the briefs pass the forbidden-terms scan and the no-process-text lint.
- Files: `docs/adr/`, `docs/cycle-reports/`, `docs/briefs/`, `tools/`, `qc/fixtures/`.
