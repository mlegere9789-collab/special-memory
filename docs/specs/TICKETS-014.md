# TICKETS-014: terrain and battlefield pipeline, tickets T-141 to T-148

> Shared namespaces (input tags and actors, random streams, flags, hash blocks, tick order, CT ids, amendment labels, ticket ids) are owned by [REGISTRY.md](REGISTRY.md); conflicts and their resolutions are in [CONFLICTS.md](CONFLICTS.md).

Spec: SPEC-014. Ticket ids are T-141 to T-148 (REGISTRY section 8). One ticket, one session, under about 400 changed lines (code, tests, data together). Each ticket writes its tests first, ends with the Q0 commands from `CLAUDE.md`, and appends one row to `ledger/usage.csv`. A ticket that cannot fit splits at the data/code boundary and never drops or weakens a listed test. Dependency order; a ticket starts only when everything in `Needs` is merged.

| Ticket | Title | Needs |
|---|---|---|
| T-141 | Amendments, source manifest, coverage and licence record | T-025, T-027 |
| T-142 | `terrain` crate: tile codec, index, height and class queries | T-141 |
| T-143 | Obstacles and line of sight | T-142 |
| T-144 | Survey geometry checks and `hd-terrain` compile (heights, classes) | T-141, T-142 |
| T-145 | Obstacle compile, adjustments, report, polish check | T-143, T-144 |
| T-146 | Slab relief and land cover data, first golden | T-145 |
| T-147 | Slab linear features, buildings, names, vantage rows | T-146 |
| T-148 | Streaming proof, legacy export, performance report | T-147 |

## T-141: Amendments, source manifest, coverage and licence record
- Spec: SPEC-014 sections 3, 4.3, 14 (required amendments).
- Work: write SPEC-004 amendment A3 (section 5: classes 8 to 11); the SPEC-005 ground-height amendment (A16) waits for open decision D-02 in CONFLICTS and is not written here; add classes to `data/rules/terrain.toml` and `data/rules/obstacles.toml` with schemas and provenance (all `placeholder`, CT-014-3 and CT-014-4); `data/schemas/survey-sources.schema.json`; `data/survey/gettysburg/sources.toml` with the elevation source entries; a validator plugin `tools/validators/survey.py` for these files (T-027 mechanism); `tools/terrain/ingest_dem.py` skeleton that writes `dem_cm.i32` and `coverage.u8` with hashes. Human work recorded in the same ticket: the provider index query result, the saved licence text and its hash, `cleared_by`; if coverage or licence fails, stop and record the failure and the fallback source as a SPEC-014 amendment before any other ticket starts.
- Acceptance: **T14-15** (the `sources.toml` rules on small fixtures and on the real file); loader half of **T14-03** (classes 8 to 11 load, `blocks_movement` only for 7 and 11).
- Touches: `docs/specs/SPEC-004`, `data/rules/`, `data/schemas/`, `data/survey/`, `tools/`.

## T-142: Tile codec, index, height and class queries
- Spec: SPEC-014 sections 4.1, 4.2, 5 (queries), 11.
- Work: new `crates/terrain` (workspace member): `TileId`, `Tile::{decode, encode}`, `TerrainIndex`, `TerrainWorld::{new, insert_tile, evict_tile, is_resident, resident_ids, tiles_for_rect, height_mm, class_at, occ_mm, flags_at, slope_pm}`, `TerrainError`, `qc/fixtures/ref_height.py` and its committed output.
- Acceptance: **T14-01**, **T14-02**, the clamp part of **T14-03**, the synthetic part of **T14-10**.
- Touches: `crates/terrain`, `Cargo.toml`, `qc/fixtures/`.

## T-143: Obstacles and line of sight
- Spec: SPEC-014 sections 5 (line of sight), 11 (`ObstacleSeg`, `obstacles_in`, `los`).
- Work: obstacle section handling with de-duplication by `seg_id`, `obstacles_in`, `los` for both modes with the exact rational intersection, `qc/fixtures/ref_los.py` (independent implementation) and its committed output on a synthetic pack committed under `qc/fixtures/`.
- Acceptance: **T14-04**, **T14-05** (on the synthetic pack here; re-run on the slab in T-147), the obstacle half of **T14-13** (obstacle queries identical for any residency).
- Touches: `crates/terrain`, `qc/fixtures/`.

## T-144: Survey geometry checks and compile of heights and classes
- Spec: SPEC-014 sections 3 (geometry rules), 5 (compile stages 1, 3, 5), 10.
- Work: `shells/terrain-cli` (workspace member) with `check` and `compile` (heights, class and occ rasterisation, tile and index writing); survey schemas for `landcover.toml`, `linear.toml`, `buildings.toml`, `names.toml`; negative fixtures under `qc/fixtures/terrain/`.
- Acceptance: **T14-06**, **T14-07**, the compile-twice half of **T14-08** (same process and two processes, on a small synthetic survey).
- Touches: `shells/terrain-cli`, `data/schemas/`, `qc/fixtures/terrain/`.
- If it overruns 400 lines, move the polygon geometry checks to a follow-up inserted before T-145.

## T-145: Obstacle compile, adjustments, report, polish check
- Spec: SPEC-014 sections 5 (stages 2 and 4), 6, 7.
- Work: segment generation (`seg_id`, building walls), `adjustments.toml` schema and application with feathering, the play-budget check, `hd-terrain report` (`qc/terrain/adjustments.csv`, `ADJ_*` flags), `survey` tiles beside the pack, `polish-check` and the `zone.toml` schema.
- Acceptance: **T14-09** (on a synthetic survey with fixtures for each failure), **T14-14**.
- Touches: `shells/terrain-cli`, `data/schemas/`, `qc/fixtures/terrain/`.

## T-146: Slab relief and land cover data, first golden
- Spec: SPEC-014 sections 3, 9.
- Work: run the ingest on the cleared source for the slab; record the slab tile rectangle and origin in `index.toml` from the georeferenced anchors; digitise slab land cover (`landcover.toml`: Wheatfield, Rose Woods, Stony Hill, Triangular Field, Peach Orchard, the woods on both Round Tops, the rocky ground around Devil's Den, creek bottoms) with provenance; first `restore` adjustments with evidence; commit `data/terrain/gettysburg-slab/`; add the `terrain` section to `qc/golden-replays/expected.json` and `tools/verify_goldens.sh`.
- Acceptance: **T14-08** (three platforms), **T14-10** on the slab (steep groups explained), **T14-15** on the slab, the geometry half of **T14-06** on real data.
- Touches: `data/survey/`, `data/terrain/`, `qc/`, `tools/verify_goldens.sh`.
- If the digitising exceeds 400 lines, split by feature kind; never merge a partial pack that fails `check`.

## T-147: Slab linear features, buildings, names, vantage rows
- Spec: SPEC-014 sections 3, 12 (T14-11, T14-12).
- Work: `linear.toml` (roads, Plum Run and other creeks, walls, fences), `buildings.toml` for the slab farm buildings, `names.toml` for every slab anchor, `[[steep_ok]]` records, `qc/terrain/vantage.toml` rows from the research and named accounts with their tags, recompile and update the golden.
- Acceptance: **T14-11**, **T14-12** (report output committed to `qc/terrain/`), **T14-05** and **T14-09** on the slab, **T14-06** (`creek_uphill`, `road_dangling`) on real data.
- Touches: `data/survey/`, `data/terrain/`, `qc/terrain/`.

## T-148: Streaming proof, legacy export, performance report
- Spec: SPEC-014 sections 8, 10 (`export-rows`), 14.
- Work: `required_tiles` helper, the random-residency test harness, `hd-terrain export-rows`, the SPEC-004 grid compatibility test (dev-dependency of `battle-rules` on `terrain` or a test in `terrain-cli`), the ignored performance test and `docs/cycle-reports/CYCLE-terrain-slab.md` with measured numbers against H14.
- Acceptance: **T14-13**, the compatibility half of **T14-03**, ignored **T14-16**.
- Touches: `crates/terrain`, `shells/terrain-cli`, `docs/cycle-reports/`.
