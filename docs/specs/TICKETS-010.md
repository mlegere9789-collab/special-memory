# TICKETS-010: battle AI, tickets T-A1 to T-A8

Spec: SPEC-010 (battle AI). One ticket, one session, under about 400 changed lines (code, tests, data together). Each ticket writes its tests first, ends with the Q0 commands from `CLAUDE.md`, and appends one row to `ledger/usage.csv`. A ticket that cannot fit splits at the data/code boundary; it never drops or weakens a listed test. A ticket starts only when everything in `Needs` is merged. SPEC-004, SPEC-005 and SPEC-007 must be merged first for T-A3 onward; ticket numbers here are provisional until the orchestrator assigns the project-wide T-NNN range.

| Ticket | Title | Needs |
|---|---|---|
| T-A1 | Trait extraction, AI tables and personality | SPEC-004 loader |
| T-A2 | `battle-ai` crate, `HqPort`, AI streams, message discipline | T-A1, SPEC-007 interface |
| T-A3 | Belief builder, forecast, budget rule, oracle test | T-A2, `BattleState` clone and setters |
| T-A4 | Operational layer: sectors, objectives, assignment, reserves, timing | T-A3 |
| T-A5 | Tactical layer: placement, lanes, routes, chokes, works, line of sight | T-A4 |
| T-A6 | Failure suite scenarios, reason codes, retreat and reform guards | T-A5 |
| T-A7 | Documented-behaviour scenarios (Gettysburg pages): discretion, pursuit, delay, guns, penetration, screen | T-A6 |
| T-A8 | Difficulty bands, advisor, `ai-verify`, goldens, performance report | T-A7 |

## T-A1: Trait extraction, AI tables, personality
- Spec: SPEC-010 sections 4 (all), 14.
- Work: personality rows carry the commander-record act that fixes each delta's sign (note field, CWV page and tag where the record gives one); `tools/extract_traits.py` writing `data/rules/ai_traits.toml` from `data/history/commanders/*.toml`; `data/rules/ai.toml` (globals, difficulty rows, trait deltas, each row with provenance and `CT-n`); `data/schemas/ai.schema.json`, `ai_traits.schema.json`; `AiTables::{from_toml_str, load_dir}` and `personality_for` in the new crate skeleton (workspace entry).
- Files: `tools/`, `data/rules/`, `data/schemas/`, `crates/battle-ai/{Cargo.toml,src/lib.rs,src/tables.rs,src/personality.rs}`, `Cargo.toml`.
- Acceptance: **T10-01**, **T10-02** (the Meade, Ewell, Lee and Hancock values come from the committed records). Any `verified` row needs `second_source`.

## T-A2: Crate surface, `HqPort`, streams, message discipline
- Spec: SPEC-010 sections 3, 6 (Messages, Explanation), 9 (streams, `Ai::hash`).
- Work: section 3 types and trait (or adapters onto the SPEC-007 names), `STREAM_AI_BASE`, `AI_ACTOR_BASE`, `AiNote`, `ReasonCode`, outbox with think delay, ack tracking, resend and escalation, `Ai::{new, step, hash, notes, explain}` with empty planners; a scripted test `HqPort`.
- Files: `crates/battle-ai/src/{port.rs,ai.rs,notes.rs,outbox.rs}`.
- Acceptance: **T10-06**, the message half of **T10-31** (unit level, scripted port), **T10-04** (the `BattleState` grep and codec checks).

## T-A3: Belief, forecast, budget
- Spec: SPEC-010 sections 7, 8.
- Work: amend SPEC-004 and SPEC-005 first if `BattleState: Clone`, fatigue and rounds setters are missing (spec amendment, then tests, then code); `build_belief`, `forecast`, enemy models, score and aggregate, horizon formula, `qc/fixtures/ref_budget.py` (independent `fractions` reference for the horizon tuples, output committed).
- Files: `crates/battle-ai/src/{lookahead.rs,belief.rs}`, small additions to `crates/battle-rules` if the amendment demands them, `qc/fixtures/`.
- Acceptance: **T10-07** (hash equality with truth belief), **T10-08**, **T10-03** (fog invariance, first version on a fixed two-regiment scene).

## T-A4: Operational layer
- Spec: SPEC-010 section 5.
- Work: sector map with estimate noise, objectives, need rule, greedy assignment, candidate variants, forecast-based choice, departure timing, reserve release with the feud rule, candidates `Stay, Attack, Withdraw, Concentrate`, work-adjusted Hold need and surplus lending, `Screen` assignment for cavalry, objective diffing; `scenarios::ai_duel` (mirrored ground, equal forces).
- Files: `crates/battle-ai/src/{operational.rs,sectors.rs,scenarios.rs}`.
- Acceptance: **T10-09**, **T10-27** (score components logged and recomputable).

## T-A5: Tactical layer
- Spec: SPEC-010 section 6 (Placement, Lanes, Routes and chokes, Terrain, Works, Line of sight).
- Work: placement scoring (guns excluded from woods, thicket, marsh; canister-range exposure), lane layout, A* with integer costs, choke detection (fords and gaps) and staging, stall watchdog, work handling with `work_ratio`, `line_clear`, fire-target choice. Over 400 lines splits into routes and chokes (T-A5a) and placement, works and line of sight (T-A5b).
- Files: `crates/battle-ai/src/{tactical.rs,placement.rs,route.rs,los.rs}`.
- Acceptance: unit tests for placement ties, A* determinism and `line_clear` against a brute-force reference; **T10-21**, **T10-22**, **T10-23** run on `fs_blob`, `fs_choke`, `fs_terrain`.

## T-A6: Failure suite completion and guards
- Spec: SPEC-010 sections 6 (Reform guard, Retreat gate with `Bled` and the rally point), 13 (failure suite), 15 risks 3 and 4.
- Work: `OwnReport::strength_ref`; scenarios `fs_hold`, `fs_outnumbered`, `fs_works`, `fs_los`, `fs_reform`, `fs_courier`, `fs_wavering`; amend SPEC-005 section 7 to add the `obstacle_stops` counter, then implement it; retreat gate and reform guard; circle detector in the test harness.
- Files: `crates/battle-ai/src/{guards.rs,scenarios.rs}`, `crates/battle-rules/src/{battle.rs}` (counter only), `tests/`.
- Acceptance: **T10-24**, **T10-25**, **T10-26**, **T10-28**, **T10-30**, **T10-31**, **T10-32**.

## T-A7: Documented-behaviour scenarios
- Spec: SPEC-010 section 6 (Discretion gate, Counterattack gate, Delay, Penetration, Guns and screen), section 13 T10-34 to T10-39, pages CWV p065, p080, p082, p084, p085.
- Work: `ObjectiveBody::discretionary`, the two gates, `Delay`, penetration commit, battery rules (support distance, counter-battery, `Cease`), `Screen` behaviour; scenarios `fs_practicable`, `fs_pursuit`, `fs_delay`, `fs_guns`, `fs_penetration`, `fs_screen`, each run for two seats built from committed commander records.
- Files: `crates/battle-ai/src/{gates.rs,batteries.rs,scenarios.rs}`, `tests/`.
- Acceptance: **T10-34**, **T10-35**, **T10-36**, **T10-37**, **T10-38**, **T10-39**. Contested accounts are tested as behaviour only. Over 400 lines splits at the gates (T10-34 to T10-36) and the field rules (T10-37 to T10-39).

## T-A8: Difficulty, advisor, replay verification, goldens
- Spec: SPEC-010 sections 4 (difficulty), 9, 10, 12, 13.
- Work: difficulty wiring, `parse_advisor_reply` and advisor recording, `hd-sim battle --ai` and `ai-verify`, AI golden replays and `expected.json` rows, the `ai_duel` band runs, the ignored performance test and `docs/cycle-reports/CYCLE-AI-01.md` with measured numbers against H6.
- Files: `crates/battle-ai/src/{advisor.rs,difficulty.rs}`, `shells/sim-cli/`, `qc/golden-replays/`, `docs/cycle-reports/`.
- Acceptance: **T10-05**, **T10-10**, **T10-11**, **T10-12** (including the adversarial replies and the `AdvisorRejected` note), **T10-13**, **T10-29**, **T10-33**. Difficulty rows are tuned on placeholder data, never on the T10-10 bands.
