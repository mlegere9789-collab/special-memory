# TICKETS-010: battle AI, tickets T-101 to T-108

> Shared namespaces (input tags and actors, random streams, flags, hash blocks, tick order, CT ids, amendment labels, ticket ids) are owned by [REGISTRY.md](REGISTRY.md); conflicts and their resolutions are in [CONFLICTS.md](CONFLICTS.md).

Spec: SPEC-010 (battle AI). One ticket, one session, under about 400 changed lines (code, tests, data together). Each ticket writes its tests first, ends with the Q0 commands from `CLAUDE.md`, and appends one row to `ledger/usage.csv`. A ticket that cannot fit splits at the data/code boundary; it never drops or weakens a listed test. A ticket starts only when everything in `Needs` is merged. SPEC-004, SPEC-005 and SPEC-007 must be merged first for T-103 onward; ticket ids are T-101 to T-108, plus T-109 added by the registry (REGISTRY section 8).

| Ticket | Title | Needs |
|---|---|---|
| T-101 | Trait extraction, AI tables and personality | T-015, T-025, T-027 |
| T-102 | `battle-ai` crate, `HqPort`, AI streams, message discipline | T-101, T-072 (SPEC-007 types and codec), T-026 |
| T-103 | Belief builder, forecast, budget rule, oracle test | T-102, T-024, T-076 (SPEC-004, SPEC-005 and SPEC-007 merged) |
| T-104 | Operational layer: sectors, objectives, assignment, reserves, timing | T-103 |
| T-105 | Tactical layer: placement, lanes, routes, chokes, works, line of sight | T-104 |
| T-106 | Failure suite scenarios, reason codes, retreat and reform guards | T-105 |
| T-107 | Documented-behaviour scenarios (Gettysburg pages): discretion, pursuit, delay, guns, penetration, screen | T-106 |
| T-108 | Difficulty bands, advisor, `ai-verify`, goldens, performance report | T-107, T-109 |
| T-109 | `HqPort` host adapter over the SPEC-007 views | T-102, T-076 |

## T-101: Trait extraction, AI tables, personality
- Spec: SPEC-010 sections 4 (all), 14.
- Work: personality rows carry the commander-record act that fixes each delta's sign (note field, CWV page and tag where the record gives one); `tools/extract_traits.py` writing `data/rules/ai_traits.toml` from `data/history/commanders/*.toml`; `data/rules/ai.toml` (globals, difficulty rows, trait deltas, each row with provenance and `CT-010-n`); `data/schemas/ai.schema.json`, `ai_traits.schema.json`; `AiTables::{from_toml_str, load_dir}` and `personality_for` in the new crate skeleton (workspace entry).
- Files: `tools/`, `data/rules/`, `data/schemas/`, `crates/battle-ai/{Cargo.toml,src/lib.rs,src/tables.rs,src/personality.rs}`, `Cargo.toml`.
- Acceptance: **T10-01**, **T10-02** (the Meade, Ewell, Lee and Hancock values come from the committed records). Any `verified` row needs `second_source`.

## T-102: Crate surface, `HqPort`, streams, message discipline
- Spec: SPEC-010 sections 3, 6 (Messages, Explanation), 9 (streams, `Ai::hash`).
- Work: section 3 types and trait (or adapters onto the SPEC-007 names), `STREAM_AI_BASE`, `AI_ACTOR_BASE`, `AiNote`, `ReasonCode`, outbox with think delay, ack tracking, resend and escalation, `Ai::{new, step, hash, notes, explain}` with empty planners; a scripted test `HqPort`.
- Files: `crates/battle-ai/src/{port.rs,ai.rs,notes.rs,outbox.rs}`.
- Acceptance: **T10-06**, the message half of **T10-31** (unit level, scripted port), **T10-04** (the `BattleState` grep and codec checks).

## T-103: Belief, forecast, budget
- Spec: SPEC-010 sections 7, 8.
- Work: write SPEC-004 amendment A8 and SPEC-005 amendment A13 first (public setters for fatigue, strength and rounds and the read accessors the belief builder needs; `BattleState` already derives `Clone`), then tests, then code; `build_belief`, `forecast`, enemy models, score and aggregate, horizon formula, `qc/fixtures/ref_budget.py` (independent `fractions` reference for the horizon tuples, output committed).
- Files: `crates/battle-ai/src/{lookahead.rs,belief.rs}`, small additions to `crates/battle-rules` if the amendment demands them, `qc/fixtures/`.
- Acceptance: **T10-07** (hash equality with truth belief), **T10-08**, **T10-03** (fog invariance, first version on a fixed two-regiment scene).

## T-104: Operational layer
- Spec: SPEC-010 section 5.
- Work: sector map with estimate noise, objectives, need rule, greedy assignment, candidate variants, forecast-based choice, departure timing, reserve release with the feud rule, candidates `Stay, Attack, Withdraw, Concentrate`, work-adjusted Hold need and surplus lending, `Screen` assignment for cavalry, objective diffing; `scenarios::ai_duel` (mirrored ground, equal forces).
- Files: `crates/battle-ai/src/{operational.rs,sectors.rs,scenarios.rs}`.
- Acceptance: **T10-09**, **T10-27** (score components logged and recomputable).

## T-105: Tactical layer
- Spec: SPEC-010 section 6 (Placement, Lanes, Routes and chokes, Terrain, Works, Line of sight).
- Work: placement scoring (guns excluded from woods, thicket, marsh; canister-range exposure), lane layout, A* with integer costs, choke detection (fords and gaps) and staging, stall watchdog, work handling with `work_ratio`, `line_clear`, fire-target choice. Over 400 lines splits into routes and chokes (T-105a) and placement, works and line of sight (T-105b).
- Files: `crates/battle-ai/src/{tactical.rs,placement.rs,route.rs,los.rs}`.
- Acceptance: unit tests for placement ties, A* determinism and `line_clear` against a brute-force reference; **T10-21**, **T10-22**, **T10-23** run on `fs_blob`, `fs_choke`, `fs_terrain`.

## T-106: Failure suite completion and guards
- Spec: SPEC-010 sections 6 (Reform guard, Retreat gate with `Bled` and the rally point), 13 (failure suite), 15 risks 3 and 4.
- Work: `OwnReport::strength_ref`; scenarios `fs_hold`, `fs_outnumbered`, `fs_works`, `fs_los`, `fs_reform`, `fs_courier`, `fs_wavering`; write SPEC-005 amendment A14 (the `obstacle_stops` counter exposed by `counters()`) first, then implement it; retreat gate and reform guard; circle detector in the test harness.
- Files: `crates/battle-ai/src/{guards.rs,scenarios.rs}`, `crates/battle-rules/src/{battle.rs}` (counter only), `tests/`.
- Acceptance: **T10-24**, **T10-25**, **T10-26**, **T10-28**, **T10-30**, **T10-31**, **T10-32**.

## T-107: Documented-behaviour scenarios
- Spec: SPEC-010 section 6 (Discretion gate, Counterattack gate, Delay, Penetration, Guns and screen), section 13 T10-34 to T10-39, pages CWV p065, p080, p082, p084, p085.
- Work: `ObjectiveBody::discretionary`, the two gates, `Delay`, penetration commit, battery rules (support distance, counter-battery, `Cease`), `Screen` behaviour; scenarios `fs_practicable`, `fs_pursuit`, `fs_delay`, `fs_guns`, `fs_penetration`, `fs_screen`, each run for two seats built from committed commander records.
- Files: `crates/battle-ai/src/{gates.rs,batteries.rs,scenarios.rs}`, `tests/`.
- Acceptance: **T10-34**, **T10-35**, **T10-36**, **T10-37**, **T10-38**, **T10-39**. Contested accounts are tested as behaviour only. Over 400 lines splits at the gates (T10-34 to T10-36) and the field rules (T10-37 to T10-39).

## T-108: Difficulty, advisor, replay verification, goldens
- Spec: SPEC-010 sections 4 (difficulty), 9, 10, 12, 13.
- Work: difficulty wiring, `parse_advisor_reply` and advisor recording, `hd-sim battle --ai` and `ai-verify`, AI golden replays and `expected.json` rows, the `ai_duel` band runs, the ignored performance test and `docs/cycle-reports/CYCLE-AI-01.md` with measured numbers against H10 (SPEC-010 section 12).
- Files: `crates/battle-ai/src/{advisor.rs,difficulty.rs}`, `shells/sim-cli/`, `qc/golden-replays/`, `docs/cycle-reports/`.
- Acceptance: **T10-05**, **T10-10**, **T10-11**, **T10-12** (including the adversarial replies and the `AdvisorRejected` note), **T10-13**, **T10-29**, **T10-33**. Difficulty rows are tuned on placeholder data, never on the T10-10 bands.

## T-109: HqPort host adapter
- Spec: SPEC-010 section 3, SPEC-007 sections 7 and 8 (`picture_view`, `order_views`, `dispatch`); added by the registry because SPEC-010 requires "SPEC-007 names or thin adapters" and T10-04 forbids `BattleState` outside `lookahead.rs`, so the adapter cannot live in `battle-ai`.
- Work: `BattleHq` in `shells/sim-cli/src/hq.rs` implementing `HqPort` over `BattleState` and `CommandState`: `own_reports` from the seat officer's friend entries, `contacts` from `picture_view` (ids are contact ids, never `RegimentId`), `map` from the terrain grid and obstacle layer, `submit` through `dispatch` as actor `0x80 | side`, `status` as Pending or Acked from `order_views`; `OutMessage` bodies `Reg` map to `Order`, `Fire` to the fire-mode payload (tag 5), and `Objective` to the documented fallback (`Move` plus `Hold`, SPEC-010 section 15 risk 4).
- Files: `shells/sim-cli/src/hq.rs`, tests.
- Acceptance: the adapter passes the T10-11 call-log check (only section 3 calls, identical `contacts()` data for equal pictures); an AI order and a human order with equal content deliver on equal ticks (T7-11 parity); no enemy `RegimentId` crosses the port.

