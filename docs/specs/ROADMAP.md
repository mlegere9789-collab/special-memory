# ROADMAP: dependency-ordered master ticket list

> Namespaces, ids and amendment labels are in [REGISTRY.md](REGISTRY.md); the collisions behind the ordering are in [CONFLICTS.md](CONFLICTS.md).

All 86 tickets still to build, in 12 waves. Every ticket of a wave can be built in its own worktree at the same time: a ticket starts only when everything in its `Needs` (the ticket tables, plus the registry dependencies below) is merged, and no two tickets of one wave change the same behavioural file. Tickets done already (`T-014` to `T-018`, `T-020`, `T-021`, and the hardening and review work in `ledger/usage.csv`) are not listed.

## How the waves were cut

- **Needs.** The `Needs` column of each `TICKETS-*.md` file, with the additions made by the registry (REGISTRY sections 8.2 and 18): `T-025` (calibration-target format) before any ticket that writes a data row with a `CT` id, `T-026` (shared constants) before any ticket that adds a tag, actor, stream, flag or magic, `T-027` (validator plugins and checker) before any ticket that adds a validated data family. They are wave 1, so they precede everything else by construction.
- **File contention.** One ticket per wave may rewrite each of `fire.rs`, `aim.rs`, `fatigue.rs`, `smoke.rs`, `slots.rs`, `hit.rs` and the steering file. One-line appends to `lib.rs`, root `Cargo.toml` and `Cargo.lock`, `battle.rs` (a field or one call in `step`), `battle_hash.rs` (one block call), `qc/golden-replays/expected.json` and `ledger/usage.csv` may share a wave; the orchestrator merges them mechanically and regenerates `Cargo.lock`. Each spec's scenarios, calibration and validators live in their own files (REGISTRY 14.2, T-027), so those never collide.
- **Added ordering (not a spec dependency).** `T-092`, `T-093`, `T-094` wait for `T-023` because they wire into the `step` that `T-023` creates; `T-091` waits for `T-141` for the retain rows of the new terrain classes; `T-164` waits for `T-121` because both hook the same shot and hit sites; `T-132` to `T-136` are serial because all edit `tools/gb_audit.py`; `T-085` follows `T-064` because both rewrite `fire.rs`.
- **Estimate class.** XS under 50 changed lines, S up to about 150, M up to about 300, L near the 400-line limit (likely to split at its data/code boundary; the ticket file says where). Usage follows class; the ledger records the actual.
- **Markers.** `[H]` a human step inside the ticket (licence evidence, source consultation, sign-off, live participants). `[N]` needs outside network or a data download that the build container may not have. `[P]` an ignored performance test whose numbers must be re-measured on reference hardware (container figures are provisional). `[D-nn]` waits for, or is shaped by, an open decision in CONFLICTS part D. `[CI]` needs the Linux, Windows and macOS runners of CI, which the container does not provide. No ticket in this list needs a GPU or a vendor; that work is in the off-cloud table at the end.

## Gates

Q0 is the local gate of `CLAUDE.md` for every ticket. Q1 is the pull request (all Q0 commands, `tools/check_registry.py` from wave 1 on). Q2 is the merge gate after each wave: `cargo test --workspace --release`, `bash tools/verify_goldens.sh` on the three platforms `[CI]`, scans and `validate_data.py`. A determinism mismatch on any platform stops the wave (P0). Q3 is the nightly run (from wave 6: the golden replays of every layer, from wave 7 the calibration smoke suite). Q4 is the cycle report. Q5 (human playtest) and Q6 (release, G7) follow wave 8 and are not tickets.

## Wave 1: foundations, nothing else waits on a ticket outside this wave

Gate: Q2; movement goldens `T4-12` recorded and verified on three platforms `[CI]`; `TR-01` to `TR-03` green.

| Ticket | Title | Class | Touches | Needs | Notes |
|---|---|---|---|---|---|
| T-019 | Steering, avoidance, anchor, movement scenarios | L | `battle-rules` steering and `scenarios/march`, `shells/sim-cli` (`battle`), `qc/`, `docs/cycle-reports/CYCLE-02-movement.md` | T-018 | splits at `hd-sim battle`; `[P]` |
| T-025 | CT identifier migration, ADR 0011 | S | `provenance.rs`, `data/rules/*.toml`, `data/schemas/`, `tools/test_validate_data.py`, `docs/adr/0011-*` | T-015 | registry |
| T-026 | Shared constants module | S | `battle-rules/src/registry.rs`, `lib.rs` | T-015 | registry |
| T-027 | Registry checker, validator plugins | S | `tools/check_registry.py`, `tools/validate_data.py`, `tools/validators/` | none | registry |
| T-130 | Source registry, time functions, ADR 0007 | S | `tools/gb_audit.py`, `data/history/gettysburg/sources.toml`, `docs/adr/0007-*` | none | `[H]` the Librarian rows beyond CWV stay `consulted = false` |
| T-162 | `perf-kit`: tiers, arrival, clusters, voices | S | `shells/perf-kit/`, root `Cargo.toml` | none | |

## Wave 2

Gate: Q2; `T5-11` to `T5-14` and `T5-17` (T-022); every new schema validates through its plugin; no existing golden changes.

| Ticket | Title | Class | Touches | Needs | Notes |
|---|---|---|---|---|---|
| T-022 | Fire phase | L | `fire.rs`, `aim.rs`, `smoke.rs`, `battle.rs`, `battle_hash.rs` | T-019, T-021 | splits at smoke |
| T-061 | Morale table, schema, pure helpers | M | `battle-rules/src/morale/`, `data/rules/morale.toml`, `morale.schema.json`, `qc/fixtures/ref_morale.py` | T-015, T-025, T-026 | |
| T-071 | Command data, loader, trait masks, amendment A2 | M | `command/{mod,data}.rs`, `data/rules/command.toml`, schemas, `tools/validators/command.py`, `docs/adr/0006-*`, SPEC-004 A2 | T-019, T-025, T-026, T-027 | |
| T-081 | Scenario format, loader, validation, hash | M | `scenario.rs`, `scenario.schema.json`, `data/scenarios/fid-small.toml`, `qc/fixtures/scenario-bad/`, `tools/validators/scenario.py` | T-018, T-025, T-027 | |
| T-101 | Trait extraction, AI tables, personality | M | `tools/extract_traits.py`, `data/rules/ai*.toml`, schemas, `crates/battle-ai/` skeleton | T-015, T-025, T-027 | |
| T-111 | Script schema, loader, compiler | M | `crates/history-cal/` skeleton, `replay-script.schema.json`, `qc/calibration/policy.toml`, `tools/validators/history_cal.py` | T-015, T-018, T-025, T-026, T-027 | `[D-06]` decide first |
| T-122 | Acoustics skeleton, integer math, tables | M | `crates/acoustics-model/`, `data/rules/acoustics.toml`, schema | T-025, T-027 | |
| T-131 | Corpus loader, structural rules, validator extension | M | `tools/gb_audit.py`, `tools/validators/gettysburg.py`, `qc/fixtures/gb/` | T-130, T-027 | |
| T-141 | Terrain amendment A3, source manifest, licence record | M | `data/rules/{terrain,obstacles}.toml`, `data/survey/`, `tools/terrain/ingest_dem.py`, `tools/validators/survey.py`, SPEC-004 | T-025, T-027 | `[H][N]` provider query, licence text, `cleared_by`; if coverage or licence fails, stop and amend SPEC-014 |
| T-161 | Provenance, clip and rights checkers, skeleton | M | `tools/check_clips.py`, `tools/check_rights.py`, `data/animation/`, `data/provenance/`, `tools/validators/assets.py` | T-027 | |
| T-163 | Percentile summariser, perf schemas, flights | M | `qc/perf/`, schemas, `tools/validators/perf.py` | T-025, T-027 | |

## Wave 3

Gate: Q2; `T5-08` to `T5-10`, `T5-15`, `T5-16`, `T5-18` (T-023); the loader tests `T7-01`, `T8-01`, `T9-01`, `T12-01` green; all T4 and T5 goldens unchanged.

| Ticket | Title | Class | Touches | Needs | Notes |
|---|---|---|---|---|---|
| T-023 | Hit, casualties, bursts, tick order | L | `hit.rs`, `damage.rs`, obstacles, `battle.rs` `step`, `battle_hash.rs`, SPEC-005 A5 | T-014, T-022 | splits at bursts |
| T-072 | Officers, dispatch codec, inputs, command hash block | M | `command/{state,codec,input}.rs`, `battle.rs` field, hash | T-071 | |
| T-082 | Fidelity table, classifier, sectors | M | `fidelity.rs`, `sector.rs`, `data/rules/fidelity.toml` | T-081 | |
| T-091 | Environment data, weather timeline, exp table | M | `environment.rs`, `data/rules/environment.toml`, `data/scenarios/gettysburg/weather.toml`, `qc/fixtures/ref_{exp,weather}.py` | T-015, T-016, T-025, T-027, T-141 | |
| T-123 | Propagation on flat ground | M | `propagation.rs`, `weather.rs`, `qc/fixtures/ref_acoustics.*` | T-122 | |
| T-132 | Evidence ladder, known-conflict fixtures | M | `tools/gb_audit.py`, `qc/fixtures/gb-known-conflicts.toml` | T-131 | |
| T-142 | `terrain` crate: tile codec, index, queries | M | `crates/terrain/`, `qc/fixtures/ref_height.py` | T-141 | |

## Wave 4: the F0 reference

Gate: Q2 and Q4. `T5-19` to `T5-21` (bands B1 to B11, determinism literals, performance) and `docs/cycle-reports/CYCLE-03-f0.md`. From this wave on, every optional layer must leave `T4-12` and `T5-20` unchanged with its option off.

| Ticket | Title | Class | Touches | Needs | Notes |
|---|---|---|---|---|---|
| T-024 | Calibration harness and the F0 reference | L | `calibration/fire.rs`, `scenarios/fire.rs`, `qc/calibration/`, `qc/golden-replays/`, cycle report | T-023 | `[P]` |
| T-062 | Morale inputs, ceiling, relaxation | M | `morale/{state,inputs,ceiling}.rs`, `battle.rs` field | T-061, T-023 | |
| T-073 | Media, latency, loss, garble | M | `command/{media,leg,garble}.rs`, `smoke.rs` (accessor, SPEC-005 A6), `qc/fixtures/ref_command.py` | T-072, T-022 | |
| T-075 | Observation, contacts, reports, pictures | M | `command/{observe,picture,report}.rs` | T-072 | `[D-03]` read visibility through a trait |
| T-083 | Aggregates, hand-overs, ledger, stream continuity | L | `aggregate.rs`, `regiment.rs`, small `fire.rs` helper, SPEC-004 A4, SPEC-005 A7 | T-023, T-082 | |
| T-092 | Heat and rain effects | M | `fatigue.rs`, `environment.rs`, `battle.rs` field, drain line in `fire.rs`, SPEC-004 A5, SPEC-005 A8 | T-091, T-022, T-023 | |
| T-093 | Smoke grid update and hash | M | `smoke.rs`, `battle_hash.rs` | T-091, T-022, T-023 | `[D-01]` hash content of tiles |
| T-094 | Relief grid and sight query | M | `sight.rs`, `terrain.rs` relief loader | T-091, T-023 | `[D-02]` source of heights |
| T-102 | `battle-ai` crate surface, `HqPort`, streams | M | `battle-ai/src/{port,ai,notes,outbox}.rs` | T-101, T-072, T-026 | |
| T-112 | Replay driver, tags `0x30` and `0x31`, history hash | M | `history-cal/src/{codec,driver}.rs`, `sim-cli` verify path | T-111, T-023 | `[D-06]` |
| T-124 | Terrain: barrier and foliage | M | `acoustics-model/src/relief.rs` | T-123 | `[D-02]` scene-local tables meanwhile |
| T-125 | Aggregation and camera packets | M | `acoustics-model/src/{aggregate,listener,packet}.rs` | T-123 | |
| T-133 | Chain sums, casualties, arms, terrain checks | M | `tools/gb_audit.py`, `qc/fixtures/gb/` | T-131 | serial after T-132 |
| T-143 | Obstacles and line of sight | M | `crates/terrain/` | T-142 | |
| T-144 | Survey geometry checks, `hd-terrain` compile | M | `shells/terrain-cli/`, survey schemas, `qc/fixtures/terrain/` | T-141, T-142 | splits at polygon checks |

## Wave 5

Gate: Q2; off-switch proofs `T6-14` (None half), `T7-16` (None half), `T9-15` all keep `T4-12` and `T5-20` unchanged.

| Ticket | Title | Class | Touches | Needs | Notes |
|---|---|---|---|---|---|
| T-063 | Morale state machine, rolls, events, hash | M | `morale/{step,transition,events}.rs`, `battle_hash.rs`, `step` call | T-062 | `[D-07]` ingestion of aggregate records |
| T-074 | Command chain, interpretation, status views | L | `command/{chain,interpret,status}.rs`, `step` call | T-073 | `[D-05]`, `[D-09]` |
| T-084 | `Battle` wrapper, replay binding, `hd-sim resolve` | L | `runner.rs`, `sim-cli`, SPEC-003 A2 | T-083 | |
| T-095 | Targeting gate, aim effects, peek | L | `fire.rs`, `aim.rs`, steering, `environment.rs`, SPEC-004 A6, SPEC-005 A9 | T-092, T-093, T-094 | `[D-02]` |
| T-096 | Observers, sighting log, signal quality | M | `observers.rs`, hash | T-094, T-093 | `[D-03]` |
| T-113 | Metrics, band evaluation, observations | M | `history-cal/src/{bands,metrics,observe}.rs` | T-112 | |
| T-121 | Sound log (SPEC-005 A10) | S | `sound_log.rs`, hooks in `fire.rs` and `hit.rs`, `step` start | T-024 | |
| T-126 | Hearing reports, scenes, bands, goldens | M | `acoustics-model/src/{report,scenes}.rs`, `qc/calibration/acoustics-bands.toml`, `expected-acoustics.json` | T-124, T-125 | `[CI]` |
| T-134 | Commanders, time ordering, hourly schedule | M | `tools/gb_audit.py` | T-131 | serial after T-133 |
| T-145 | Obstacle compile, adjustments, polish check | M | `shells/terrain-cli/` | T-143, T-144 | |
| T-151 | `ui-model` crate, snapshot, commands, display fields | M | `crates/ui-model/`, `data/ui/display-fields.toml`, `tools/validate_ui.py` | T-024, T-026, T-027 | |

## Wave 6

Gate: Q2; Q3 starts running the golden replays of morale, command and environment; `T6-07`, `T7-15`, `T9-19`, `T12-11`.

| Ticket | Title | Class | Touches | Needs | Notes |
|---|---|---|---|---|---|
| T-064 | Symptoms, flight orders, report, fire interplay | L | `morale/{symptoms,report}.rs`, `slots.rs`, `fire.rs`, `aim.rs`, `battle.rs`, SPEC-004 A7, SPEC-005 A11 | T-063 | `[D-04]` splits at the report |
| T-076 | Command scenario, bands, goldens, performance | M | `command/calibration.rs`, `scenarios/command.rs`, `sim-cli`, `qc/calibration/`, goldens | T-074, T-075 | `[P]` |
| T-097 | Environment scenarios, goldens, bands, performance | M | `scenarios/env.rs`, `calibration/env.rs`, `qc/` | T-095, T-096 | `[P]`; moves goldens to a follow-up if over 400 lines |
| T-114 | Gettysburg bands, timeline, conflict data | M | `qc/calibration/`, `qc/fixtures/`, `data/scenarios/gb_timeline/`, `history-cal/src/fixtures.rs` | T-111, T-113 | `[D-06]` |
| T-127 | Acoustics adapter, engine facade, performance | S | `adapter.rs`, `engine.rs`, cycle report | T-121, T-126 | `[P]` |
| T-135 | Audit reports, reconcile, training class | M | `tools/gb_audit.py`, `qc/audit/gettysburg/` | T-132, T-133, T-134 | |
| T-146 | Slab relief and land cover, first golden | L | `data/survey/`, `data/terrain/`, `qc/`, `tools/verify_goldens.sh` | T-145 | `[N][H]` needs the cleared source; splits by feature kind; `[CI]` |
| T-152 | Information views: confidence, status, morale | M | `ui-model/src/{confidence,status,morale}.rs`, `data/ui/*.toml` | T-151 | `[D-04][D-05]` |
| T-153 | Overlays, march estimate, overnight flow | M | `ui-model/src/{overlay,march,overnight}.rs` | T-151 | forecast runner is an injected closure |
| T-154 | Accessibility core | M | `ui-model/src/{bindings,flash,captions,focus}.rs`, `data/ui/palettes.toml`, `tools/validate_ui.py` | T-151 | |
| T-155 | Localisation | M | `ui-model/src/text.rs`, `data/text/en/`, `tools/lint_strings.py` | T-151 | |
| T-164 | Event drain (SPEC-005 A12) | S | `battle.rs`, hooks in `fire.rs` and `hit.rs` | T-024, T-121 | |

## Wave 7

Gate: Q2; `cal run --suite smoke` exits 0 (`T11-12`) `[CI]`; `hd-sim verify` accepts every golden `kind`; `T6-13`, `T6-14` golden.

| Ticket | Title | Class | Touches | Needs | Notes |
|---|---|---|---|---|---|
| T-065 | Morale calibration, bands, goldens | M | `calibration/morale.rs`, `scenarios/morale.rs`, `qc/calibration/`, `sim-cli` (`--morale`) | T-064, T-024 | |
| T-085 | F1 bundled volleys and the fit tool | M | `f1.rs`, `fire.rs`, `sim-cli` (`fit-f1`), `data/rules/f1.toml` | T-024, T-084 | needs the `f0-curves.json` of T-024 |
| T-103 | Belief builder, forecast, budget, SPEC-004 A8 and SPEC-005 A13 | L | `battle-ai/src/{lookahead,belief}.rs`, small setters in `battle-rules`, `qc/fixtures/ref_budget.py` | T-102, T-024, T-076 | |
| T-109 | `HqPort` host adapter | S | `shells/sim-cli/src/hq.rs` | T-102, T-076 | |
| T-115 | Suite runner, reports, `hd-sim cal`, CI | L | `history-cal/src/{suite,report}.rs`, `sim-cli/src/cal.rs`, `.github/workflows/ci.yml`, goldens, `CAL-01.md` | T-024, T-113, T-114 | `[CI][P]` |
| T-136 | Pilot records from the research volume | M | `data/history/gettysburg/`, `qc/audit/gettysburg/AP-pilot.*` | T-135 | `[H]` Reviewer block, historian sign-off |
| T-147 | Slab linear features, buildings, names, vantage rows | L | `data/survey/`, `data/terrain/`, `qc/terrain/` | T-146 | `[H]` |
| T-156 | Difficulty and the tutorial engine | L | `ui-model/src/{difficulty,tutorial}.rs`, `data/ui/`, `data/scenarios/drill_ground.toml` | T-152, T-153, T-155 | `[D-08]` splits difficulty first |
| T-157 | Replay keyframes, photo sidecar, guide data | M | `ui-model/src/{camera,replay_view,photo}.rs`, `data/guide/` | T-151, T-155 | `[H]` guide text review stays `pending` |
| T-159 | UI adapters over SPEC-006 and SPEC-007 | S | `ui-model/src/adapters.rs`, `data/ui/morale.toml` | T-152, T-064, T-076 | `[D-04][D-05]` |
| T-165 | `sim-capi`: layouts, stepping, snapshots, ADR 0008 | M | `shells/sim-capi/`, `include/hd_sim.h`, `docs/adr/0008-*` | T-164 | `[CI]` for the three platforms |

## Wave 8

Gate: Q2 and Q4 (morale and terrain slab reports); H6 (T-066) and H14 (T-148) are recorded.

| Ticket | Title | Class | Touches | Needs | Notes |
|---|---|---|---|---|---|
| T-066 | Morale performance report and scenario switch | S | `crates/battle-rules/tests`, cycle report, `data/scenarios` | T-065 | `[P]` |
| T-086 | F2 exchange and aggregate movement | M | `aggregate.rs`, `runner.rs` | T-085 | `[D-07]` |
| T-104 | Operational layer | M | `battle-ai/src/{operational,sectors,scenarios}.rs` | T-103 | |
| T-116 | Fit and hold-out tooling | M | `history-cal/src/{fit,holdout}.rs`, `qc/calibration/*.toml` | T-115 | |
| T-148 | Streaming proof, legacy export, performance | M | `crates/terrain/`, `shells/terrain-cli/`, cycle report | T-147 | `[P]` |
| T-158 | Usability protocol and report tool | S | `qc/usability/`, `tools/usability_report.py` | T-156 | `[H]` the first live round (at least 10 testers) is off-cloud |
| T-166 | `sim-capi`: orders, replay, smoke view, error fuzz | M | `shells/sim-capi/` | T-165 | |

## Wave 9

Gate: Q2; `T8-15` to `T8-20` (scale scenario, goldens) `[CI]`; the C interface checks `T16-01` to `T16-06`.

| Ticket | Title | Class | Touches | Needs | Notes |
|---|---|---|---|---|---|
| T-087 | Sector streaming, scale scenario, goldens, performance | L | `hit.rs`, `smoke.rs`, grid, `scenarios/fidelity.rs`, `qc/golden-replays`, `CYCLE-04-fidelity.md`, SPEC-002 A3 (if needed), SPEC-005 A15 | T-086 | `[D-01][P][CI]` |
| T-105 | Tactical layer (T-105a routes and chokes, T-105b placement, works, line of sight) | L | `battle-ai/src/{tactical,placement,route,los}.rs` | T-104 | `[D-02]` |
| T-167 | M1 and vendor pack: templates, ADR checker | M | `docs/adr/`, `docs/cycle-reports/`, `docs/briefs/`, `tools/check_m1_adr.py` | T-161, T-163, T-166 | output feeds the off-cloud lane |

## Wave 10

Gate: Q2; failure-suite goldens `T10-33`.

| Ticket | Title | Class | Touches | Needs | Notes |
|---|---|---|---|---|---|
| T-106 | Failure suite, guards, `obstacle_stops` (SPEC-005 A14) | L | `battle-ai/src/{guards,scenarios}.rs`, `battle.rs` counter | T-105 | |

## Wave 11

| Ticket | Title | Class | Touches | Needs | Notes |
|---|---|---|---|---|---|
| T-107 | Documented-behaviour scenarios | L | `battle-ai/src/{gates,batteries,scenarios}.rs` | T-106 | `[D-09]`; splits at the gates |

## Wave 12

Gate: Q2 and Q4: `T10-05`, `T10-10`, `T10-11`, `T10-12`, `T10-13` (H10), AI goldens verified on three platforms `[CI]`; `docs/cycle-reports/CYCLE-ai.md`.

| Ticket | Title | Class | Touches | Needs | Notes |
|---|---|---|---|---|---|
| T-108 | Difficulty bands, advisor, `ai-verify`, goldens, performance | M | `battle-ai/src/{advisor,difficulty}.rs`, `sim-cli`, `qc/golden-replays/`, cycle report | T-107, T-109 | `[D-08][P][CI]` |

## Critical path and the tail

The long chain is the SPEC-010 AI: `T-072` (wave 3) to `T-073`, `T-074` (5), `T-076` (6), then `T-103` to `T-108` one per wave, because `T-103` needs SPEC-007 merged as the AI ticket file states. Relaxing `T-103` to need only `T-072` (the types and codec that `HqPort` adapts) and `T-024` would start the AI chain at wave 5 and end the roadmap at wave 10; that is a change to `TICKETS-010.md` for the orchestrator to approve, and it was not made here. Waves 10 to 12 hold one ticket each by dependency; nothing else is waiting on them, so spare capacity there can take the follow-up tickets that overrun tickets will split into.

Open decisions (CONFLICTS part D) and the first wave they affect: D-06 (before wave 2, `T-111`), D-07 (before wave 5, `T-063`), D-02 and D-03 (before wave 4, `T-094`, `T-075`), D-04 and D-05 (before wave 5, `T-074`; before wave 6, `T-064`), D-01 (before wave 9), D-08 (before wave 7, `T-156`), D-09 (before wave 11), D-10 (before the M1 gate).

## Off-cloud lane (no ticket; cannot be done in the cloud container)

| Item | Needs | Where it comes from |
|---|---|---|
| M1 renderer prototype on RM-R and RM-M machines, scenes S0 to S6, `T16-14` to `T16-18`, `CYCLE-M1-renderer`, ADR 0009 | a GPU machine, the snapshots of `f0_large` (T-024), `sim-capi` (T-165, T-166), the pack of T-167 | SPEC-016 sections 3 to 5, 11 |
| Animation capture (about 300 clips, 12 pilot clips first), performers, safety, consent | vendor, performers, counsel review of contracts, the checkers of T-161 | SPEC-016 section 6 |
| Audio recording, foley, voice, music | vendor, powder permits, range access, the checkers of T-161 | SPEC-016 section 7 |
| RFQ rounds (art, capture, audio, QA) | vendors, counsel, the pack of T-167 | SPEC-016 section 8 |
| Rights, contracts, gates G5 to G7, patent landscape | counsel | SPEC-016 section 9, `G0-charter.md` |
| Licence clearance of the elevation source, provider query, `cleared_by` | human with network access (inside T-141) | SPEC-014 section 3 |
| Elevation download and ingest with pinned GDAL (inside T-146) | network and a data volume | SPEC-014 section 3 |
| Consulting the candidate sources of SPEC-013, permission letters, historian sign-off; the audit packets themselves (Auditor, blind auditor, reconciler, reviewer sessions) are opened by the orchestrator after T-135 and are not counted above | human librarian and historian | SPEC-013 sections 4, 8 |
| Usability rounds (at least 10 new testers each) | participants | SPEC-015 section 11, T-158 |
| Q5 human playtests, Q6 release clearance | humans, counsel | `CLAUDE.md` gates |
| Three-platform determinism proof and nightly runs | the Linux, Windows and macOS runners of CI | ADR 0001 |
| Re-measuring every `[P]` figure on reference hardware | a quiet machine of the recorded class | each spec's performance section |
| Optional language-model advisor | a model host outside the ship build | SPEC-010 section 10 |
