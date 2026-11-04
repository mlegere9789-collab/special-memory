# SPEC-013: Gettysburg data model and order-of-battle audit

Data model, checker and audit workflow for the Gettysburg order of battle to regiment and battery level. Python tooling and TOML data only; no change to `crates/`. **CWV pNNN** = the research volume, PDF page NNN, with the page's own tag (`untagged` when none); every figure is a hypothesis at that confidence. Invented numbers are placeholders with a calibration target (CT-1301 to CT-1305, section 13; the 13xx range avoids the CT-1 to CT-9 reuse between SPEC-004 and SPEC-005).

## 1. Purpose and scope
In scope: formations, strengths with ranges, commanders over time, weapons mix, arrivals and departures, terrain objects, timeline events, weather, the source plan, the two-source rule, consistency rules, the audit report, regression fixtures and the agent split. Out of scope: elevation, calibration bands, the scenario loader that consumes these records, morale. No source except CWV has been consulted; this spec asserts nothing about any other source.

## 2. Definitions
- **Claim**: one source's figure or time for one fact, with page. **Lineage**: sources that copy one another share one; two sources confirm only if lineages differ. **Consulted**: the registry row has edition, access, licence text and date.
- **Stamp**: `1863-MM-DDThh:mm`, local clock as reported, no zone. `t_min` = minutes since `1863-07-01T00:00`. **Window**: `[-4320, 20160)`, 1863-06-28 to 1863-07-14 inclusive (CWV p079 Meade's appointment, p084 the retreat). **Battle days**: `[0, 4320)`. **Hour bucket**: `floor(t_min / 60)`.
- **Chain**: army > corps > division > brigade > regiment; a battery attaches to an artillery group (a brigade-level unit record), a division, a corps or the army. **Packet** (`AP-...`): one brigade or artillery group with its regiments or batteries.
- **Engaged**: carried into action in the battle; **present**: reported present on arrival. They are different facts and are never merged (CWV p083: 5,000 present against 3,430 engaged for Stuart).

## 3. Data layout and schemas
Integer-only TOML (a float is GB02), one record per file, stem equals `id`, under `data/history/gettysburg/`: `sources.toml`, `units/` (army to regiment), `batteries/`, `arrivals/`, `terrain/`, `events/`, `weather/`, each validated by the `gb-*` schema of that kind; `gb-regiment.schema.json` also holds the shared `$defs`. Unknown values are omitted, never guessed. Ids: units `us_`/`cs_` plus slug, `ar_`, `ev_`, `wx_YYYYMMDD`.

**Shared facts.** `int_fact {value, low?, high?, confidence, sources, claims[], second_source?, flag?}` with `claim {document, page, figure, figure_high?, tag?}`. `time_fact {at, early?, late?, ...same}` with `claim {document, page, at, at_late?, tag?}`. `second_source` is `CODE:page`. `verified` requires `second_source` and at least two claims (schema).

| Record | Required fields | Notes |
|---|---|---|
| unit | `id, side, level, name, commanders[]`; `assignments[]` unless army; regiments add `designation {state, ordinal, arm}` and `strength.engaged` | `strength {present?, engaged?, headquarters?}`; `arms[] {weapon, count}` (weapon is an id of `data/rules/weapons.toml`); `casualties[] {scope: battle/day1/day2/day3, killed?, wounded?, missing_captured?, total?}`; `experience {raised, prior_battles}` |
| assignment | `parent, kind (organic/attached), confidence, sources`, optional `from`, `to` | the first organic assignment is the parent of record for sums |
| stint | `person {id, name}, kind (command/acting/temporary_higher), confidence, sources`, optional `rank, from, to` | `person.id` equals a `data/history/commanders` id when that file exists |
| battery | as unit plus `designation {state, label}`, `strength.engaged`, `guns[] {weapon, count}` (1 to 8 rows), `rounds_per_gun?` | |
| arrival | `id, formation, event (arrive/deploy/withdraw/depart), at (time_fact), place {name, confidence, sources}` | `place` may carry `x_mm, y_mm` in frame `gb_local_v1`, `road` |
| terrain | `id, name, kind, geometry {type, frame, points[[x_mm,y_mm]]}, position_tolerance_mm, confidence, sources` | walls, fences, earthworks, embankments need `height_mm`; roads, lanes, creeks need `width_mm`; `terrain_class` names a row of `terrain.toml`; works raised in the campaign carry `exists_from` |
| event | `id, kind, role (trigger/validation), at, summary (at most 300 characters, original wording)` | `actors[]`, `place`, `not_before[] {event, gap_min}`; `commander_casualty` requires `person_outcome {person, outcome, remained_in_command?}` |
| weather | `id, date, segment[] {from, to, sky, confidence, sources}` | optional `ground`, `temp_f`, `humidity_pm`, `wind_from_deg`, `wind_mm_s`, `visibility_m`, `rain_dmm_per_h` as `int_fact` |

SPEC-004 `Training` is derived (section 7), not stored.

## 4. Source plan
`sources.toml` (schema `$defs/source_registry`) holds one row per source: `code, title, kind, lineage, licence_class (public_domain, open_licence, permission_required, facts_only, internal, unknown), consulted`, and when consulted `edition, access, licence_text, consulted_on, page_scheme`. It ships with CWV consulted (`internal_research`, lineage `web_compilation`: the volume says it was compiled mainly from web encyclopedias and a battlefield-trust site) and these candidates, all `consulted = false`, no edition or licence recorded: `OR` Official Records, `NPS` park-service historic resource and order-of-battle materials, `BACH` Bachelder papers and maps, `WARREN` the Warren survey map, `LOSS` specialist strength and loss compilations, `STUDY` multi-volume studies of the campaign and the three days, `LOC` Library of Congress photographs, `BMI` Union Bureau of Military Information order-of-battle reports (named at CWV p258; existence and form unconfirmed). The Librarian confirms each source exists in the assumed form, records edition and licence, and sets `consulted`. `permission_required` or `unknown` sources need `cleared_by` (human legal sign-off) before any record cites them; park-service material needs written permission (plan section 9).

## 5. Evidence ladder and promotion
- `unverified`: one claim, or any claim from a source that is not consulted (GB04), or an unresolved conflict.
- `partial`: at least one consulted source with a page; or two disagreeing claims stored as a range (`low`, `high` span every claim).
- `verified`: two or more claims from different lineages, sources consulted and licence-cleared, figures equal (times within `TIME_TOL_MIN`), `value` equal to them, `second_source` naming the second, and the Reviewer's sign-off. A new disagreeing claim demotes the fact to `partial`.
- CWV tags (`[V]`, `[U]`) are copied into `tag` and never lift a fact above `partial`; CWV and an encyclopedia it drew on are one lineage.

Promotion: (1) first pass collects claims at `partial` or lower; (2) a blind second pass, given only unit ids and the source list, collects claims again; (3) `reconcile` proposes a decision per fact; (4) the Reviewer re-derives every promotion and a sample of the rest; (5) the historian signs off.

## 6. Consistency rules
Severity E fails `validate_data.py`; W is printed and must be listed in the report as fixed, waived with a reason, or disputed.

| Rule | Check |
|---|---|
| GB01 | schema violation |
| GB02 | a float anywhere; **GB03** id differs from file stem, or is duplicated within its kind |
| GB04 | a cited document missing from the registry, or `consulted = false`; **GB05** a cited `permission_required` or `unknown` source without `cleared_by` |
| GB06 | `verified` without two consulted claims of different lineages that agree, or `second_source` not equal to a claim's `CODE:page` |
| GB07 | `low <= value <= high`; with claims, `low <= min` and `high >= max` of the claim bounds |
| GB08 | a stamp invalid or outside the window; a battle-day event outside `[0, 4320)`; stint, assignment or segment with `from > to` |
| GB09 | parent absent, not adjacent in the chain, other side, or cyclic; a `from`/`to` overlap between organic assignments |
| GB10 | strength chain sum (section 7) for `engaged`, `present` and battery `guns`, on parents of record; skipped with an I finding when any child lacks the figure |
| GB11 | casualty components against total; day scopes against battle scope; losses not above `engaged.high`; recorded percentages (section 7) |
| GB12 | sum of `arms` counts above `engaged.high`; weapon id not in `weapons.toml`; battery `guns` against `strength` |
| GB13 | one person with overlapping `command` or `acting` stints (shared endpoint allowed); a formation with overlapping `command` stints; a stint after the person is killed, mortally wounded or captured (a wounded person flagged `remained_in_command` gives W); a stint of another side; `temporary_higher` overlapping a command stint (W, needs `note`) |
| GB14 | arrival after its own departure; `at` before a `not_before` event's `at` plus `gap_min`; a child arriving more than 60 minutes before its parent (W) |
| GB15 | duplicate `(side, state, ordinal, arm)`; a Confederate brigade holding regiments of more than one state (W, CWV p300) |
| GB16 | weather segments of a day overlapping, unordered, or outside the date's day plus the following night |
| GB17 | a known-conflict fixture violated (section 9) |
| GB18 | `terrain_class` absent from `terrain.toml`; polygon with fewer than 3 points; coordinates outside the frame bounds |
| GB19 | audit report integrity: a blind pass with `saw_prior_values`, a reviewer session equal to a pass it reviews, or a promotion with no reviewer report |

## 7. Algorithms (exact integer terms)
- `t_min(stamp) = (days_from_civil(y, m, d) - days_from_civil(1863, 7, 1)) * 1440 + hh * 60 + mm`, using the standard proleptic Gregorian day count; an impossible date (June 31) is GB08. `hour_of(t) = floor_div(t, 60)`.
- Bounds of a fact: `lo = low or value`, `hi = high or value`. Chain sum for parent P with children `C_i` and optional `headquarters` H: pass iff `sum(lo_i) + lo_H <= hi_P` and `sum(hi_i) + hi_H >= lo_P`. When P and every child are `verified` the test is exact: `sum(value_i) + value_H == value_P`.
- Percentage: `pct(loss, base) = floor((loss * 100 + base / 2) / base)` (integer division); a recorded rate must equal it within 1.
- Verified agreement for times: `max(at) - min(at) <= TIME_TOL_MIN` over all claims (`at_late` counts), `TIME_TOL_MIN = 15` (placeholder, CT-1301).
- Temperature for consumers: `temp_dc = floor_div((temp_f - 32) * 100 + 9, 18)` (tenths of a degree Celsius).
- `training_class(raised, prior_battles, as_of)`: `months = days_between(raised, as_of) / 30` (floor); `Veteran` if `prior_battles >= 3 and months >= 12`; `Trained` if `prior_battles >= 1 or months >= 6`; else `Raw` (placeholder thresholds, CT-1302).
- Hourly schedule: arrivals, deploys, withdrawals and departures sorted by `(hour_of(at), formation id, event rank arrive < deploy < withdraw < depart, id)`.
- Review sample: `n = min(count, max(10, (count * 10 + 99) / 100))` records per packet (CT-1303); every proposed promotion is re-derived.

## 8. Audit workflow, split and review
Roles, each a separate session, never two on one packet: **Librarian** (registry); **Auditor** (first pass, one packet, under about 400 changed lines); **Blind auditor** (second pass, sees no values); **Reconciler** (runs `reconcile`, records decisions); **Reviewer** (re-derives from the source page, writes the review block); **Historian** (human sign-off, plan section 9). Order: registry; tree skeleton (army to brigade) from the first consulted official source; packets by corps; arrivals, events, terrain and weather in parallel. The orchestrator opens one ticket per packet from `docs/ticket-template.md` after T-135; packet counts are known only after the skeleton exists.

Report file: `qc/audit/gettysburg/<packet>.<pass>.report.toml`, schema `$defs/audit_report`: `packet, scope[], pass, agent {role, session}, date` (passed on the command line, never read from a clock), `saw_prior_values`, `sources_used[]`, `counts`, `findings[] {rule, severity, record, path, message, disposition, waiver_reason}`, `reconciliation[] {record, path, claims[], decision, value, low, high}`, `headline[] {record, path, headline, audited_low, audited_high, note}` (the audited sum against the headline figure; the headline stays `unverified`), `review`, `historian`. Findings sort by `(record, path, rule)`.

## 9. Known conflicts as regression fixtures
File `qc/fixtures/gb-known-conflicts.toml`, tables `[[conflict]] {id, kind, record, path, claims[], rule}`. Rule GB17: once the named record exists, it must not be `verified` while its claims disagree, its range must span the claims, and a sum fixture must be reported not hidden. Rows (all `untagged`):

| Id | Conflict in the volume | Required state |
|---|---|---|
| C01 | Engaged 93,921 Union and 71,699 Confederate cited to ABT, older tallies differ (p079) | army `engaged` never `verified`; report holds the audited sum beside the headline |
| C02 | Losses Union 23,049 (3,155 + 14,529 + 5,365), Confederate 28,063 (3,903 + 18,735 + 5,425), older Confederate about 23,000 (p079; p084 gives 23,000 to 28,000) | components sum (control); Confederate range 23,000 to 28,063 |
| C03 | Pickett's Charge: Confederate 6,555 against 1,123 + 4,019 + 3,750 = 8,892 (p084) | GB11 fires unless both stored as claims with `flag` |
| C04 | Hood's step-off about 16:00 and about 16:30, same page (p081) | `early` 16:00, `late` 16:30, not `verified` |
| C05 | Pickett's step-off "roughly 2-3 p.m." (p083); bombardment from about 13:00, two hours; Alexander's message 14:45 to 15:00 (p083) | `not_before` the message; both claims kept |
| C06 | Reynolds killed after I Corps arrived about 10:20 and before Howard heard about 11:30 (p080); Hancock on Cemetery Hill about 15:30 to 16:00 (p080) | `early` 10:20, `late` 11:30; Hancock range kept |
| C07 | Wheatfield: "six" Union divisions or elements, five listed; eleven brigades; 20,444 engaged; losses "over 6,000" against "4,000-6,000" (p082) | count flagged; loss range 4,000 to 6,133 |
| C08 | Pickett's Charge "9 brigades" listed as 3 + 4 + 2, plus two reserve brigades (p083); about 12,500 men | per-brigade records; brigade count by audit |
| C09 | Bombardment 150 to 170 guns (p083); Alexander's battalion "about 50+" (p082) | battery-level `guns` sums; range until audited |
| C10 | East Cavalry Field: Stuart 5,000 present or 3,430 engaged; Union about 3,250; losses 254 (219 in Custer's brigade) and 181 (p083) | `present` and `engaged` both kept |
| C11 | Iron Brigade 1,153 of 1,885; 24th Michigan 397 of 496; 1st Minnesota 215 of 262 (p286; p082 "about 82%") | `pct` gives 61, 80, 82 (control) |
| C12 | Brigades named by commander: "Nicholls's/Walker's", "Hoke's ... under Avery" (p082), Pettigrew's brigades as [Fry, Marshall, Davis, Brockenbrough] (p083); plan section 4.3 names the Stonewall, Florida and Georgia brigades | one brigade record per designation; two simultaneous `command` stints fail GB13 |
| C13 | V Corps "Meade then Sykes" (p072); Meade in army command from 1863-06-28T03:00 (p079) | Meade's V Corps stint ends at or before that stamp |
| C14 | Weather: high 80s F on July 1 to 3, violent thunderstorm night of July 3 to 4 (p079); rain began July 4 (p084); thunderstorms July 4 (p116) | onset claims kept; `unverified` |

Probe records live in `qc/fixtures/gb/bad/` (first line `# expect: GBnn`) and `good/`.

## 10. Public API (`tools/gb_audit.py`, Python 3.11 or later, standard library plus `jsonschema`)
```
t_min(stamp: str) -> int          stamp_of(t: int) -> str          hour_of(t: int) -> int
load_corpus(root: str) -> Corpus                      # sorted file walk, no clock
check_corpus(c: Corpus) -> list[Finding]              # Finding(path, rule, severity, message), sorted
check_fixtures(c: Corpus, fixture_dir: str) -> list[Finding]
hourly_schedule(c: Corpus) -> list[tuple[int, str, str, str]]
reconcile(first: Corpus, second: Corpus) -> list[dict]
training_class(raised: str, prior_battles: int, as_of: str) -> str
write_report(path: str, report: dict, date: str) -> None
```
CLI: `python3 tools/gb_audit.py check|hourly|reconcile A B|report PACKET --date D`; exit 1 on any E finding.

**Extending `tools/validate_data.py`** (T-131 edits it): add `GB_SCHEMAS = {"units": "gb-regiment", "batteries": "gb-battery", "arrivals": "gb-arrival", "terrain": "gb-terrain-object", "events": "gb-timeline-event", "weather": "gb-weather"}` and `check_gettysburg(reg) -> (files, problems)`, called in `main()` after `check_rules`, adding to `count` and `problems`. It returns `(0, 0)` when `data/history/gettysburg` is absent; validates `sources.toml` against `gb-regiment.schema.json#/$defs/source_registry` and each record against its schema (existing message format, rule R8 for schema errors and floats); imports `gb_audit` (tools directory put on `sys.path`) and prints each finding as `path: GBnn message`, counting only E. R6 also runs `check_fixtures` on `qc/fixtures/gb-known-conflicts.toml`. `tools/test_validate_data.py` gains throwaway-root tests.

## 11. Performance budget (hypothesis H13)
`check` on 2,000 records and 200 reports in at most 10 s; whole validator at most 30 s (CT-1305). Record count is unknown until the skeleton exists.

## 12. Acceptance tests (tests first; thresholds are not edited to pass)
- **T13-01 schemas:** each of the six files passes the Draft 2020-12 metaschema check; every file in `qc/fixtures/gb/good/` validates; each file in `bad/` fails with its expected code.
- **T13-02 time:** `t_min` of `1863-07-01T00:00`, `1863-06-28T03:00`, `1863-07-02T16:30`, `1863-07-03T15:00`, `1863-07-14T23:59` equal 0, -4140, 2430, 3780, 20159; `stamp_of(t_min(s)) == s` for every minute in the window; `hour_of(-1) == -1`.
- **T13-03 chain sums:** parent 100 to 120 with children 40 to 50 and 60 to 65 passes; parent 130 to 140 fails; an all-verified case passes at equal and fails one off; a child without `engaged` gives an I finding.
- **T13-04 casualties:** component, day-scope and loss-above-engaged cases each pass and fail as intended; `pct(1153,1885) == 61`, `pct(397,496) == 80`, `pct(215,262) == 82`; C03 flagged.
- **T13-05 commanders:** overlapping stints fail; a shared endpoint passes; a killed commander with a later stint fails; a wounded commander with `remained_in_command` gives W; C13 passes at the boundary stamp and fails one minute later.
- **T13-06 ladder:** `verified` fails with one claim, two claims of one lineage, two disagreeing lineages, an unconsulted source, or CWV alone; passes with two agreeing consulted lineages and a matching `second_source`.
- **T13-07 conflicts:** every C01 to C14 row loads; a synthetic record marking each conflict `verified` raises GB17.
- **T13-08 determinism:** `check` output is byte-identical across runs and under `PYTHONHASHSEED` 0 and 1; the file contains no wall-clock call.
- **T13-09 reports:** a valid report passes; a blind pass with `saw_prior_values = true`, a reviewer session equal to an audited session, and a waiver without reason each fail.
- **T13-10 schedule:** three arrivals with equal hours order by formation id then event rank; `training_class` on (1862-08-01, 0), (1863-05-01, 0), (1861-07-01, 3), (1862-08-01, 2) gives Trained, Raw, Veteran, Trained.
- **T13-11 cross-checks:** a weapon id or terrain class absent from the rules tables raises GB12 or GB18.
- **T13-12 validator:** `python3 tools/validate_data.py` exits 0 on the repository and 1 on a throwaway root with a bad unit; existing tests pass unchanged.

## 13. Calibration targets
CT-1301 `TIME_TOL_MIN` against audited first-hand accounts. CT-1302 training thresholds against SPEC-004 CT-5. CT-1303 review sample against the pilot packet's defect rate. CT-1304 the 60-minute child-before-parent warning against audited arrivals. CT-1305 performance. None changes a stored fact.

## 14. Assumptions
A1 Stamps are reported clock times; no zone or railroad-time correction. A2 Frame `gb_local_v1` (origin, axes, millimetres) is fixed by the terrain pipeline spec (SPEC-014); until then `x_mm`, `y_mm` are optional. A3 Weather fields are reconciled with SPEC-009 by amendment. A4 Weapons beyond the five in `weapons.toml` need a SPEC-005 amendment. A5 Counsel decides what each `licence_class` permits; the checker enforces only `cleared_by`. A6 Auditors are agents with human historian sign-off.

## 15. Risks
Sources may be unavailable or tightly licensed (fallback: public-domain returns only, fewer `verified` facts). A wrong lineage judgement gives a false `verified` (the Reviewer re-derives every promotion). Wide ranges weaken GB10, so verified data use the exact test. Returns define strength differently: separate fields. Source times are coarser than minutes: `early` and `late` carry that.

## 16. Done when
Schemas, registry, `gb_audit.py`, validator extension and conflict fixtures are committed and pass T13-01 to T13-12; CWV pilot records (T-136) validate at `partial` or lower; scans pass; ADR and ledger rows written; no record cites an unconsulted source.
