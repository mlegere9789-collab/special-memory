# TICKETS-006: morale and cohesion, tickets T-061 to T-066

Spec: SPEC-006. One ticket, one session, under about 400 changed lines (code, tests, data together). Each ticket writes its tests first, ends with the Q0 commands from `CLAUDE.md`, and appends one row to `ledger/usage.csv`. A ticket that cannot fit splits at the data/code boundary; it never drops or weakens a listed test. Order is dependency order; a ticket starts only when everything in `Needs` is merged. All tickets need the SPEC-004 and SPEC-005 tickets of TICKETS-P1 up to T-023 unless a narrower list is given.

| Ticket | Title | Needs |
|---|---|---|
| T-061 | Morale table, schema, pure helpers | T-015 |
| T-062 | Inputs, ceiling and relaxation | T-061, T-023 |
| T-063 | State machine, rolls, events, hash | T-062 |
| T-064 | Soldier symptoms, flight orders, report, fire interplay | T-063 |
| T-065 | Calibration harness, bands, goldens | T-064, T-024 |
| T-066 | Performance report and scenario switch | T-065 |

## T-061: Morale table, schema, pure helpers
- Spec: SPEC-006 sections 2, 3, 5.8 (`load_gate` only), 7 (helper signatures).
- Work: `crates/battle-rules/src/morale/{mod,table,helpers}.rs`; `data/rules/morale.toml` with every row's provenance (page and tag, or placeholder with CT-61 to CT-68); `data/schemas/morale.schema.json`; `MoraleTable::from_toml_str`, `Tables.morale`; `shock_unit`, `lerp`, `load_gate`, `classify_exposure`; `qc/fixtures/ref_morale.py` (`Fraction`) with committed output; flag constants `F_LOOKBACK`, `F_SKULK`, stream constant.
- Acceptance: **T6-01**, **T6-02**, **T6-03**, **T6-04** (exposure classes at the listed bearings).
- Files: `crates/battle-rules`, `data/rules`, `data/schemas`, `qc/fixtures`.

## T-062: Inputs, ceiling and relaxation
- Spec: SPEC-006 sections 4 (state structs, `enable_morale`), 5.2, 5.3.
- Work: `MoraleState`, `RegimentMorale`, `Leader`, `enable_morale` (officer selection, `nerve_offset` from the named stream), `set_leader`, `set_supply`, input sampling from a frozen snapshot (fatigue mean, ammunition, terrain, wall, exposure, charge, pressure, leader), ceiling `T`, relaxation, window decay. No state transitions yet.
- Acceptance: **T6-05** (penalties equal formulas, bounds, monotone convergence, proptest); an `enable_morale` unit test for `s0`, `shock_unit`, officer indices and that the only enable-time draw is `nerve_offset`.
- Files: `crates/battle-rules/src/morale/{state,inputs,ceiling}.rs`.

## T-063: State machine, rolls, events, hash
- Spec: SPEC-006 sections 5.1, 5.4, 5.5 (transitions and neighbour events, without flight orders and returns), 6.
- Work: casualty ingestion from the SPEC-005 log with `cursor`, the five-state machine with hysteresis, the fixed draw count, `MoraleEvent` list and cascade application, `broken_count`, hash block (written only when `morale` is `Some`), `step_morale` wired after SPEC-005 step (5).
- Acceptance: **T6-06** (states), **T6-08** (cascade, order independence), **T6-09** (draw count, stream isolation), the `None` half of **T6-14** (existing goldens unchanged), and the field-change half of **T6-14** (every listed field alters the hash).
- Files: `crates/battle-rules/src/morale/{step,transition,events}.rs`, `battle_hash.rs`.

## T-064: Soldier symptoms, flight orders, report, fire interplay
- Spec: SPEC-006 sections 5.5 (entry actions, returns), 5.6, 5.7, 5.8 (apply the amendments to SPEC-004 and SPEC-005 text first).
- Work: look-back and skulk draws, `F_SKULK` roster exclusion (`regiment_roster`, `sync_roster`), absent list and returns, flight `Order::Move` with retreat point, `issue_order` refusal, `Forming` cap, `BattleState::can_fire` morale gate, `fire_pm`, `shake_pm`, load gate in the fire phase, `sigma_s` factor, indicator with dwell, causes, `morale_report`, `set_rally_point`.
- Acceptance: **T6-07** (rally, including the movement check), **T6-10** (interface), **T6-11** (fire), **T6-12** (absentees and officers).
- Files: `crates/battle-rules/src/morale/{symptoms,report}.rs`, small edits in `slots.rs`, `battle.rs`, `fire.rs`, `aim.rs`, plus the two spec amendments.

## T-065: Calibration harness, bands, goldens
- Spec: SPEC-006 sections 9 (T6-13, T6-14 golden), 10.
- Work: `calibration::{cal_attrition, run_morale_bands}` with the injected-casualty test hook, `qc/calibration/morale-bands.toml` (M1 to M8 with sources), `hd-sim battle --morale on`, golden replay row for `cal_attrition(trained, 3 per s)`, recorded results and any placeholder edits made by spec amendment only.
- Acceptance: **T6-13** (bands M1 to M8), **T6-14** golden (tick 1200, seed 1, Linux literal, cross-process and three-platform check).
- Files: `crates/battle-rules/src/calibration`, `qc/calibration`, `qc/golden-replays/expected.json`, `shells/sim-cli`.

## T-066: Performance report and scenario switch
- Spec: SPEC-006 sections 8, 9 (T6-15).
- Work: ignored perf test on `f0_large` with morale on, numbers written to `docs/cycle-reports/CYCLE-03-morale.md`, per-scenario `morale` flag in scenario TOML (default off), tutorial scenario note for the symptom display contract (indicator, causes, symptoms; no raw value).
- Acceptance: **T6-15** printed mean, p99, max against H6; existing scenarios unchanged with the flag off (goldens identical).
- Files: `crates/battle-rules/tests`, `docs/cycle-reports`, `data/scenarios`.
