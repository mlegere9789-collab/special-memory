# SPEC-011: historical replay and calibration (the core history QC)

> Shared namespaces (input tags and actors, random streams, flags, hash blocks, tick order, CT ids, amendment labels, ticket ids) are owned by [REGISTRY.md](REGISTRY.md); conflicts and their resolutions are in [CONFLICTS.md](CONFLICTS.md).

New crate `crates/history-cal` (named so it cannot be confused with the `battle_rules::calibration` module of SPEC-005, SPEC-006, SPEC-007 and SPEC-009; pure library, depends on `core-math`, `sim-state`, `battle-rules`; `#![forbid(unsafe_code)]`; no I/O, clock or threads). The shell `shells/sim-cli` gains `hd-sim cal` (file I/O and reporting only).
Source key: **CWV pNNN** = PDF page NNN of the research volume, with the page's tag. Invented numbers are `placeholder` with a target `CT-011-11xx` (section 9). Confidence mapping (p069: ledger figures win over narrative; p117: anything outside the ledger needs checking): ledger rows tagged V or P and entries tagged `[V]` are `partial` until a `second_source` exists; narrative-only, `[U]`, `[UNVERIFIED]` or untagged figures are `unverified`. Nothing here is `verified`.

## 1. Purpose and scope
Feed documented orders and arrival times into the deterministic core as scripted inputs and compare the outcome with the record: casualties by day, by corps, for named regiments, and fixed-trigger times. The same machinery tunes placeholder parameters within limits against overfitting. Out of scope: order of battle and map (scenario spec), morale, capture and command rules, F1/F2 internals (ADR 0002).

## 2. Definitions
**Trigger**: a scripted input at a fixed clock time. **Observation**: an emergent event with an evidenced window. **Band**: an evidenced range for a measured quantity; it gates iff confidence is `partial` or `verified`, `unverified` bands report only. **Fit set / hold-out set**: scenarios whose bands may, or may never, enter a tuning loss. **Clock**: integer seconds since 00:00 local on 1863-07-01 (the sources give clock times without a zone); tick = `(at_s - clock_start_s) * TICK_HZ`, where `clock_start_s` is the clock second of tick 0 (`[meta]`, default 0, equal to the SPEC-008 scenario value when one is bound; REGISTRY section 10). Row times in this spec are clock seconds; the SPEC-008 scenario rows use seconds from scenario start. Day N is `[(N-1)*86400, N*86400)`. Half-open windows throughout.

## 3. Data (integer-only TOML, ADR 0004 reader; provenance flattened as in rules rows)
`data/scenarios/<id>/script.toml` (`schema = "replay-script/1"`, schema `data/schemas/replay-script.schema.json`), `[meta]` `scenario`, `fidelity_class` 0..2, `clock_start_s` (optional), `end_s`, then rows, each with `id` and provenance:
- `[[arrival]]`: `unit`, `at_s`, `x_m`, `y_m`, `heading_deg` 0..359, `formation` (column, line, skirmish), optional `count` (default: the unit's strength).
- `[[order]]`: `unit` (any level), `at_s`, `kind` (hold, move, form, face, fire_at), `x_m`, `y_m`, `gait`, `formation`, `heading_deg`, `target_unit` as the kind needs. A non-regiment unit expands to its regiments in ascending id, each keeping its offset from the parent's anchor.
- `[[trigger]]`: `at_s`, optional `early_s`, `late_s` (a conflicting time, section 4.5), `hook` (name; default `marker`).
- `[[observe]]`: `kind` (`first_casualty` a on b, `reached` unit at `x_m, y_m, radius_m` by centroid, `loss_rate_at_least` unit `rate_pm`, `last_casualty_by` a on b), `lo_s`, `hi_s`, optional `after` (a trigger id; then the window is an offset).
`UnitRecord { id, level (army, corps, division, brigade, regiment, battery), parent, side, strength u16, training, formation }` comes from the scenario's `units.toml`, ids `^[a-z0-9_]+$`, indexed in ascending id.
`qc/calibration/<battle>-bands.toml`: `[[band]]` `id`, `scenario`, `unit`, `metric` (`count`, `rate_pm`, `rate_pm_median` over regiments), `basis` (`kw` killed+wounded, `total` kw+captured), `from_s`, `to_s`, `low`, `high` (absent: open above), `phrase`, `source_value`, `needs` (`capture`, `morale`, `melee`; default none), provenance. Gate is derived, never stored.
`qc/calibration/policy.toml`: every threshold of this spec as a placeholder row. `tunables.toml` (`id`, `file`, `row`, `key`, `ct`, `lo`, `hi`, `step`), `split.toml` (`scenario`, `set`), `accepted-ranges.toml`, `holdout-log.toml`, `baseline.toml` (`id`, `median`, `excess_pm`).
`qc/fixtures/history-conflicts.toml`: `[[conflict]]` `id`, `subject`, `rule`, `ref` (band or trigger id), `value_a`, `value_b`, sources.

## 4. Algorithms
**4.1 Compile.** `compile_script` emits `ScriptInput { tick, seq, payload }` sorted by `(tick, seq)`, `seq` ascending in file order within a tick (error above 65,535 per tick), actor `0xFD` (REGISTRY section 2). Orders use `encode_order` tags 1 to 5 (SPEC-004, SPEC-005). New payloads (tag space in REGISTRY section 2): tag `0x30` `Arrive { unit_index u16, x i64, y i64 (raw Fx metres), heading u32, formation u8, count u16 }`; tag `0x31` `Trigger { index u16 }`.
**4.2 Driver.** Per tick `t`: apply every input with `tick == t` in `seq` order (`Arrive` calls `spawn_regiment`, so regiment ids follow input order; `Trigger` appends `(t, index)` to `timeline_log` and calls the hook), then `BattleState::step`, then evaluate observations at tick `world.tick`. The driver draws no random numbers. `history_hash = FNV-1a 64(battle_hash, u32 log length, log entries)`; it is the replay checkpoint and final hash in replay-mode files, so `hd-sim verify` proves triggers too. Header `scenario_id` is `<scenario>/<variant>`.
**4.3 Metrics.** Unit sets are the unit's regiment descendants. `count` = casualty records (SPEC-005 section 6) with `unit` in the set, `tick` in the window, kind in the basis. Strength = sum of `count` of arrivals before `to_s`. `rate_pm = 1000 * count / strength` (floor).
**4.4 Evaluate** per seed list (sorted, median is the lower median, index `(n-1)/2`): `spread_pm = 1000 * (max - min) / max(median, 1)`; `excess_pm` = 0 inside, else `1000 * distance_to_band / (high - low)` (open band or zero width: divide by `max(low, 1)`). Verdicts, first match: `NotMeasurable` (a `needs` flag is absent from `Capabilities`; exception: `needs = capture` with basis `total` is measured as `kw` and gates on `<= high` only, marked `upper_only`); `Report` (not gating); `Unstable` (`n >= 4` and spread above `spread_limit_pm`); `Pass` (median inside); `Fail`. A gated band wider than `weak_width_pm` (`1000 * (high - low) / max(low, 1)`) is flagged `weak`.
**4.5 Timeline tests.** Scripted: the logged tick equals `(at_s - clock_start_s) * 20` exactly, always gating whatever the evidence confidence (it tests the harness; weak evidence is listed in the report). A trigger with `early_s`/`late_s` runs as three variants (nominal, early, late); every variant must pass every gated band and timeline window. Observed: the median over seeds lies in the window; gating follows confidence. A "~T" time widens to `T +- approx_time_s`; a stated range is used as given; ledger time beats narrative time and the other value is kept as `early_s` or `late_s`.
**4.6 Fitting.** Tunables are placeholder rows with a `CT` (not ones from section 7 data). Loss `= sum(w * min(excess_pm, 2000)^2)` over **gated fit-set bands only** (`partial` 1, `verified` 2, `unverified` 0), seeds 1 to 8. Coordinate descent, 3 rounds, tunables ascending, grid `lo..=hi` by `step` (at most 33 points); ties go to the value nearest the reviewed one, then lower. No randomness. Guards: free parameters at most `gated fit bands / 3`; SPEC-005 `run_band_suite` must still pass. **Ranges, not points:** the accepted range of a parameter is the contiguous run containing its minimum of grid values with loss `<= min * 11 / 10 + 100 * bands`; the row keeps the run's middle value, `note = "fitted, accepted range a..b, CAL-NN"`, confidence stays `unverified`; a run spanning the whole prior is `unidentified` and the reviewed value stays. **Hold-out** (`hd-sim cal holdout`, Q4 only, seeds 101 to 116, disjoint from fit seeds): accept iff the share of gated hold-out bands passing is at least `holdout_pass_pm`, hold-out loss is at most `2 * fit loss + 100 * bands`, and every gated timeline test passes. Each look is appended to `holdout-log.toml`; the fourth look in a cycle is refused. Bands are never edited to fit a result; a band changes only through an amendment citing new evidence (as SPEC-005 section 9).
**4.7 Reports.** `docs/cycle-reports/CAL-NN.md`, text fully determined by inputs: header (scenario, script and tables hashes, class, seeds, ticks, counters; no wall time), gate summary, band table (`id | conf | gate | low | high | min | med | max | excess_pm | verdict | trend`), timeline table, fixtures, parameters with accepted ranges, hold-out, drift. Integers only; permille print as `NNNpm`. Trend = `1000 * (median - baseline) / max(|baseline|, 1)`; `drift` above `trend_alert_pm`; a drift that turns a pass into a fail is a Fail. `report_hash` is FNV-1a 64 of the text.
**4.8 Determinism.** Integers only; ascending ids; sorted seeds; no hash maps; threads or processes only in the shell, merged in seed order; a run is a pure function of (script, tables, seed, class).

## 5. Public API
`ReplayScript::from_toml_str(&str) -> Result<ReplayScript, ScriptError>`; `compile_script(&ReplayScript, &[UnitRecord]) -> Result<Vec<ScriptInput>, ScriptError>`; `encode_arrive`, `decode_arrive`, `encode_trigger`, `decode_trigger`; `ReplayDriver::{new(BattleState, Vec<ScriptInput>, &[UnitRecord], &ReplayScript), step, run_to(u64), casualties, timeline_log, observations, history_hash}`; `trait TriggerHook { fn on_trigger(&mut self, &mut BattleState, u16) }`; `Capabilities { capture, morale, melee: bool }`; `BandSet::from_toml_str`; `measure(&Band, &ReplayDriver, &[UnitRecord]) -> i64`; `evaluate(&Band, &[i64], &Capabilities, &Policy) -> BandResult { min, median, max, spread_pm, excess_pm, verdict, weak }`; `run_suite(&SuiteSpec, &[u64]) -> SuiteReport`; `render_report(&SuiteReport) -> String`; `fit(&FitSpec) -> FitOutcome`; `holdout(&FitOutcome, &HoldoutSpec) -> HoldoutVerdict`. CLI: `hd-sim cal run --suite smoke|full --seeds LIST [--variant ...]`, `cal report`, `cal fit`, `cal holdout`; exit 0 clean, 1 gated failure, 2 malformed input.

## 6. Performance budget (hypothesis H11)
About `1.5 us` per soldier-tick (SPEC-004 H4 plus SPEC-005 H5 over 20,000 soldiers). Smoke suite (vignettes of at most 3,000 soldiers, 4 seeds, 4 cores): at most 4 minutes. Full nightly suite: at most 4 hours. Fits are offline (Q4). Evaluation and rendering: under 1 s. Vignettes above 20,000 soldiers (the Wheatfield, 20,444 engaged) run as F1, never F0.

## 7. Gettysburg band table (seed; values from CWV only)
Widening (CT-011-1101): from a stated figure `v`: exact count `[v*850/1000 floor, v*1150/1000 ceil]`; "about" `+-200 pm`; "nearly" `[-150 pm, 0]`; "over" `[0, +150 pm]`; "A to B" as given; "other estimates higher" leaves the top open. Basis `total` unless stated. Day N window is the day; `any` rows run in every vignette.

| id | scenario | target | low | high | conf | source |
|---|---|---|---:|---:|---|---|
| HB-01 | gb_day1 | Union, nearly 9,000 | 7650 | 9000 | partial | p069 V, p080 |
| HB-02 | gb_day1 | Confederate, slightly over 6,000 | 6000 | 6900 | partial | p069 V, p080 |
| HB-03 | gb_day2 | Union, about 10,000 (Trudeau), others higher | 8000 | open | partial | p069 V, p082 |
| HB-04 | gb_day2 | Confederate, about 6,800, others higher | 5440 | open | partial | p069 V, p082 |
| HB-05 | gb_v_pickett | Union, about 1,500 | 1200 | 1800 | partial | p069 P, p084 |
| HB-06 | gb_v_pickett | Confederate, 6,555 vs 8,892 (1,123+4,019+3,750) | 6555 | 8892 | unverified | p069 P, p084 (unsettled) |
| HB-07 | gb_full | Union, 23,049 (ABT) | 19591 | 26507 | unverified | p079, p084 |
| HB-08 | gb_full | Confederate, 28,063 (ABT) vs about 23,000 | 23000 | 28063 | unverified | p079, p084 |
| HB-09 | gb_full | Union kw, 3,155+14,529 | 15031 | 20337 | unverified | p079 |
| HB-10 | gb_full | Confederate kw, 3,903+18,735 | 19242 | 26034 | unverified | p079 |
| HB-11 | gb_v_east_cav | Union, 254 | 215 | 293 | partial | p069 V, p083 |
| HB-12 | gb_v_east_cav | Custer's brigade, 219 | 186 | 252 | partial | p069 V, p083 |
| HB-13 | gb_v_east_cav | Confederate, 181 | 153 | 209 | partial | p069 V, p083 |
| HB-14 | gb_v_wheatfield | eleven brigades, about 30 percent of 20,444, rate | 240pm | 360pm | partial | p069 V, p082 |
| HB-15 | gb_v_wheatfield | both sides, over 6,000 (some say 4,000 to 6,000) | 4000 | open | unverified | p082 |
| HB-16 | gb_v_wheatfield | regiment median rate, 30 to 50 percent | 300pm | 500pm | unverified | p085 |
| HB-17 | gb_v_minnesota | 1st Minnesota kw rate, 215 of 262 | 697pm | 943pm | partial | p286 V, p082 |
| HB-18 | gb_v_iron_brigade | Iron Brigade rate, 1,153 of 1,885 | 520pm | 704pm | unverified | p286 (no tag shown) |
| HB-19 | gb_v_iron_brigade | 24th Michigan rate, 397 of 496 | 680pm | 920pm | unverified | p286 |
| HB-20 | gb_v_iron_brigade | 2nd Wisconsin rate, 77 percent | 654pm | 886pm | unverified | p286 |
| HB-21 | gb_day1 | I Corps rate, "more than half" | 500pm | open | unverified | p080 [UNVERIFIED] |
| HB-22 | gb_v_south_cav | 1st West Virginia rate, 98 of about 400 | 208pm | 282pm | unverified | p083 |
| HB-23 | any | loss rate at which units halt, 25 to 30 percent (`needs morale`) | 250pm | 300pm | unverified | p116 |
| HB-24 | any | loss rate "destroyed for the battle" (`needs morale`) | 500pm | open | unverified | p116 |

Times (seconds on the clock; scripted rows are inputs, observed rows are tests; "~T" windows `+- 900 s`, CT-011-1101):

| id | kind | nominal / window | source, conf |
|---|---|---|---|
| TL-01 | arrival, I Corps lead | 37200 | p080, unverified |
| TL-02 | arrival, Hancock | 56700 (55800 to 57600) | p080, unverified |
| TL-03 | order, Lee lifts the restriction | 52200 | p080, unverified |
| TL-04 | arrival, Meade | 86400 | p081, unverified |
| TL-05 | order, III Corps advances | 140400 | p081, unverified |
| TL-06 | order, Hood steps off | 145800, early 144000 | p069 V vs p081, partial |
| TL-07 | order, Ewell's evening attacks | 154800, early 153000 | p069 V vs p082, partial |
| TL-08 | order, Johnson's second attack | 189000 | p083, unverified |
| TL-09 | trigger, cannonade begins | 219600 | p069 V, p083, partial |
| TL-10 | order, charge steps off | 223200, late 226800 | p069 P vs older accounts, partial |
| TL-11 | observed, first Buford/Heth casualty | 26100 to 28800 | p069 V, p080, partial |
| TL-12 | observed, Davis repulsed at the cut | 38700 to 40500 | p069 V, p080, partial |
| TL-13 | observed, Union centroid on Cemetery Hill | 58500 to 60300 | p069 V, p080, partial |
| TL-14 | observed, Confederates off Culp's Hill | by 213300 | p083, unverified |
| TL-15 | observed, last casualty by the charging units | 1200 to 1800 after TL-10 | p084, unverified |

Conflict fixtures (`history-conflicts.toml`; rules `envelope`: band covers both values; `time_window`: window covers both; `no_subtract`: no band derived by subtracting stated components of different bases; `bad_value`: a value in `qc/fixtures/audit-known-errors.toml` appears only in `unverified` rows): CF-01 28,063 vs 23,000 (HB-08); CF-02 6,555 vs 8,892 (HB-06); CF-03 charge at 14:00 vs 15:00 (TL-10; the cannonade and the 2:45 to 3 p.m. message on p083 also conflict); CF-04 Hood 16:00 vs 16:30 (TL-06); CF-05 Ewell 18:30 vs 19:00 (TL-07); CF-06 day-1 Union "nearly 9,000" vs about 4,000 prisoners, composition not stated (HB-01, `no_subtract`); CF-07 Wheatfield 4,000 to 6,000 vs 6,133 (HB-14, HB-15); CF-08 Stuart's strength 3,430 to 5,000 (p083), run both as scenario inputs; CF-09 Reynolds killed between 10:20 and 11:30 (p080), a window, never a point; CF-10 the ABT headline values 93,921, 71,699, 23,049, 28,063 only in `unverified` rows.

## 8. Acceptance tests (prefix T11; tests first, thresholds not edited to pass)
- **T11-01 script:** a float, unknown key, row without provenance, placeholder without `CT`, `verified` without `second_source` each give the named error; `(at_s - clock_start_s) * 20` ticks exact; inputs sorted `(tick, seq)`; 65,536 inputs in one tick rejected, 65,535 accepted; parent orders expand in ascending regiment id.
- **T11-02 codec:** tags `0x30` and `0x31` round-trip (proptest); truncation and trailing bytes rejected; tags 1 to 5 unchanged.
- **T11-03 driver:** toy script (arrivals at ticks 0 and 100, move at 200): regiment 2 exists exactly after the step at tick 100; permuting `[[order]]` rows with different times leaves `history_hash` unchanged; same-tick rows keep file order; `hd-sim verify` passes the written replay; a trigger removed from the log changes the hash.
- **T11-04 timeline:** scripted triggers log exactly `(at_s - clock_start_s) * 20`; `reached` for `march_single` fires within 1 percent of the SPEC-004 T4-03 arrival tick; variants run three times and all must pass.
- **T11-05 evaluate:** band 100..200: medians 150, 50, 250 give Pass/0, Fail/500, Fail/500; open band low 100, median 50 gives Fail/500; samples `[100,100,200]` with limit 400 give Unstable (spread 1000); even count takes the lower median; `unverified` gives Report; absent `capture` makes a `total` band `upper_only`; absent `morale` gives `NotMeasurable`.
- **T11-06 band tables:** every row loads with provenance; `low` and `high` recompute from `phrase` and `source_value` by section 7; ABT components sum to their totals; no `verified`; ids match the section 7 list (test holds the literal list).
- **T11-07 fixtures:** every CF rule holds; in-memory mutations (HB-08 narrowed to 28,063..28,063, TL-06 window narrowed to a point) make the fixture test fail.
- **T11-08 report:** a fixed in-memory `SuiteReport` renders to `qc/calibration/golden/report-sample.md` (recorded once); two processes agree.
- **T11-09 fit:** synthetic quadratic loss in two parameters: minimum found, accepted run equals the brute-force set; unverified bands have weight 0; too many free parameters refused; a call counter shows zero hold-out evaluations during `fit`.
- **T11-10 hold-out:** a synthetic fit that lowers fit loss and breaks the hold-out limits is rejected; looks are logged; the fourth look is refused.
- **T11-11 determinism:** `gb_v_minnesota` replay (seed 1) and the smoke `report_hash` are literals recorded once on Linux in `qc/golden-replays/expected.json`; `hd-sim verify` and `cal run --suite smoke --seeds 1,2` agree on Linux, Windows and macOS.
- **T11-12 suite:** `cal run --suite smoke` exits 0 on a clean tree, 1 when any gated band fails (synthetic broken table), 2 on a malformed script.

## 9. Calibration targets (all in `policy.toml`, placeholders)
CT-011-1101 band conventions: `widen_exact_pm 150`, `widen_approx_pm 200`, `approx_time_s 900`, `weak_width_pm 1500` (set by source review, never by simulation output). CT-011-1102 `spread_limit_pm 400`; seed lists 1 to 4 (Q1), 1 to 16 (nightly), 1 to 8 (fit), 101 to 116 (hold-out). CT-011-1103 `trend_alert_pm 150`. CT-011-1104 `holdout_pass_pm 700`, loss factor 2, 3 looks per cycle. CT-011-1105 tunable ranges, steps, 3 rounds, loss tolerance. Fit set (frozen in `split.toml` before the first fit): gb_v_minnesota, gb_v_wheatfield, gb_v_iron_brigade, gb_day1. Hold-out: gb_v_east_cav, gb_v_pickett, gb_day3, and other `data/history` battles once they have second-source bands (CT-005-9).

## 10. Assumptions
A1 The scenario spec (SPEC-008) provides the unit list and the map; `units.toml` is the interim form of its `[[unit]]` rows (open decision D-06 in CONFLICTS). A2 The casualty log (SPEC-005 section 6) is in ascending target order; morale and capture specs later add `CasualtyKind::Captured` and set `Capabilities`; until then `capture` is false, so most `total` bands gate only on their upper edge. A3 Day-level scenarios run F1 or F2 and expose the same log (aggregate casualties need records: open decision D-07 in CONFLICTS). A4 Payload tags `0x30`, `0x31` and actor `0xFD` are assigned by REGISTRY section 2. A5 One V-tagged secondary source does not make a figure `verified`.

## 11. Risks
Most bands are unverified or weak, so gates are loose until a second-source pass (the report counts `weak` bands). Overfitting to few vignettes: guarded by the free-parameter limit, ranges, hold-out seeds and scenarios, and the look limit. Casualty definitions differ (missing against `kw`). Narrative times set scripted inputs. Smoke cost may exceed H11. Tags and actors are owned by REGISTRY section 2, so no other spec can take `0x30` or `0x31`.

## 12. Done when
fmt, clippy `-D warnings`, `cargo test --workspace` (also `--release`), T11-01 to T11-12, the scans and `validate_data.py` (extended to scripts, bands, fixtures) pass; section 7 data committed; goldens verified on three platforms; first `docs/cycle-reports/CAL-NN.md` committed; nightly and Q1 CI steps wired; no floating point outside `#[cfg(test)]`.

## Amendment A1 (from T-111, ADR 0010 decision D-06)
One owner per historical row. This amendment replaces the text of sections 1 to 5 and 10 wherever it conflicts.
- **Ownership.** Arrivals, departures and weather live in the SPEC-008 scenario (`[[arrival]]`, `[[departure]]`, `[[weather]]`, each with its evidence window). `script.toml` keeps `[[order]]`, `[[trigger]]`, `[[observe]]` and the new `[[override]]`. The script has no `[[arrival]]` rows and the compiler never emits tag `0x30`; the tag stays allocated to SPEC-011 in REGISTRY section 2.2 and the `Arrive` codec is dropped from section 5. The driver wraps the SPEC-008 `Battle`, so class hand-overs stay visible to it; `ReplayDriver::new` takes the `Battle` in place of the `BattleState` (T-112).
- **`[[override]]` rows** (`id`, `target_kind` in `arrival`, `departure`, `trigger`, `target` a unit id or a scenario trigger id, `variant` in `early`, `late`, `at_s`, provenance). The `early` and `late` variants of section 4.5 replace `at_s` of the named scenario row; the nominal variant leaves it. The script loader checks the row's own fields; resolving `target` against the scenario belongs to the driver.
- **Trigger rows** keep `at_s`, optional `early_s` and `late_s`, and `hook`. Their `Trigger { index }` payload (tag `0x31`) indexes the trigger rows in ascending id order, so file order never changes a log.
- **Units.** `UnitRecord` gains `x_m`, `y_m` (whole metres, the unit's anchor in the scenario at its arrival; a non-regiment unit's anchor is the floor mean of its regiments' anchors). `units.toml` remains the interim list until the scenario hands the same fields over. Levels `regiment` and `battery` are regiments; the other levels are headquarters. `RegimentId` is the rank by ascending unit id among the regiments, the SPEC-008 rule, so file order never matters.
- **Compile.** An `[[order]]` to a regiment emits one input; to any other unit it emits one input per descendant regiment in ascending regiment id, a `move` target being the order's point plus the regiment's anchor minus the parent's anchor (`face`, `form`, `hold`, `fire_at` carry over unchanged). Within one tick the inputs of orders come in file order, then the triggers in ascending id; `seq` counts from 0 and a tick holds at most 65,535 inputs (`ScriptError::TooManyInputs`). A `fire_at` input is `u8 5, u16 regiment, u16 target regiment` until SPEC-005 publishes its own encoder for tag 5.
- **Errors.** `ScriptError` is `Data(DataError)` (float, unknown key, provenance, range, parse), `Duplicate`, `Reference`, `Time`, `TooManyInputs`.
- **Policy.** `qc/calibration/policy.toml` holds `[[threshold]]` rows (`id`, `value`, provenance) for every threshold of section 9, each a placeholder naming its own `CT-011-11nn`; `Policy::from_toml_str` refuses a missing, unknown or duplicate id and a row whose target differs from the one section 9 assigns.

## Amendment A2 (from T-113 implementation, 2026-11-04)
Section 4.4 and acceptance test T11-05 disagreed: the text gave `Unstable` the condition `n >= 4`, while T11-05 requires the samples `[100, 100, 200]` (n = 3, spread 1000 against limit 400) to give `Unstable`. The acceptance test states the intended behaviour, so section 4.4 reads `Unstable` (`n >= 3` and spread above `spread_limit_pm`). With two samples the median and the spread are not separable, so n below 3 is never `Unstable`.
