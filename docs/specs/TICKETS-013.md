# TICKETS-013: Gettysburg data and audit pipeline, tickets T-130 to T-136

Spec: SPEC-013 (data model and order-of-battle audit). Ticket numbers are a block (130 to 136) so they cannot collide with other lists. One ticket, one session, under about 400 changed lines (code, tests, data together). Each ticket writes its tests first, ends with the Q0 commands from `CLAUDE.md`, and appends one row to `ledger/usage.csv`. A ticket that cannot fit splits at the data/code boundary; it never drops or weakens a listed test. A ticket starts only when everything in `Needs` is merged. The six `data/schemas/gb-*.schema.json` files are inputs to T-130, not outputs.

| Ticket | Title | Needs |
|---|---|---|
| T-130 | Source registry, time functions, ADR 0005 | none |
| T-131 | Corpus loader, structural rules, validator extension | T-130 |
| T-132 | Evidence ladder and known-conflict fixtures | T-131 |
| T-133 | Chain sums, casualties, arms, terrain cross-checks | T-131 |
| T-134 | Commanders, time ordering, hourly schedule | T-131 |
| T-135 | Audit reports, reconcile, training class, determinism | T-132, T-133, T-134 |
| T-136 | Pilot records from the research volume | T-135 |

## T-130: Source registry, time functions, ADR 0005
- Spec: SPEC-013 sections 2, 4, 7 (time functions), 14.
- Work: `data/history/gettysburg/sources.toml` with CWV consulted and the candidates `OR`, `NPS`, `BACH`, `WARREN`, `LOSS`, `STUDY`, `LOC`, `BMI` all `consulted = false` with no edition or licence text; `tools/gb_audit.py` with `t_min`, `stamp_of`, `hour_of`, registry reading and the GB04 and GB05 checks; ADR `docs/adr/0005-gettysburg-history-data.md` (integer-only history records, claims and lineage, local clock stamps, the 13xx calibration range).
- Acceptance: **T13-02**; the schema half of **T13-01** (metaschema check of the six schemas and `source_registry`); GB04 and GB05 cases of **T13-06** (unconsulted source, uncleared `permission_required` source).
- Files: `tools/gb_audit.py`, `tools/test_gb_audit.py`, `data/history/gettysburg/sources.toml`, `docs/adr/0005-*.md`, `ledger/usage.csv`.

## T-131: Corpus loader, structural rules, validator extension
- Spec: SPEC-013 sections 3, 6 (GB01 to GB03, GB07 to GB09), 10 (validator extension).
- Work: `load_corpus` (sorted walk, ids, file stems), `check_corpus` for the listed rules, `qc/fixtures/gb/good/` and `bad/` probe records (one per rule, first line `# expect: GBnn`), `check_gettysburg` in `tools/validate_data.py` called from `main()`, throwaway-root tests in `tools/test_validate_data.py`.
- Acceptance: **T13-01** (good and bad probes), **T13-12**; existing `tools/test_validate_data.py` tests unchanged and green.
- Files: `tools/gb_audit.py`, `tools/validate_data.py`, `tools/test_validate_data.py`, `qc/fixtures/gb/`.

## T-132: Evidence ladder and known-conflict fixtures
- Spec: SPEC-013 sections 5, 6 (GB04 to GB06, GB17), 9.
- Work: lineage lookup, claim agreement, `second_source` matching, demotion on disagreement; `qc/fixtures/gb-known-conflicts.toml` with C01 to C14 exactly as in section 9; `check_fixtures`; R6 call in `validate_data.py`.
- Acceptance: **T13-06** in full, **T13-07**.
- Files: `tools/gb_audit.py`, `tools/validate_data.py`, `qc/fixtures/gb-known-conflicts.toml`, `qc/fixtures/gb/bad/`.

## T-133: Chain sums, casualties, arms, terrain cross-checks
- Spec: SPEC-013 sections 6 (GB10 to GB12, GB18), 7 (interval sum, `pct`).
- Work: interval and exact chain sums over `engaged`, `present` and battery `guns`; casualty component, scope and ceiling checks; `pct`; arms and weapon-id checks against `data/rules/weapons.toml`; terrain class, polygon and bounds checks against `data/rules/terrain.toml`.
- Acceptance: **T13-03**, **T13-04**, **T13-11**.
- Files: `tools/gb_audit.py`, `tools/test_gb_audit.py`, `qc/fixtures/gb/`.

## T-134: Commanders, time ordering, hourly schedule
- Spec: SPEC-013 sections 6 (GB13 to GB16), 7 (hourly schedule).
- Work: stint overlap with shared endpoints, casualty-event ordering, `not_before`, arrival and departure ordering, assignment overlap, brigade state homogeneity warning, weather coverage, `hourly_schedule` and the `hourly` CLI command.
- Acceptance: **T13-05** (including the C13 boundary stamp), the schedule half of **T13-10**.
- Files: `tools/gb_audit.py`, `tools/test_gb_audit.py`, `qc/fixtures/gb/`.

## T-135: Audit reports, reconcile, training class, determinism
- Spec: SPEC-013 sections 5 (promotion), 7 (`training_class`, review sample), 8, 10 (`reconcile`, `write_report`).
- Work: report reading and GB19, `reconcile` over two corpora, `write_report` with sorted output, `training_class`, review sample size function, `report` CLI command; a test that the module contains no wall-clock call.
- Acceptance: **T13-08**, **T13-09**, the `training_class` half of **T13-10**.
- Files: `tools/gb_audit.py`, `tools/test_gb_audit.py`, `qc/audit/gettysburg/` (one valid sample report under `qc/fixtures/gb/good/`).

## T-136: Pilot records from the research volume
- Spec: SPEC-013 sections 3, 9, 16.
- Work: records built only from the CWV pages named in section 9, all at `partial` or `unverified`: the army and corps skeleton from CWV p072, I Corps and the Iron Brigade chain and the 24th Michigan from CWV p080 and p286, the timeline events for C04 to C06, and weather for 1863-07-01 to 1863-07-04 from CWV p079, p084 and p116. No unit sourced to an unconsulted document; no `verified` fact. Then the first packet report with the Reviewer's block, used to set CT-1303.
- Acceptance: `python3 tools/validate_data.py` exits 0 with the records present; C01 to C14 fixtures pass against them; the pilot report validates (GB19 clean).
- Files: `data/history/gettysburg/` records, `qc/audit/gettysburg/AP-pilot.*.report.toml`, `ledger/usage.csv`.
