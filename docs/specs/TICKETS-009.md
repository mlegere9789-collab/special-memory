# TICKETS-009: smoke, weather and sight, tickets T-091 to T-097

> Shared namespaces (input tags and actors, random streams, flags, hash blocks, tick order, CT ids, amendment labels, ticket ids) are owned by [REGISTRY.md](REGISTRY.md); conflicts and their resolutions are in [CONFLICTS.md](CONFLICTS.md).

Spec: SPEC-009 (smoke, weather and sight), on top of SPEC-004 and SPEC-005. One ticket, one session, under about 400 changed lines (code, tests, data together). Each ticket writes its tests first, ends with the Q0 commands from `CLAUDE.md`, and appends one row to `ledger/usage.csv`. A ticket that cannot fit splits at the data/code boundary; it never drops or weakens a listed test. Order is dependency order; a ticket starts only when everything in `Needs` is merged. Every ticket keeps the `env = None` goldens (T5-20, T4-12) unchanged.

| Ticket | Title | Needs |
|---|---|---|
| T-091 | Environment data, weather timeline, exp table | T-015, T-016, T-025, T-027, T-141 (retain rows for terrain classes 8 to 11) |
| T-092 | Heat and rain effects | T-091, T-022 |
| T-093 | Smoke grid update and hash | T-091, T-022 |
| T-094 | Relief grid and sight query | T-091, T-023 |
| T-095 | Targeting gate, aim effects, peek | T-092, T-093, T-094 |
| T-096 | Observers, sighting log, signal quality | T-094, T-093 |
| T-097 | Scenarios, goldens, calibration bands, performance | T-095, T-096 |

## T-091: Environment data, weather timeline, exp table
- Spec: SPEC-009 sections 3 and 5 (the `exp_table` only).
- Work: `data/rules/environment.toml`, `data/scenarios/gettysburg/weather.toml` (rows with sources and placeholders as listed), `environment.schema.json`, `weather.schema.json` (and the sidecar `relief.schema.json` when T-094 lands); `EnvTable`, `WeatherTable::from_toml_str`, `WeatherNow` interpolation; `exp_table` const array, `qc/fixtures/ref_exp.py` and `qc/fixtures/ref_weather.py` with committed output; extend `tools/validate_data.py` to the two new schemas.
- Touches: `crates/battle-rules/src/environment.rs` (new), `lib.rs`, `data/`, `qc/fixtures/`, `tools/validators/environment.py`.
- Acceptance: **T9-01**, **T9-02**, **T9-03**.

## T-092: Heat and rain effects
- Spec: SPEC-009 section 7 (including SPEC-004 amendment A5 for section 4 and SPEC-005 amendment A8).
- Work: apparent temperature, heat curve interpolation, multiplied fatigue drain and recovery in `fatigue_step`, firing-drain multiplier, powder damp and ground wetness state, `Weather {wind, wet_pm, ground_pm}` filled from `WeatherNow`, `BattleState.env: Option<Environment>` and `set_environment`.
- Touches: `fatigue.rs`, `environment.rs`, `battle.rs`.
- Acceptance: **T9-12** (formula, neutral equals `None`), **T9-13** (damp and ground formulas, misfire rate at full damp), **T9-15** for the movement goldens.

## T-093: Smoke grid update and hash
- Spec: SPEC-009 section 4, section 9 (hash and tick order for smoke).
- Work: `add_smoke` with yield and cap, the pure snapshot update (advection, diffusion, retention, rain-scaled ceiling decay, edge loss), counters, bounding-rectangle trimming, `smoke_pm`, `total`, environment hash block (smoke counters and weather state), test hooks for reversed cell order and full-grid update.
- Touches: `smoke.rs`, `battle_hash.rs`, `battle.rs`.
- Acceptance: **T9-04**, **T9-05**, **T9-06**, **T9-07**, the smoke part of **T9-17**; **T5-17** unchanged with `env = None`.

## T-094: Relief grid and sight query
- Spec: SPEC-009 section 5.
- Work: `ElevationGrid::{from_rows, ground_mm}` (bilinear, `i128`), `to_mm`, `sight` with relief, obstacle and smoke terms, `signal_quality_pm`, eye and aim heights from posture.
- Touches: `sight.rs` (new), `terrain.rs` for the relief sidecar loader, `lib.rs`.
- Acceptance: **T9-08**, **T9-09**.

## T-095: Targeting gate, aim effects, peek
- Spec: SPEC-009 section 6 (with SPEC-004 amendment A6 for section 9 and SPEC-005 amendment A9 for the section 3 and 4 changes), section 9 (`F0` clarification, tick order).
- Work: gate with retries in the fire phase (no draw when holding fire), range-error multiplier from line transmit, `smoke_at_shooter` from `smoke_pm`, peek update every sight period, steering target offset, `Move` reset, `view_blocked`, `EnvInputs`, counters `sight_queries`, `shots_withheld`, `sight_blocked`, hash fields for peek and inputs.
- Touches: `fire.rs`, `aim.rs`, `battle.rs` steering, `environment.rs`, `battle_hash.rs`.
- Acceptance: **T9-10**, **T9-11**, the peek and input part of **T9-17**; **T5-14** and **T5-18** still pass with `env = None` and with `env = Some` in clear air.

## T-096: Observers, sighting log, signal quality
- Spec: SPEC-009 section 8.
- Work: `Observer`, `add_observer`, regiment observers, per-period schedule by id, sample selection, `Sighting`, `SightLog`, `sightings`, hash fields, vision range from `clear_vis_mm * light_pm`.
- Touches: `observers.rs` (new), `battle.rs`, `battle_hash.rs`.
- Acceptance: **T9-14**, the observer part of **T9-17**.

## T-097: Scenarios, goldens, calibration bands, performance
- Spec: SPEC-009 sections 10, 11 (T9-15 to T9-19), 12.
- Work: scenarios `env_smoke_line`, `env_hill_hq`, `env_heat_march`, `smoke_line`, `crest`; bands E1 to E8 in `qc/calibration/bands.toml` and `run_band_suite`; golden replays and `expected.json` rows; ignored perf test and `docs/cycle-reports/CYCLE-environment.md` with the numbers. If this overruns 400 lines, move the goldens to a follow-up ticket inserted before the band suite is frozen.
- Touches: `scenarios/env.rs`, `calibration/env.rs` (REGISTRY section 14), `qc/`, `docs/cycle-reports/`.
- Acceptance: **T9-15**, **T9-16**, **T9-17**, **T9-18** (recorded), **T9-19**.
