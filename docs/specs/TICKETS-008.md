# TICKETS-008: fidelity scheduler and scenario loader, tickets T-081 to T-087

> Shared namespaces (input tags and actors, random streams, flags, hash blocks, tick order, CT ids, amendment labels, ticket ids) are owned by [REGISTRY.md](REGISTRY.md); conflicts and their resolutions are in [CONFLICTS.md](CONFLICTS.md).

Spec: SPEC-008. Ids use the 08x series so they never collide with other ticket lists. One ticket, one session, under about 400 changed lines (code, tests, data together). Each ticket writes its tests first, ends with the Q0 commands from `CLAUDE.md`, and appends one row to `ledger/usage.csv`. A ticket that cannot fit splits at the data/code boundary; it never drops or weakens a listed test. A ticket starts only when everything in `Needs` is merged. P1 tickets are those of `TICKETS-P1.md`.

| Ticket | Title | Needs |
|---|---|---|
| T-081 | Scenario format, loader, validation, hash | T-018, T-025, T-027 |
| T-082 | Fidelity table, classifier, caps, sector map | T-081 |
| T-083 | Aggregate regiments, hand-overs, ledger, stream continuity | T-023, T-082 |
| T-084 | Battle wrapper, schedule, replay binding, `hd-sim resolve` | T-083 |
| T-085 | F1 bundled volleys and the F1 fit tool | T-024, T-084 |
| T-086 | F2 exchange and aggregate movement | T-085 |
| T-087 | Streaming obstacles, smoke tiles, hot-window grid, scale scenario, goldens, perf | T-086 |

## T-081: Scenario format and loader
- Spec: SPEC-008 sections 2.3, 3.
- Work: `crates/battle-rules/src/scenario.rs` (`Scenario`, `ScenarioError`, `scenario_hash`, `from_toml_str`, terrain `source` file support); `data/schemas/scenario.schema.json`; a validator plugin `tools/validators/scenario.py` for `data/scenarios/*.toml` (the sidecar directories are not scenarios; REGISTRY section 12); `data/scenarios/fid-small.toml` (Buford's division, Heth's division, one arrival, one trigger, two weather rows, with CWV p069, p079, p080 provenance); one bad fixture per rule V1 to V12 in `qc/fixtures/scenario-bad/`.
- Acceptance: **T8-01**, **T8-02**, **T8-03** (the `scenario_hash` half; the `Battle::hash` half moves to T-084).
- Files: `crates/battle-rules`, `data/`, `qc/fixtures/`, `tools/validators/scenario.py`.

## T-082: Fidelity table, classifier, sectors
- Spec: SPEC-008 sections 2.1, 4.1, 7 (sector states only).
- Work: `data/rules/fidelity.toml` and schema, `FidelityTable`, `FidelityClass`, `ClassInput`, `classify` (gap, hysteresis, dwell, strict-prefix caps), `SectorMap::{new, state, hot_window}` with Hot, Warm, Cold and the smoke-total exit rule.
- Acceptance: **T8-05** (all hand-derived cases), **T8-06**, the Hot-set brute-force half of **T8-15**; loader half of the **T8-01** style checks for `fidelity.toml`.
- Files: `crates/battle-rules`, `data/rules`, `data/schemas`.

## T-083: Aggregates and hand-overs
- Spec: SPEC-008 section 5; amend SPEC-005 section 4 for materialised-soldier streams in the same change, spec first (SPEC-005 amendment A7; SPEC-004 amendment A4 for the new `Regiment` fields, REGISTRY section 9).
- Work: `Aggregate`, regiment `status/class/since`, `condense`, `materialise` (exact remainder distribution, `serial` stream ids, projectile-ownership guard), `Battle::ledger` pieces for soldiers and ammunition, shot-word discard helper (50 words, misfire first).
- Acceptance: **T8-07** and **T8-08** on a hand-built 6-regiment fixture stepped by the SPEC-005 `BattleState::step` with forced class changes, **T8-09**, **T8-10**.
- Files: `crates/battle-rules`, `docs/specs/SPEC-005-weapons-and-fire.md` (amendment only).

## T-084: Battle wrapper and replay binding
- Spec: SPEC-008 sections 4 (tick order), 8, 9 (`Battle`).
- Work: `Battle::{from_scenario, step, run, hash}` with arrivals, departures, triggers (order, fire mode, resupply, marker) and weather applied at boundaries; `fid_mixed` scenario; tag-0x20 input and `verify` check; `hd-sim resolve`; the `Battle::hash` extension.
- Acceptance: **T8-03** (hash half), **T8-04**, **T8-11** (scheduler off the SPEC-005 path: class-0 goldens unchanged), **T8-17**, **T8-18** (class 0 only until T-085 and T-086 land; the class 1 and 2 cases are enabled by those tickets), **T8-19**.
- Files: `crates/battle-rules` (module `runner`), `shells/sim-cli`, `docs/specs/SPEC-003-sim-cli-harness.md` (SPEC-003 amendment A2: `verify` binding dispatch and the subcommand table of REGISTRY section 15).

## T-085: F1 volleys and the fit tool
- Spec: SPEC-008 sections 2.2, 6.1; the F1 half of 11.
- Work: `F1Table` and `data/rules/f1.toml` schema; `pending` shot lists, count and victim streams, delay ring, victim and outcome path into the SPEC-005 casualty queue; `fit_f1` and `hd-sim fit-f1`; the committed `f1.toml` generated from `qc/calibration/f0-curves.json`.
- Acceptance: **T8-12**, **T8-14**; unit tests that an F1 shot consumes 50 words and that a ring entry with an empty roster is lost without a draw mismatch.
- Files: `crates/battle-rules`, `shells/sim-cli`, `data/rules`, `data/schemas`.

## T-086: F2 exchange
- Spec: SPEC-008 sections 6.2, 6.3.
- Work: aggregate fire with carried remainders, aggregate casualties and disabled release, aggregate movement and fatigue, class-2 `Battle` path; enable the class 1 and 2 cases of T8-18.
- Acceptance: **T8-13**, the class 1 and class 2 cases of **T8-18**; a property test that `fit` never underflows and `rounds` never goes negative.
- Files: `crates/battle-rules`.

## T-087: Sector streaming, scale scenario, goldens, performance
- Spec: SPEC-008 sections 7 (smoke tiles, obstacle subsets, grid), 10, and the remaining tests.
- Work: `sector_obstacles` index and chord filtering, per-sector `SmokeGrid` tiles, hot-window `SpatialGrid` with doubling, `gb_scale_lite` generator (400 regiments, 165,000 men), golden replays and `expected.json` rows, ignored perf test, `docs/cycle-reports/CYCLE-04-fidelity.md` with measured mean, p99 and max against H8-1 to H8-5.
- Acceptance: the chord and smoke halves of **T8-15**, **T8-16**, **T8-20**, **T8-21**.
- Files: `crates/battle-rules`, `crates/sim-state` only if the grid needs a constructor tweak (write SPEC-002 amendment A3 first), `qc/golden-replays`, `docs/cycle-reports`.
