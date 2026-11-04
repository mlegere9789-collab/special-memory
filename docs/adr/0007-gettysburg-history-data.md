# ADR 0007: Gettysburg history data

**Status:** accepted.

**Decision.** The Gettysburg order-of-battle records (SPEC-013) follow five rules.

1. **Integer-only records.** Every record under `data/history/gettysburg/` is integer-only TOML, one record per file, file stem equal to `id`. A float anywhere is refused (rule GB02), as in ADR 0004. Unknown values are omitted, never guessed. Strengths and losses are whole men or guns; fractions are written as integers in a named unit.
2. **Claims and lineage.** A fact stores each source's figure or time as a claim with document code and page. Sources that copy one another share a lineage, and two sources confirm a fact only when their lineages differ. `verified` needs two agreeing claims from different lineages, both from consulted and licence-cleared sources, a `second_source` of the form `CODE:page`, and the Reviewer's sign-off. CWV tags never lift a fact above `partial`.
3. **Source registry.** `data/history/gettysburg/sources.toml` lists every source. A source is consulted only when its row records edition, access, licence text, date and page scheme. A record may cite only a consulted source (GB04), and a `permission_required` or `unknown` source also needs `cleared_by` (GB05). At registry creation only CWV is consulted; every other row is a candidate with `consulted = false` and no edition or licence text.
4. **Local clock stamps.** Times are `1863-MM-DDThh:mm`, the clock time as the source reports it, with no zone and no railroad-time correction. `t_min` counts minutes from `1863-07-01T00:00` (negative before it) using the proleptic Gregorian day count; the campaign window is `[-4320, 20160)` and the battle days are `[0, 4320)`. Hour buckets use floor division. Times from sources coarser than a minute carry `early` and `late`.
5. **Calibration range.** Invented numbers in this data and tooling are placeholders that name a target in the `CT-013-1301` to `CT-013-1305` range (REGISTRY section 7): the time tolerance, training thresholds, review sample, the child-before-parent warning and the performance budget. None of them changes a stored fact.

**Why.** Historical figures disagree between sources and between pages of one source, and the simulation must not inherit a guess as a fact. Keeping claims, lineage and consulted status in the data lets a checker refuse a false `verified` mechanically. Integer-only records and local stamps keep the audit output byte-identical across platforms, with no clock and no float in the tooling.

**Consequences.** `tools/gb_audit.py` holds the time functions, the registry reader, the GB04 and GB05 checks and the report writer; its report date is passed in and never read from a clock. Later tickets of SPEC-013 add the corpus checks and the validator plugin. Adding a source or clearing one is a change to `sources.toml` only.
