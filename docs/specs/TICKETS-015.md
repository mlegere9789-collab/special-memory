# TICKETS-015: interface layer, tickets T-151 to T-158

> Shared namespaces (input tags and actors, random streams, flags, hash blocks, tick order, CT ids, amendment labels, ticket ids) are owned by [REGISTRY.md](REGISTRY.md); conflicts and their resolutions are in [CONFLICTS.md](CONFLICTS.md).

Spec: SPEC-015 (interface, tutorial, accessibility and localisation). Ticket ids are T-151 to T-158, plus T-159 added by the registry (REGISTRY section 8). One ticket, one session, under about 400 changed lines (code, tests, data together). Each ticket writes its tests first, ends with the Q0 commands from `CLAUDE.md`, and appends one row to `ledger/usage.csv`. A ticket that cannot fit splits at the data/code boundary; it never drops or weakens a listed test. A ticket starts only when everything in `Needs` is merged.

| Ticket | Title | Needs |
|---|---|---|
| T-151 | ui-model crate, read-only snapshot, commands, display-field registry | T-024 (SPEC-004 and SPEC-005 code merged), T-026, T-027 |
| T-152 | Information views: confidence, order status, morale explanation | T-151 |
| T-153 | Overlays, march estimate, overnight flow | T-151 |
| T-154 | Accessibility core: bindings, flash governor, captions, palettes | T-151 |
| T-155 | Localisation: string tables, plurals, formatters, pseudo-localisation | T-151 |
| T-156 | Difficulty and the tutorial engine | T-152, T-153, T-155 |
| T-157 | Replay keyframes, photo sidecar, guide and card data | T-151, T-155 |
| T-158 | Usability protocol and report tool | T-156 |
| T-159 | Adapters: `InfoSource` over SPEC-007 and `MoraleSource` over SPEC-006 | T-152, T-064, T-076 |

## T-151: ui-model, snapshot, commands
- Spec: SPEC-015 sections 2, 3, 10 (pause and speed), 12 (`snapshot_hash`).
- Work: new crate `crates/ui-model` and workspace entry; `Snapshot`, `capture`, `soldier_frame`, the `InfoSource` and `MoraleSource` traits with the `Sighting`, `OrderRecord`, `MoraleReport` types and a `FullInfo` implementation; `UiCommand` to input record; `UiMode` and pause and speed accumulator; `encode_difficulty` types stubbed to the struct only; `data/ui/display-fields.toml`, `data/schemas/ui-display-fields.schema.json`, first version of `tools/validate_ui.py` (display-field check).
- Files: `crates/ui-model/**`, root `Cargo.toml`, `data/ui/display-fields.toml`, `data/schemas/ui-*.schema.json`, `tools/validate_ui.py`.
- Acceptance: **T15-01**, **T15-02**, **T15-03**; a `snapshot_hash` golden for a fixed battle recorded once and re-derived by an independent script.

## T-152: Information views
- Spec: SPEC-015 sections 4 (confidence), 5, 6.
- Work: `confidence_pm`, `confidence_band`, `derive_status`, `morale_view`, parameter loaders for `data/ui/{confidence,status,morale,symptoms}.toml` with provenance (placeholders carry CT-015-1 to CT-015-3), HQ-view enemy handling in `capture`.
- Files: `crates/ui-model/src/{confidence,status,morale}.rs`, `data/ui/*.toml`, schemas.
- Acceptance: **T15-04**, **T15-05**, **T15-06**.

## T-153: Overlays, march estimate, overnight flow
- Spec: SPEC-015 sections 4 (overlays), 8.
- Work: `overlay`, `OverlayParams` and `data/ui/overlays.toml`, `march_ticks`, `OvernightStep` state machine with undo and confirm, forecast input builder from the HQ view (the runner is a stub that takes a closure).
- Files: `crates/ui-model/src/{overlay,march,overnight}.rs`, `data/ui/overlays.toml`.
- Acceptance: **T15-07**, **T15-08**, **T15-17**.

## T-154: Accessibility core
- Spec: SPEC-015 section 11 (accessibility bullets).
- Work: `Action`, `Chord`, `Bindings` with conflict detection and TOML profile; `FlashGovernor`; motion scale for `reduce_motion`; caption timing and limits; `FocusGraph`; `data/ui/palettes.toml` with the channel table; palette checks in `tools/validate_ui.py` (deficiency simulation, delta E, contrast).
- Files: `crates/ui-model/src/{bindings,flash,captions,focus}.rs`, `data/ui/palettes.toml`, `tools/validate_ui.py`, `tools/test_validate_ui.py`.
- Acceptance: **T15-10**, **T15-11**, **T15-12**, **T15-13**.

## T-155: Localisation
- Spec: SPEC-015 section 11 (localisation).
- Work: `StringTable`, `Arg`, `plural_category`, locale formatters for clock, distance (yards or metric) and percent, `pseudo_localise` with modes, `tools/lint_strings.py` (no user-visible literals, no joined fragments), stub text-fit test, `data/text/en/*.toml` seed strings for the interface, missing-key CI check.
- Files: `crates/ui-model/src/text.rs`, `data/text/en/`, `tools/lint_strings.py`.
- Acceptance: **T15-14**.

## T-156: Difficulty and tutorial engine
- Spec: SPEC-015 sections 10 (difficulty), 11 (tutorial).
- Work: `Difficulty`, `encode_difficulty`, `decode_difficulty`, `data/ui/difficulty.toml` presets (placeholders, CT-015-5), whitelist scan; `TutorialState`, `tutorial_step`, `data/ui/tutorial.toml` for M1 to M8, scenario `drill_ground` data, skip path, hint cadence, scripted bot test.
- Files: `crates/ui-model/src/{difficulty,tutorial}.rs`, `data/ui/{difficulty,tutorial}.toml`, `data/scenarios/drill_ground.toml`.
- Acceptance: **T15-09**, **T15-15**. If the 400-line limit is exceeded, split difficulty into its own ticket before the tutorial.

## T-157: Replay keyframes, photo sidecar, guide data
- Spec: SPEC-015 sections 7 (camera, replay, photo), 9.
- Work: `CameraPose`, `CameraPath`, keyframe store and seek, divergence detection, `*.hdphoto` sidecar reader and writer, guide and card schemas (`data/schemas/ui-guide.schema.json`), validator rules (provenance, `review`, `reveal`, history length at most 60 words), a seed set of three guide entries marked `review = "pending"`.
- Files: `crates/ui-model/src/{camera,replay_view,photo}.rs`, `data/guide/**`, `data/schemas/ui-guide.schema.json`, `tools/validate_ui.py`.
- Acceptance: **T15-16**; the guide validator rejects a missing source, a `verified` row without a second source, and a release build containing `pending` entries.

## T-158: Usability protocol and report tool
- Spec: SPEC-015 sections 11 (usability plan), 14 (T15-18).
- Work: `qc/usability/criteria.toml` (U1 to U8), `qc/usability/protocol.md` (tasks, scripts, participant mix, telemetry event list), `tools/usability_report.py` with a sample CSV fixture, a test that U1 is at least 800.
- Files: `qc/usability/**`, `tools/usability_report.py`, `tools/test_usability_report.py`.
- Acceptance: **T15-18**; the first live round is run after T-156 and its report committed under `docs/cycle-reports/`.

## T-159: Information and morale adapters
- Spec: SPEC-015 sections 3 (contracts) and 16 (assumption 1); added by the registry because the contracts of SPEC-015 are satisfied by no existing ticket of SPEC-006 or SPEC-007.
- Work: in `crates/ui-model/src/adapters.rs`, `CommandInfo` implementing `InfoSource` over the SPEC-007 `picture_view`, `order_views` and `estimate` (sightings from contact views, `OrderRecord` from `OrderView`, never a true `RegimentId`), and `MoraleFeed` implementing `MoraleSource` over the SPEC-006 `morale_report`; the mapping of the six SPEC-006 indicators to the five SPEC-015 levels and of the SPEC-006 causes to factor ids is data in `data/ui/morale.toml`. The shape differences recorded as open decisions D-04 (morale report) and D-05 (order status) in CONFLICTS are resolved before this ticket starts; until then it implements the coarse mapping only.
- Files: `crates/ui-model/src/adapters.rs`, `data/ui/morale.toml`, tests.
- Acceptance: with `cmd` enabled, an enemy regiment moved in the true state without a new sighting leaves `snapshot_hash` unchanged (T15-04 half); every SPEC-006 indicator maps to exactly one level; every `OrderView` status maps to one SPEC-015 status.

