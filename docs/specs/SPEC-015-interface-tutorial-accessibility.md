# SPEC-015: interface, tutorial, accessibility and localisation

Crate: `crates/ui-model` (new; add to workspace `members`). Depends on `core-math`, `sim-state`, `battle-rules` (path) and `proptest` (dev); data are read through `battle_rules::parse_table` (ADR 0004), so no new third-party dependency. `#![forbid(unsafe_code)]`, integers and `Fx` only, ascending-id iteration, no `HashMap`, no clock. Rendering, input and fonts live in `shells/` and may use floats; nothing they compute flows back.
Source key: **CWV pNNN** = "The American Civil War: Research Volume and Game Design", PDF page NNN (design proposals, no evidence tags). No number here is historical: each is a `placeholder` with a calibration target (CT-n, section 15).

## 1. Purpose and scope
What the player sees and how it is derived: command and field views, replay and photo mode, overnight planning, tutorial, guide, difficulty, accessibility, localisation, usability plan. Out of scope: rendering, audio mixing, art, and the courier and morale rules (consumed through section 3 contracts).
Tone follows CWV p516 (its section 14): sober, no kill counters, no fanfare. The result screen states the fidelity class that ran (ADR 0002; CWV p513 section 8) and lists casualties by regiment (CWV p512 section 4.9).

## 2. Definitions
- **Snapshot**: immutable copy of everything the interface may show for one tick. **View model**: a pure function of a Snapshot. **Display field**: any numeric view-model field.
- **HQ view** (default, plan 2.2): what headquarters knows, delayed and partial. **Full view**: reads truth, optional.
- **UiCommand**: the only way the interface changes the simulation, by becoming an input record (SPEC-002).

## 3. Read-only architecture and read contracts (checklist item 7)
Every number shown is a simulation value or a printed quantisation of one; the interface has no statistic of its own (test T15-02).
1. `capture(&BattleState, &dyn InfoSource, &dyn MoraleSource, InfoMode, start_clock_s: u32) -> Snapshot` takes shared references only. Own regiments come from `BattleState`; enemy regiments come **only** from `InfoSource::sightings` in HQ view. `clock_s = start_clock_s + tick / 20`.
2. Snapshots refresh every `snapshot_every_ticks = 2`; the renderer interpolates (shell side). `soldier_frame(&BattleState, &mut SoldierFrame)` copies `pos, facing, flags, unit` columns in slot order.
3. `UiCommand::Order{regiment, order}` becomes an input record at the current tick with payload `encode_order` (SPEC-004 section 11). Pause, speed, camera, selection, overlays and tutorial state are `UiState`: never hashed or recorded.
4. Difficulty is recorded as one tick-0 input (actor `0xFF`, seq 0, `encode_difficulty`); replay format v1 is unchanged.
5. Contracts owned by later specs, defined here as the interface they must satisfy (section 16):
```rust
pub struct Sighting { pub unit: u16, pub last_seen_tick: u64, pub pos: Vec2Fx, pub pos_err_mm: u32, pub strength_lo: u16, pub strength_hi: u16, pub formation_known: bool }
pub struct OrderRecord { pub id: u32, pub regiment: RegimentId, pub order: Order, pub sent_tick: Option<u64>, pub eta_lo: u64, pub eta_hi: u64, pub eta_hi0: u64,
  pub ack_tick: Option<u64>, pub interpreted: Option<Order>, pub executed_tick: Option<u64>, pub completed_tick: Option<u64>, pub lost_known: bool }
pub struct MoraleReport { pub level: u8 /*0 steady,1 uneasy,2 wavering,3 breaking,4 broken*/, pub margin: i32 /*model units above the next worse level*/, pub trend: i8, pub factors: Vec<(u8, i32)> /*factor id, signed contribution; sums to the model's total*/ }
pub trait InfoSource { fn sightings(&self, side: u8, now: u64, out: &mut Vec<Sighting>); fn orders(&self, side: u8, out: &mut Vec<OrderRecord>); fn eta_window(&self, regiment: RegimentId, order: &Order, now: u64) -> (u64, u64); }
pub trait MoraleSource { fn report(&self, r: RegimentId) -> MoraleReport; }
```
`FullInfo` (truth, all confidence Firm) implements `InfoSource` for Full view and the tutorial.

## 4. Command view
Period-map style (art direction is not specified here). **Marker** per regiment: position, heading, formation (shape cue: line = bar, column = block, skirmish = dotted bar; side = outline shape plus the letters U or C in text mode), strength shown as an exact figure for own units and as a range `strength_lo..strength_hi` rounded to 50 for enemies, order badge (section 5), morale glyph (section 6).
**Confidence shading** (enemy markers only):
`score_pm = max(0, 1000 - age_s * age_pm_per_s - pos_err_mm * err_pm_per_100m / 100000)`, `age_s = (now - last_seen_tick) / 20`, integer floor. Bands: Firm >= 750 (solid outline, opaque), Probable >= 500 (dashed, 75 percent), Possible >= 250 (dotted, 50 percent, uncertainty circle of radius `pos_err_mm`), Stale below 250 (hollow ghost labelled with the last-seen clock time). Defaults `age_pm_per_s = 5`, `err_pm_per_100m = 100` (CT-1).
**Orders panel**: `OrderRecord`s newest first with status text (section 5); drafting shows `eta_window` as clock times.
**Timeline**: one lane per order (hatched bar over `[eta_lo, eta_hi]`, markers for acknowledgement, start, completion), plus sighting and morale events on a shared axis. Scrubbing is replay-only.
**Overlays** (`overlay(kind, &BattleState) -> OverlayGrid { cell_mm, width, height, origin, level: Vec<u8> }`): Terrain = `TerrainGrid` class id; Cover = `cover_pm` of the class (SPEC-004 section 5, used by SPEC-005); Smoke = `SmokeGrid.density` (SPEC-005 section 8); Supply = rounds per soldier (SPEC-005 `FireExt`) averaged per regiment, plus train data when the operational spec supplies it. Level = number of thresholds in `data/ui/overlays.toml` that the true value reaches (placeholder, CT-4). Legends print the thresholds and the tooltip prints the exact value.

## 5. Order status (checklist item 11)
`derive_status(&OrderRecord, now, &StatusParams) -> OrderStatus`, first match wins:
1. `completed_tick` set: `Completed`. 2. `lost_known`: `Lost`.
3. `ack_tick` set: `Misread` if `interpreted` is `Some(i)` with `i != rec.order`, else `Executing` if `executed_tick` set, else `Received`.
4. `sent_tick` is `None`: `Drafted`.
5. `now <= eta_hi`: `Delayed { by_ticks: eta_hi - eta_hi0 }` if `eta_hi > eta_hi0`, else `InTransit`.
6. `grace = (eta_hi - sent_tick) * presume_lost_pm / 1000`: `now <= eta_hi + grace` gives `Overdue` (no word yet), otherwise `PresumedLost`. `presume_lost_pm = 500` (placeholder, CT-2).
`OrderView.misread` is true whenever `interpreted` differs from the order, also after `Completed`. Each status has a distinct icon shape and one string-table sentence ("{regiment}: order understood as {understood}"). Loss or misreading is learned only through the information model; after the battle, replay **truth view** may show real fates, labelled as such.

## 6. Morale feedback (checklist item 12)
`morale_view(&MoraleReport, &MoraleViewParams) -> MoraleView { level, warn: bool, trend, top: [(factor, Band); 3] }`. Each level has a word, a distinct icon shape and a symptom set from `data/ui/symptoms.toml`: fire slowed (the figure is the actual loading multiplier), men looking back, ranks thinning. `top` holds the three factors with the largest `|contribution|` (ties by lower factor id), each banded Large (`|c| >= large`), Moderate (`>= moderate`), Slight, and rendered as a sentence ("Heavy losses in the last five minutes"). `warn` is true when `level == 2` and `margin < warn_margin` (placeholder, CT-3). A level change raises one toast naming the top factor, optional auto-pause, and a log entry. No hit-point bar exists.

## 7. Field view, cards, replay and photo mode
- **Camera**: `CameraPose { pos_mm: [i64; 3], yaw: Angle, pitch_urad: i32, fov_pm: u16 }` (integers, so paths reproduce exactly); modes free, orbit, follow.
- **Regimental card**: real name, side, strength, formation, order status, morale view, ammunition band, history of at most 60 words (section 9). Minimal HUD: clock, speed, pause mark, card, toasts; each can be hidden.
- **Replay**: replays hold hashes, not states, so the viewer keeps `BattleState` keyframes every `keyframe_interval_ticks = 6000` (CT-10); seek = nearest earlier keyframe plus stepping. A checkpoint hash mismatch shows a divergence banner.
- **Photo mode**: pause, free camera, integer `CameraPath`, render options (exposure, depth of field, hide HUD) that never change state, weather or time. Sidecar `*.hdphoto` (TOML): scenario id, tick, `state_hash`, camera, options.

## 8. Overnight planning screens
State machine `OvernightStep`: Situation (strength, fatigue, ammunition, casualties as known) -> Lines and reserves -> Trains (ammunition, ambulance; CWV p512 section 4.8) -> Next-day orders -> Review and confirm. No clock; undo until Confirm. Review lists units that will arrive tired, with fatigue, and what is unknown. Each move shows `march_ticks(route, gait, &Tables) -> u64`: per whole 1000 mm step, class at the step midpoint, `micro += 20_000_000_000_000 / (speed_mm_s * speed_pm)` with `speed_mm_s` from `LocomotionTable` and `speed_pm` from `TerrainTable`; the last partial step adds `per_metre * r / 1000`; result `ceil(micro / 1_000_000)`. **Forecast**: `n_forecast = 5` headless quick-resolve runs (ADR 0002) on a *believed* world built from the HQ view, never the true one; shows ranges and the fidelity class, labelled "forecast, not prediction". Confirm emits `UiCommand`s for tick 0 of the next scenario.

## 9. Battlefield guide and data
Entries `data/guide/{places,regiments,people,events}/*.toml`: `id`, `title_key`, `text_key`, `links[]`, `reveal` (`always` | `after_scenario`, so the guide never leaks what the information model withholds), `disputed` (text gives each account and says the record is disputed, plan section 9), `review` (`pending` | `approved`; release needs `approved`, CWV p515, p516) and provenance as SPEC-004 section 2. Content on slavery and on Black residents and refugees is included and must not make the Confederate position look easier than it was (CWV p509 section 3.6, p516, its section 14).
Other files: `data/ui/{confidence,status,morale,overlays,symptoms,difficulty,palettes,tutorial,display-fields}.toml`, schemas `data/schemas/ui-*.schema.json`, strings `data/text/<locale>/<domain>.toml`.

## 10. Difficulty, pause and speed
`Difficulty { ai_initiative_pm, ai_risk_pm, info_noise_pm, courier_spread_pm, assists: u8 }`. The first four are behaviour: AI initiative and risk tolerance, the scale of sighting error and of courier transit spread. `assists` bits: show ETA window, auto-pause on events, order preview, full view (flagged on the result screen), historical restrictions. Presets `guided`, `standard`, `exacting`, `custom` (CT-5). No key may scale a weapon, damage, ammunition, locomotion, drill or terrain value; the options screen lists every key with its value and effect in words. Noisy reports show their real noise as ranges.
Pause is accepted in every `UiMode` (Battle, Replay, Photo, Overnight, Tutorial, Briefing, Menu); orders may be sent while paused, stamped with the current tick. Speed `speed_pm` is one of 250, 500, 1000, 2000, 4000, 8000, 16000 (cap 4000 in live play), an integer ticks-per-frame accumulator.

## 11. Tutorial, accessibility, localisation, usability
**Tutorial** (`data/ui/tutorial.toml`, scenario `drill_ground`, real simulation, period drill language to be checked against the manuals, CWV p453): modules and minutes: M1 map 4, M2 orders and formations 6, M3 marching and fatigue 5, M4 fire and ammunition 6, M5 morale, reading why 6, M6 couriers and delay 6, M7 information and confidence 3, M8 small action 4 (sum 40, placeholder CT-9). Objectives and hints are predicates over Snapshot and `UiEvent`. **Skip path**: skip all at first launch, or skip a module by performing its objectives unprompted; skipped players keep contextual hints, up to 3 per mechanic, at most one per 60 s of UI time, never blocking, shown while paused, none time-limited.
**Accessibility** (every item has a test):
- Remapping: `Bindings`, up to 3 `Chord`s per `Action`, conflict detection, reset, TOML profile; no action needs a hold or the mouse.
- UI scale 750 to 2000 permille in steps of 50; smallest body text 14 px at 1080p, 100 percent (CT-6).
- Palettes: default, protan, deutan, tritan. Every information channel (side, confidence, status, morale, overlay level, selection) has at least two cues, one not colour.
- Subtitles for all speech with speaker and direction, shown when the sound arrives (CWV p514 speed-of-sound delay); at most 2 lines of 42 characters; `duration_ms = max(1500, 1000 + 60 * chars)` (CT-7); captions for key non-speech sound.
- Reduced motion (no shake, pulses or animated transitions) and reduced flash: `FlashGovernor::admit` allows at most 3 luminance transitions covering 25 percent of the view or more in any 20 ticks.
- Keyboard navigation: deterministic `FocusGraph`, map cursor in 500 m and 20 m steps, next or previous regiment by id; every view model has a screen-reader text.
**Localisation**: `StringTable` per locale; keys `domain.name`, plural forms `key#one|few|many|other` chosen by `plural_category(locale, n)`; arguments are typed (`Int`, `Name`, `Clock`, `Distance`, `Percent`), formatted per locale and per unit option (yards and miles, or metric). A sentence is one key with placeholders; code never joins translated fragments. A missing key falls back to English with a `[!key]` marker in debug and fails CI in a shipped locale. `pseudo_localise(&str, PseudoMode) -> String` (accented, plus 40 percent length, bracketed, placeholders untouched; RTL mode adds bidi marks).
**Usability plan**: each round has at least 10 new testers (3 using accessibility options, 3 without strategy-game experience), moderated think-aloud, scripted tasks, opt-in local telemetry. Criteria in `qc/usability/criteria.toml` (permille): U1 unaided tutorial completion >= 800 (aid = moderator help; game hints are not aid); U2 blocking findings <= 2; U3 order status comprehension >= 900 (five orders, UI only); U4 morale reason named from the card within 15 s >= 850; U5 more certain of two enemy markers picked without legend >= 850; U6 median time to first order <= 120 s; U7 smallest-size text read correctly >= 950; U8 SUS score >= 70 (CT-11). A failed round is followed by fixes and a new round with new testers.

## 12. Public API (exact)
```rust
pub fn snapshot_hash(s: &Snapshot) -> u64;                       // FNV-1a 64, canonical little-endian encoding
// capture, soldier_frame, the traits and record types: section 3
pub fn confidence_pm(age_ticks: u64, err_mm: u32, p: &ConfidenceParams) -> u16;
pub fn confidence_band(pm: u16) -> ConfBand;                     // Firm, Probable, Possible, Stale
pub fn derive_status(rec: &OrderRecord, now: u64, p: &StatusParams) -> OrderStatus;
pub fn morale_view(r: &MoraleReport, p: &MoraleViewParams) -> MoraleView;
pub fn overlay(kind: OverlayKind, b: &BattleState, p: &OverlayParams) -> OverlayGrid;
pub fn march_ticks(route: &[Vec2Fx], gait: Gait, t: &Tables) -> u64;
pub fn encode_difficulty(d: &Difficulty) -> Vec<u8>; pub fn decode_difficulty(b: &[u8]) -> Result<Difficulty, UiError>;
impl Bindings { pub fn bind(&mut self, a: Action, c: Chord) -> Result<(), BindError>; pub fn resolve(&self, c: Chord) -> Option<Action>; pub fn to_toml(&self) -> String; pub fn from_toml(s: &str) -> Result<Bindings, UiError>; }
impl FlashGovernor { pub fn admit(&mut self, tick: u64, luminance_delta_pm: u16, area_pm: u16) -> bool; }
impl StringTable { pub fn load(locale: &str, toml: &str) -> Result<StringTable, UiError>; pub fn tr(&self, key: &str, args: &[Arg]) -> String; }
pub fn pseudo_localise(s: &str, m: PseudoMode) -> String;
pub fn plural_category(locale: &str, n: i64) -> Plural;
pub fn tutorial_step(s: &mut TutorialState, snap: &Snapshot, ev: &[UiEvent]) -> Vec<TutorialEvent>;
```

## 13. Performance budget (hypotheses, CT-10)
`capture`, 300 regiments: mean 0.3 ms, max 1 ms. `soldier_frame`, 20,000 soldiers: mean 1 ms. Overlay, 256 x 256 cells: 1 ms, on tick change only. Interface logic: 2 ms per frame. Replay keyframes: 150 MB for a four-hour battle.

## 14. Acceptance tests (tests first; goldens recorded once from the stated reference)
- **T15-01 read-only:** a proptest of 1,000 random UI action sequences (selection, camera, overlays, pause, speed, drafts) interleaved with steps leaves `battle_hash` equal to a run with no interface; a source scan finds `issue_order(`, `spawn_` or `&mut BattleState` nowhere in `crates/ui-model/src` outside `commands.rs`.
- **T15-02 no hidden values (item 7):** every display field is listed in `data/ui/display-fields.toml` with `source` and `kind` (`exact`, `band`, `range`), and the list equals the field set of `Snapshot` and the view models. Over 200 random battles each `exact` field equals the simulation value, each `band` equals the quantisation printed in its tooltip, each `range` brackets the truth. Moving a simulation value across a band threshold changes the band. `tools/validate_ui.py` fails on a field without a source.
- **T15-03 pause and speed:** all seven speeds with pauses at random ticks give the same hash at the same tick count; `Pause` is accepted in every `UiMode`.
- **T15-04 confidence:** with defaults, (age 0, err 0) gives 1000 Firm; (60 s, 100 m) 600 Probable; (100 s, 0) 500 Probable; (150 s, 0) 250 Possible; (120 s, 300 m) 100 Stale. Non-increasing in age and error (proptest). In HQ view, changing enemy `BattleState` positions arbitrarily with a fixed `InfoSource` leaves `snapshot_hash` unchanged.
- **T15-05 order status (item 11):** table-driven over all statuses; `now == eta_hi` gives `InTransit`, `eta_hi + 1` `Overdue`, `eta_hi + grace + 1` `PresumedLost`; the misread flag persists after `Completed`; `Lost` shows only when `lost_known`. A scripted fixture (delay, loss, misread) yields a per-tick status list recorded once from the information-model fixture.
- **T15-06 morale (item 12):** over 1,000 random reports, `top` equals the three largest `|c|` with the tie rule, shown plus remaining factors sum to the report total, `warn` follows the rule, and every level has a distinct icon id and symptom set.
- **T15-07 overlays:** level equals the threshold count for each kind; Cover reads `TerrainTable.cover_pm`; Smoke reads `SmokeGrid.density`; cells are row-major from the south-west.
- **T15-08 march:** 1000 m of open ground at `quick` 1303 mm/s with the placeholder table gives 15350 ticks (`2e13 / (1303 * 1000)` = 15,349,194 micro-ticks per metre); 1000 m of woods at 600 permille gives 25582. A zero-length route gives 0.
- **T15-09 difficulty:** presets differ only in whitelisted keys; difficulty data references no `data/rules` table; two runs differing only in `assists` (not full view) have equal hashes; encode and decode round-trip.
- **T15-10 bindings:** every action binds to every key of a fixture set, conflicts are reported, TOML round-trips, no action is unbound by default, every action runs from the keyboard alone.
- **T15-11 palettes and cues:** `tools/validate_ui.py` simulates the three deficiencies and requires CIE76 delta E >= 25 between categorical colours used together, text contrast >= 4.5:1, graphics >= 3:1 (CT-6); every channel lists two cue kinds, one non-colour.
- **T15-12 flash and motion:** 100 qualifying flashes at 20 per second admit at most 3 per 20-tick window; `reduce_motion` sets shake, pulse and transition durations to 0.
- **T15-13 captions:** every audio id flagged `speech` has a string key; duration and line limits hold.
- **T15-14 localisation:** placeholders survive `pseudo_localise`; `en` plurals (1 is `one`, else `other`); a lint finds no user-visible literal outside keys and no joined fragments; a stub-metric text-fit test passes at scales 750, 1000, 1500, 2000 with +40 percent text; a missing key in a shipped locale fails.
- **T15-15 tutorial:** a scripted bot on the real simulation reaches every objective of M1 to M8; minutes sum to 40 plus or minus 2; skip marks modules skipped and keeps hints; hint cadence and caps hold; tutorial state is absent from `battle_hash`.
- **T15-16 replay and photo:** seeking to tick t via keyframes gives a straight run's hash; a sidecar round-trips; a corrupted checkpoint raises the divergence banner.
- **T15-17 overnight:** Confirm is reachable only through all steps; undo restores the prior plan; confirmed orders decode via `decode_order` to the planned orders; the forecast input is unchanged when true enemy positions change.
- **T15-18 usability tooling:** `tools/usability_report.py` reads `qc/usability/criteria.toml` and a round CSV and prints pass or fail per criterion; a test asserts U1 is at least 800.

## 15. Calibration targets
CT-1 confidence weights and bands (playtest U5). CT-2 overdue and presumed-lost grace (U3). CT-3 morale warning margin and factor bands (U4). CT-4 overlay thresholds. CT-5 difficulty presets, fitted by bot runs so levels differ measurably. CT-6 type size, scale, contrast, delta E. CT-7 caption timing. CT-8 pseudo-localisation expansion. CT-9 tutorial minutes and hint cadence (median completion 40 plus or minus 5 minutes). CT-10 snapshot cadence, keyframe interval, performance. CT-11 usability thresholds other than U1.

## 16. Assumptions and risks
Assumptions: (1) the command layer provides `InfoSource`, the morale model provides `MoraleSource` with factor terms summing to its total; both must satisfy section 3 or amend this spec. (2) The operational spec provides train and line data for section 8. (3) Replay v1 accepts an actor `0xFF` input. (4) Terrain ground height for the camera comes from art data.
Risks: the information model may expose less than the order states need (states then merge, never invent); morale terms may not be additive; keyframes may exceed memory on small machines (raise the interval); drill vocabulary is unverified until the manuals are read; guide text depends on advisory review.

## 17. Done when
fmt, clippy `-D warnings`, `cargo test --workspace`, the scans and `tools/validate_ui.py` pass; T15-01 to T15-18 pass; no floating point in `crates/ui-model`; a usability round report is committed.
