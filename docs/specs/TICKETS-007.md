# TICKETS-007: command friction and information model

Spec: SPEC-007 (command and information). One ticket, one session, under about 400 changed lines (code, tests, data together). Each ticket writes its tests first, ends with the Q0 commands from `CLAUDE.md`, and appends one row to `ledger/usage.csv`. A ticket that cannot fit splits at the data/code boundary; it never drops or weakens a listed test. Dependency order; a ticket starts only when everything in `Needs` is merged. Ids `TC-n` are mapped to `T-nnn` numbers by the orchestrator when scheduled. All code lives in `crates/battle-rules/src/command/` unless stated.

| Ticket | Title | Needs |
|---|---|---|
| TC-1 | Spec amendments, data, schemas, loader, trait masks | T-019 (movement merged) |
| TC-2 | Officers, dispatch codec, options and script inputs, state hash block | TC-1 |
| TC-3 | Media, latency, loss, capture, garble, streams | TC-2, T-022 (smoke grid) |
| TC-4 | Arrival, staff delay, interpretation, ack, chain fan-out, status views | TC-3 |
| TC-5 | Observation, contacts, reports, picture views, visibility | TC-2 |
| TC-6 | Scenario, calibration bands, goldens, performance | TC-4, TC-5 |

## TC-1: Amendments, data and loader
- Spec: SPEC-007 sections 3, 9 (amendment text only).
- Work: first amend SPEC-004 section 11 (conditional command hash block, order wire tags 0x10 to 0x13 reserved, direct-order refusal when command is enabled) and SPEC-005 section 7 (`step_command` first); then `data/rules/command.toml`, `data/schemas/command.schema.json`, `data/schemas/officers.schema.json`, `CommandTable::from_toml_str`, `trait_mask`, `load_officers` (cycle, level, double-owner, two-army checks), `tools/validate_data.py` extension for both schemas, ADR 0005 (command layer is optional state, hash block conditional).
- Acceptance: **T7-01**.
- Files: `crates/battle-rules/src/command/{mod,data}.rs`, `data/rules/command.toml`, `data/schemas/`, `docs/specs/SPEC-004*`, `SPEC-005*`, `docs/adr/0005-*`, `tools/validate_data.py`.

## TC-2: Officers, codec, inputs, hash
- Spec: SPEC-007 sections 2, 4, 9, 11 (types and codec).
- Work: `OfficerId`, `MessageId`, `Level`, `Medium`, `Latitude`, `TraitMask`, `Officer`, `CommandState`, `CommandOptions`, `encode_dispatch`/`decode_dispatch`, options and script payloads, `BattleState.cmd`, `enable_command`, `apply_command_input` (side checks, direct-order refusal, option and script effects), officer position refresh, hash block.
- Acceptance: **T7-02**; the direct-order refusal part of **T7-11**; hash-field-sensitivity half of **T7-16** (changing any hashed field changes the hash; `cmd = None` leaves SPEC-004 and SPEC-005 goldens unchanged).
- Files: `command/{state,codec,input}.rs`, `battle.rs` (field and hash), `qc/golden-replays/expected.json` untouched.

## TC-3: Media, latency, loss, garble
- Spec: SPEC-007 sections 5, 8 (streams).
- Work: `estimate`, courier legs with the eight-point terrain mean, flag and telegraph rules, smoke wait and fallback (add `SmokeGrid::density_at` if absent), per-leg draws, loss and capture with `query_segment`, arrival with extra legs, reading and garbling, stale rule, redirect to deputy, `qc/fixtures/ref_command.py` and its committed output.
- Acceptance: **T7-03**, **T7-04**, **T7-05**, **T7-06**, **T7-13**, **T7-14**.
- Files: `command/{media,leg,garble}.rs`, `smoke.rs` (accessor), `qc/fixtures/ref_command.py`, `qc/fixtures/ref_command.out`.

## TC-4: Chain, interpretation, status
- Spec: SPEC-007 sections 6, 8 (status, views).
- Work: staff completion queue, `interpret` (go score, reach, gait shift, decline), ack with `received`, `executing`, `ModKind`, fan-out to subordinates preserving formation shape, `issue_order` at regiment level, `OrderRecord`, `OrderStatus` derivation, `order_views`, `dispatch` with `DispatchError`, `instant_orders` path.
- Acceptance: **T7-07**, **T7-08**, **T7-09**, **T7-12**, and the instant-order and AI-parity parts of **T7-11**.
- Files: `command/{chain,interpret,status}.rs`.

## TC-5: Observation and pictures
- Spec: SPEC-007 section 7.
- Work: observation with quality, cover and smoke terms and `STREAM_OBSERVE`, contacts with association, decay and drop, friend entries, report build and merge, `picture_view`, `omniscient_view`.
- Acceptance: **T7-10**; the picture and omniscient parts of **T7-11**.
- Files: `command/{observe,picture,report}.rs`.

## TC-6: Scenario, bands, goldens, performance
- Spec: SPEC-007 sections 10, 12, 13.
- Work: `command_chain` scenario (3 x 3 x 3 regiments, one link of each medium) and a `cemetery_hill` calibration scenario in `battle_rules::scenarios`; `hd-sim battle --scenario command_chain`; `calibration::run_command_bands` with `qc/calibration/command-bands.toml`; golden replay row; ignored perf test; cycle report `docs/cycle-reports/CYCLE-xx-command.md`. If it overruns 400 lines, move the perf test and report to a follow-up.
- Acceptance: **T7-15**, **T7-16** (golden literal recorded once on Linux, verified on three platforms), ignored **T7-17**.
- Files: `command/calibration.rs`, `scenarios.rs`, `shells/sim-cli`, `qc/calibration/`, `qc/golden-replays/`, `docs/cycle-reports/`.
