# SPEC-004: battle-rules I, movement, drill and formations

> Shared namespaces (input tags and actors, random streams, flags, hash blocks, tick order, CT ids, amendment labels, ticket ids) are owned by [REGISTRY.md](REGISTRY.md); conflicts and their resolutions are in [CONFLICTS.md](CONFLICTS.md).

Crate: `crates/battle-rules` (new; add to workspace `members`). Depends on `core-math`, `sim-state` (path), `serde` and `toml` (exact-pinned, rules loader only), `proptest` (dev). `#![forbid(unsafe_code)]`. SPEC-005 adds weapon modules to the same crate. State is `Fx`/integers only, iteration is ascending id, no `HashMap`.
Source key: **CWV pNNN** = "The American Civil War: Research Volume and Game Design", PDF page NNN, with the tag the page gives. An invented number is `placeholder` with a named calibration target (CT-004-n, section 10).

## 1. Conventions
- `TICK_HZ: u32 = 20` (matches SPEC-003). Lengths in metres (`Fx`), speeds in m/s, angles in `Angle`.
- `fx_ratio(num: i64, den: i64) -> Fx`: raw = `((num as i128) << 32) / den` truncating toward zero, saturated; `den == 0` follows core-math `Div` rules.
- **Data units are integers**: millimetres (`_mm`), mm/s (`_mm_s`), permille (`_pm`, 1000 = 1.0), parts per million (`_ppm`), ticks (`_ticks`). The loader rejects TOML floats and unknown keys. Data become `Fx` only through `fx_ratio`.
- Named streams (`StreamId`): `STREAM_BATTLE_SETUP = 0x0B01` (scenario jitter only); per-soldier streams `0x4000_0000 + slot_index` are defined in SPEC-005.
- Flag bits in `AgentStore.flags`: `F_KNEEL = 1<<0`, `F_PRONE = 1<<1`, `F_DISABLED = 1<<2` (down for N ticks), `F_WOUNDED = 1<<3` (out for the battle), `F_GUN = 1<<4`. `AgentStore.unit` holds the `RegimentId`; `team` holds the side. `hp` is 100 at spawn (SPEC-005 owns damage).

## 2. Provenance fragment (add `data/schemas/rules-common.schema.json`)
```json
{"$id":"rules-common.schema.json","$defs":{"provenance":{"type":"object","additionalProperties":false,
 "required":["confidence"],"properties":{
  "confidence":{"$ref":"common.schema.json#/$defs/confidence"},
  "placeholder":{"type":"boolean"},
  "calibration_target":{"type":"string","pattern":"^CT-[0-9]{3}-[0-9]+$"},
  "sources":{"$ref":"common.schema.json#/$defs/sources"},
  "second_source":{"type":"string"},"note":{"type":"string"}},
 "allOf":[{"if":{"properties":{"placeholder":{"const":true}},"required":["placeholder"]},
           "then":{"required":["calibration_target"]},"else":{"required":["sources"]}}]}}}
```
Data rows in sections 3 to 6 carry these fields flattened (`confidence`, `placeholder`, `calibration_target`, `sources`); the loader enforces the same rule (`DataError::Provenance`).

## 3. Locomotion (`data/rules/locomotion.toml`, schema `locomotion.schema.json`)
The research text names Hardee (1855) and Casey (1862) as the drill references and says it did not open them (CWV p453, `[Memory; I did not open these manuals]`). **No page gives a drill rate**, so all four speeds are placeholders. Candidates are remembered cadence times step length (90, 110, 165 steps/min at 28, 28, 33 inch), unsourced; replace with manual-sourced figures (CT-004-1).

| id | speed_mm_s | drain_ppm_per_tick | basis |
|---|---:|---:|---|
| walk (common time) | 1067 | 7 | placeholder, CT-004-1, CT-004-2 |
| quick | 1303 | 28 | placeholder, CT-004-1, CT-004-2 |
| double_quick | 2305 | 139 | placeholder, CT-004-1, CT-004-2 |
| run | 3600 | 1111 | placeholder, CT-004-1, CT-004-2 |

Globals (all placeholder, CT-004-3): `accel_mm_s2 = 3000`, `body_radius_mm = 250`, `personal_space_mm = 500`, `avoid_gain_mm_s = 1200`, `max_lag_mm = 6000`, `catchup_mm = 1500`, `catchup_fatigue_limit_ppm = 600000`, `rest_recover_ppm_per_tick = 83`, `rest_speed_mm_s = 100`.
Fatigue speed curve `[[fatigue_curve]] fatigue_ppm, speed_pm`: (0,1000), (500000,900), (800000,650), (1000000,400); placeholder, CT-004-2.
Schema (`locomotion.schema.json`): required `schema` (const `"locomotion/1"`), `tick_hz` (const 20), `gait[]` (one item per id: `id` enum, `speed_mm_s` int>0, `drain_ppm_per_tick` int>=0, provenance), `globals` (the integer keys above), `fatigue_curve[]` (`fatigue_ppm` 0..1000000 strictly increasing from 0 to 1000000, `speed_pm` 0..1000 non-increasing).

## 4. Fatigue (integer model)
Per-soldier `fatigue: u32` in `0..=1_000_000` (ppm of exhaustion). Each tick, in this order:
1. Moving (applied displacement > 0): `fatigue += drain_ppm_per_tick(gait) * terrain.effort_pm / 1000` (integer floor). Catch-up gait (section 7) uses the double-quick drain.
2. Not moving (displacement 0 or speed below `rest_speed_mm_s`): `fatigue -= rest_recover_ppm_per_tick`.
3. Clamp to `[0, 1_000_000]`. SPEC-005 adds a firing drain in the same step.
`fatigue_speed_pm(f)`: linear interpolation between the two bracketing curve points, `p = p0 + (p1-p0) * (f-f0) / (f1-f0)` with `i64` division truncating toward zero.
Targets (placeholders, +-20 percent, CT-004-2): a fresh soldier on open ground reaches 500000 ppm after 900 s at quick and 180 s at double-quick, 1000000 after 45 s at run; at rest 500000 recovers to 0 in 300 s. The section 3 drains derive from these (500000 / (180 s * 20) = 139).

## 5. Terrain (`data/rules/terrain.toml`, schema `terrain.schema.json`)
`[[class]]` rows: `id: u8` (unique, ascending), `name`, `speed_pm`, `effort_pm`, `line_disorder_pm`, `cover_pm` (consumed by SPEC-005), `blocks_movement: bool` + provenance. Qualitative source: woods slow movement and break formations (CWV p511 section 4.4); swamps, creek bottoms and thickets are named obstacles (CWV p065, terrain bullets before section 9.3; CWV p511). All numbers are placeholders (CT-004-4).

| id | name | speed_pm | effort_pm | line_disorder_pm | cover_pm | blocks |
|---:|---|---:|---:|---:|---:|---|
| 0 | open | 1000 | 1000 | 0 | 0 | no |
| 1 | road | 1050 | 900 | 0 | 0 | no |
| 2 | orchard | 850 | 1100 | 150 | 150 | no |
| 3 | woods | 600 | 1400 | 400 | 300 | no |
| 4 | thicket | 350 | 1800 | 600 | 450 | no |
| 5 | marsh | 450 | 1800 | 300 | 0 | no |
| 6 | ford | 500 | 1500 | 300 | 0 | no |
| 7 | deep_water | 0 | 1000 | 0 | 0 | yes |

Grid file (`data/scenarios/<name>.toml`, table `[terrain]`): `cell_mm` (default 8000), `origin_x_mm`, `origin_y_mm`, `legend` (single char to class id, array of `{ch, class}`), `rows` (array of equal-length strings, row 0 at the south). `TerrainGrid::class_at(pos) -> u8`: cell index = floor((pos - origin) / cell) clamped to the grid (outside positions take the nearest edge cell). Positions entering a `blocks_movement` cell are not applied (displacement for that axis step is zero; soldier slides along the other axis if free).

## 6. Drill (`data/rules/drill.toml`, schema `drill.schema.json`)
Formations: `Column`, `Line`, `Skirmish`. Training: `Raw`, `Trained`, `Veteran`. Qualitative sources: units are "wildly uneven in training" (CWV p030 section 1.3); transitions take time and a well-trained regiment changes formation faster (CWV p511 section 4.5). All numbers are placeholders (CT-004-5).
- `[[training]] id, transition_pm`: raw 1300, trained 1000, veteran 750.
- `[[transition]] from, to, base_ticks`: column to line 1200, line to column 1000, line to skirmish 1500, skirmish to line 1500, column to skirmish 1800, skirmish to column 1200 (other pairs invalid). Duration `T = ceil(base_ticks * transition_pm / 1000)` (integer ceiling).
- `[[formation]] id, cols (see below), file_width_mm, rank_gap_mm, deployed_pm, reserve_offset_mm`: line `ranks = 2`, file width 560, rank gap 900; column `cols = 8`, file width 560, rank gap 900; skirmish deployed_pm 100, interval 3500 mm, reserve column 8 wide placed `reserve_offset_mm = 100000` behind the skirmish rank. (Two-rank line and the interval are remembered drill practice, unverified.)
- Globals: `closeup_interval_ticks = 40`, `settle_tol_mm = 1000`, `settle_budget_ticks = 600`.
Schema (`drill.schema.json`): `transition[]` (`from != to`, `base_ticks` int>=1), `training[]` (three unique ids), `formation[]` (one per formation; `ranks`/`cols` int 1..64, `deployed_pm` int 1..1000), all with provenance. `terrain.schema.json` mirrors section 5 (unique ascending `id`, `_pm` ints >= 0, `blocks_movement` bool).

## 7. Regiments and the drill state machine
`RegimentId(u16)`, assigned ascending from 0. `Regiment { id, team: u8, training, state: RegimentState, anchor: Vec2Fx, heading: Angle, order: Order, pending: Option<Order>, roster: Vec<AgentId>, dirty: bool, slots: Vec<SlotLocal> }`; `RegimentState = Steady(Formation) | Forming { from, to, ticks_left: u32 }`; `Order = Hold | Move { to: Vec2Fx, gait: Gait } | Form(Formation) | Face(Angle)`.
- `issue_order(id, Order)` takes effect at the start of the next step. While `Forming`, `Hold` applies immediately; any other order replaces `pending` and starts when the transition ends.
- `Form(f)` from `Steady(g)`, `f != g`: compute target slots for `f` at the current anchor and heading (section 8), run `assign_slots`, enter `Forming { from: g, to: f, ticks_left: T }`. The anchor does not move while forming. Soldiers steer to their new slots at the double-quick gait. At `ticks_left == 0` the state becomes `Steady(f)`. `Form(g)` while already `Steady(g)` is a no-op.
- `Move`: allowed in `Steady` only. Anchor speed `A = min over roster of soldier_cap(s)` (section 9, regiment gait, no catch-up), additionally multiplied by `(1000 - line_disorder_pm(terrain at s)) / 1000` while `Steady(Line)`. The anchor halts (no move, no turn) on a tick where `max slot error > max_lag_mm`. Otherwise it first turns toward the target at most `turn_per_tick = Angle::from_degrees_int(1)` raw per tick (placeholder, CT-004-3; 20 degrees/s) then moves `min(A / TICK_HZ, distance)` along the heading. On arrival the order becomes `Hold`.
- `Face(a)`: turns the heading at `turn_per_tick`; soldiers follow their slots (a wheel emerges from slot geometry).
- `can_fire(&Regiment) -> bool` is true iff state is `Steady(Line)` or `Steady(Skirmish)` (consumed by SPEC-005). Cohesion and morale are out of scope (a later spec).

## 8. Slots and assignment
Local frame: `forward = Vec2Fx::from_angle(heading)`, `right = Vec2Fx::from_angle(heading - Angle::QUARTER)`, `world = anchor + right * u + forward * v`. `SlotLocal { u: Fx, v: Fx }`.
`slot_layout(formation, n, &DrillTable) -> Vec<SlotLocal>` in canonical order, rank-major, rank 0 at the front:
- block of `n` men with `cols` columns (`cols = ceil(n / ranks)` for line; the table value for column and the skirmish reserve), `rank = k / cols`, `file = k % cols`, `u = (2*file - (cols_in_rank - 1)) * file_width / 2` (centred; `cols_in_rank` is `cols` except the last rank), `v = -rank * rank_gap`.
- skirmish: the first `ceil(n * deployed_pm / 1000)` slots are a single rank at interval spacing, centred, `v = 0`; the rest are a reserve column block starting `v = -reserve_offset_mm`.
`assign_slots(positions: &[(AgentId, Vec2Fx)], layout: &[SlotLocal], anchor, heading) -> Vec<(AgentId, usize)>`:
1. Project each soldier into the new frame (`u = dot(p - anchor, right)`, `v = dot(p - anchor, forward)`, raw `i64` keys).
2. Sort soldiers by `(v descending, u ascending, index ascending)`.
3. Cut the sorted list into groups whose sizes equal the slot count of each rank in `layout` (front first), sort each group by `(u ascending, index ascending)`, and give the j-th soldier of group r the j-th slot of rank r.
The result is a permutation, independent of the input order. The roster is `Vec<AgentId>` ascending by index. A soldier that becomes `F_DISABLED` or `F_WOUNDED`, or is despawned, leaves the roster and sets `dirty`. When `dirty` and `tick % closeup_interval_ticks == 0` the layout is regenerated for the new `n` and slots reassigned (the line closes up).

## 9. Steering and avoidance (per tick, two phases)
`soldier_cap(s) = speed(gait) * terrain.speed_pm/1000 * fatigue_speed_pm(fatigue)/1000`, zero for `F_DISABLED`/`F_WOUNDED`. A soldier whose slot error exceeds `catchup_mm` and whose fatigue is below `catchup_fatigue_limit_ppm` uses `max(gait, double_quick)`.
Phase A (read-only, frozen snapshot, grid rebuilt at tick start): for each alive soldier `desired_velocity(id)`:
`seek = direction(slot - pos) * min(cap, distance * TICK_HZ)`; `push = sum over neighbours n with d < personal_space of unit(pos - n) * (personal_space - d) / personal_space * avoid_gain`, neighbours from `query_radius(pos, personal_space)` in its `(distance_sq, index)` order, coincident pairs use direction `Vec2Fx::from_angle(Angle::from_raw(index * 0x9E37_79B9))` (wrapping `u32`); `push` length is clamped to `cap / 2`; `desired = clamp_length(seek + push, cap)`.
Phase B (ascending id): `vel += clamp_length(desired - vel, accel / TICK_HZ)`; `pos += vel / TICK_HZ` (terrain block rule, section 5); `facing = regiment heading`; update fatigue (section 4). If `distance(slot) <= |vel|/TICK_HZ` and `push == 0`, snap to the slot with `vel = 0`.
Men at design spacing exert no push (`personal_space_mm = 500` is below 560 and 900). Pathing is straight line plus slot steering only; no general path-finding.

## 10. Calibration targets
CT-004-1 gait speeds against the manuals (second source required). CT-004-2 fatigue (section 4 targets). CT-004-3 acceleration, lag, avoidance, turn rate (no page figure; set by playtest review). CT-004-4 woods: a line of battle moves at most 50 percent of its open-field rate (qualitative, CWV p511 section 4.4). CT-004-5 transitions: veteran/raw ratio 0.4 to 0.8, raw column to line 60 to 120 s (placeholder band). Promotion beyond `unverified` needs `second_source`.

## 11. `BattleState`, hash and public API
`BattleState { world: WorldState, regiments: Vec<Regiment>, loco: Vec<LocoExt> (indexed by slot index), terrain: TerrainGrid, grid: SpatialGrid, tables: Tables }`, `LocoExt { fatigue: u32, slot: SlotLocal, catchup: bool }`.
`battle_hash(&BattleState) -> u64`: `StateHasher`; `write_world`, then `u32` regiment count and per regiment (ascending id): `id u16, team u8, training u8, state tag u8, from u8, to u8, ticks_left u32, anchor.x i64, anchor.y i64, heading u32, order tag u8, order target x,y i64, gait u8, dirty u8, roster_len u32, roster slot indices u32`; then `u32` slot_count and per slot ascending: `fatigue u32, slot.u i64, slot.v i64, catchup u8`; then three reserved `u32 = 0` blocks (fire, smoke, casualty log) that SPEC-005 fills.
Public API (exact): `TICK_HZ`, `fx_ratio`, `STREAM_BATTLE_SETUP`, flag consts; `Gait`, `Formation`, `Training`, `RegimentId`, `RegimentState`, `Order`, `Regiment`, `RegimentSpec { team, count, training, formation, anchor, heading }`, `SlotLocal`, `TerrainGrid::{from_rows, class_at}`, `LocomotionTable`, `DrillTable`, `TerrainTable` each with `from_toml_str(&str) -> Result<Self, DataError>`, `Tables::load_dir(&str)`, `DataError` (enum: `Parse`, `Float`, `UnknownKey`, `Provenance`, `Range`), `fatigue_speed_pm`, `slot_layout`, `assign_slots`, `transition_ticks(from, to, training, &DrillTable) -> u32`, `BattleState::{new, spawn_regiment, issue_order, step_movement, desired_velocity, can_fire, hash}`, `encode_order`/`decode_order`, `battle_hash`. `step_movement` runs: rebuild grid; regiments ascending (orders, drill state, anchor, close-up); phase A; phase B. Order wire format (replay input payload; the whole tag space and the actor ids are in REGISTRY section 2): `u8 tag (1 Hold, 2 Move, 3 Form, 4 Face), u16 regiment, then Move: i64 x, i64 y, u8 gait; Form: u8 formation; Face: u32 raw angle`.
`spawn_regiment` places men on their formation slots; ids ascend with slot order.

## 12. Headless scenarios (library functions in `battle_rules::scenarios`, also run by `hd-sim battle --scenario NAME --ticks N --seed S`)
- `march_single`: 1 regiment, 400 Trained, Column, anchor (0,0), heading 0, open terrain, `Move` to (200,0) at quick.
- `deploy_pair`: 2 regiments (Raw, Veteran), 400 each, Column, spaced 300 m apart in y, both `Form(Line)` at tick 0.
- `terrain_strip`: `march_single` over a 40 m wide woods strip centred at x = 100.
- `march_large`: 50 regiments of 400 (20,000 soldiers), Column, 100 m spacing, all `Move` 300 m at quick, seed 42.

## 13. Acceptance tests (tests first; thresholds are not edited to pass)
- **T4-01 loader:** `data/rules/*.toml` load and validate. A TOML float, an unknown key, a placeholder row with no `calibration_target` and a non-placeholder row with no `sources` each return the named `DataError`. `python3 tools/validate_data.py` validates the same files against the schemas.
- **T4-02 fx_ratio:** `fx_ratio(1067,1000)`, `(-1,3)`, `(1,0)` match listed raw values.
- **T4-03 march:** `march_single` arrival tick is within 1 percent of `ceil(distance * 20 / speed)` for each of the four gaits (distance 200 m) and the maximum slot error never exceeds `max_lag_mm`. The reference values come from an independent script `qc/fixtures/ref_march.py` using `fractions.Fraction`; the implementer runs it once and commits its output.
- **T4-04 terrain:** in `terrain_strip` the ticks spent inside the strip equal `strip_length / (speed * 0.6) * 20` within 2 percent.
- **T4-05 fatigue:** CT-004-2 targets hold within 20 percent; fatigue never leaves `[0, 1_000_000]`; `fatigue_speed_pm` equals the curve at every breakpoint and is non-increasing (proptest).
- **T4-06 accel:** per tick `|vel_new - vel_old| <= accel / 20` plus one ulp, for every soldier over `deploy_pair`.
- **T4-07 drill timing:** for all six transitions and three training levels the number of ticks in `Forming` equals `transition_ticks`; `can_fire` is false while `Forming` and in `Column`.
- **T4-08 settle:** in `deploy_pair`, by tick `T + settle_budget_ticks` every soldier is within `settle_tol_mm` of its slot, for both regiments.
- **T4-09 slots:** `assign_slots` returns a permutation; it is identical for any permutation of the input list (proptest); within each rank `u` is non-decreasing along slot order; `slot_layout` dimensions match the table (width of a 400-man line is `200 * 560 mm`).
- **T4-10 closeup:** after despawning 40 men from a 400-man line, on the next closeup tick the layout has 360 slots and every remaining soldier has a distinct slot.
- **T4-11 avoidance:** two soldiers spawned coincident separate to at least `personal_space_mm` within 40 ticks; two 100-man files marching through each other keep a minimum pair distance of at least 0.8 of `personal_space_mm` after settling; `desired_velocity` called for all soldiers in descending id order equals the ascending-order results (purity of phase A).
- **T4-12 determinism:** `march_single` (tick 4000, seed 1), `deploy_pair` (tick 3000, seed 2), `march_large` (tick 600, seed 42) hash to literals recorded once on Linux and checked in `qc/golden-replays/expected.json`; same-process and two-process runs agree; `hd-sim verify` passes on the committed replays on all three CI platforms.
- **T4-13 perf (ignored test, prints numbers):** `march_large` movement step.

## 14. Performance budget (hypothesis, to be measured)
Baseline: toy scenario mean 11.8 ms, p99 17 ms per tick at 20,000 agents, single thread (`docs/cycle-reports/CYCLE-00-baseline.md`). Hypothesis H4: `step_movement` for 20,000 soldiers is at most 10 ms mean and 18 ms p99 in release, single thread, leaving room for SPEC-005 inside the 50 ms tick. The research text suggests 4 to 6 ms on 8 cores (CWV p453, `[Estimate]`); this project is single-threaded, so that is context only. Record measurements in the cycle report.

## 15. Done when
fmt, clippy `-D warnings`, `cargo test --workspace` (also `--release`) and all T4 tests pass; the tools scans pass; schemas, data and golden hashes committed and verified on three platforms; public items documented; no floating point outside `#[cfg(test)]`; T4-13 numbers recorded.

## Amendment A1 (from T-016 implementation, 2026-11-04)
Open inconsistency to settle in calibration ticket T-024: calibration target CT-004-4 says woods slow a line to 50 percent of its open-ground speed or less, but the `woods` terrain row is `speed_pm = 600`. The data keep the 600 value (a placeholder); the calibration run decides which of the two is changed. `Gait` is defined in `crates/battle-rules/src/locomotion.rs` and T-018 must reuse it. `TerrainGrid::from_rows` takes the `TerrainTable` so it can reject unknown legend classes and record which classes block movement. `fatigue_step` takes the applied speed as `Fx`; zero means the displacement was blocked. Blocking is applied per axis, x then y. `assign_slots` returns pairs in ascending slot index and surplus soldiers receive no slot; `transition_ticks` is 0 when `from == to`; `regiment_roster` skips disabled and wounded soldiers.

## Amendment A9 (from T-019 implementation, 2026-11-04)
- **Snap and acceleration.** The snap of section 9 sets the velocity to zero, which breaks T4-06 whenever a soldier arrives at marching speed. The snap therefore also requires `|vel| <= accel / TICK_HZ` at the start of the tick, so a snap never changes the velocity by more than the acceleration limit. The distance test uses the position after the move.
- **T4-03 expected value.** `ceil(distance * 20 / speed)` ignores the fatigue speed curve, which slows a 200 m march by 0.2 percent at walk, 0.8 at quick, 2.5 at double-quick and 66 at run. The expected arrival tick is the `model_ticks` value of `qc/fixtures/ref_march.csv` (exact rational model of the anchor at the pace of its most tired soldier); the test requires the arrival tick within 1 percent of it, and never earlier than the ideal tick. The ideal tick is printed in the cycle report for comparison. T4-04 likewise compares the ticks the anchor spends inside the strip with `strip_length / (speed * 0.6) * 20` within 2 percent and with the model value.
- **Gait of a soldier.** `Forming`: double-quick (section 7). `Move`: the ordered gait. Any other order: quick. Catch-up raises the gait to at least double-quick. The gait chosen also selects the fatigue drain.
- **Anchor details.** The anchor arrives when its heading equals the bearing of the target and the distance is at most one step; it is then set exactly on the target and the order becomes `Hold`. `Face` becomes `Hold` when the heading reaches the ordered angle. A coincident pair uses the angle from the index of the soldier being steered. A soldier with no regiment, or disabled or wounded, has desired velocity zero. The speed cap of a soldier that is `Steady(Line)` is multiplied by the line-disorder factor only inside the anchor-speed minimum, not in its own cap.
- **Scenario geometry.** `deploy_pair` anchors are `(0, 0)` and `(0, 300)` heading 0; `march_large` anchors are `(0, 100 i)`, targets `(300, 100 i)`; `terrain_strip` grid has 8 m cells, woods in cells 10 to 14 of one row. `march_single_at(gait)` is `march_single` at another gait. The scenarios issue their orders before tick 0 and use no random draws, so the seed enters the hash only through the world header.
- **T4-11 geometry.** The two files are single-file columns (`cols = 1` in the drill table of the test), 100 men each, marching in opposite directions on one line and ending interleaved half a rank gap (450 mm) apart; the distance is measured over all pairs after both regiments have held for 400 ticks.
- **T4-12.** `hd-sim battle` and its replay goldens move to a later ticket; the three hash literals are recorded in `crates/battle-rules/tests/t4_12_determinism.rs` and checked there in one process and in a child process.

## Amendment A2 (from T-071, SPEC-007)
Section 11 describes the hash and the input tags as they stand with every optional layer off. This amendment fixes how optional layers extend them; the shared values are owned by `docs/specs/REGISTRY.md` and the registry wins where this text differs.
- **Tag space.** The order wire format of section 11 covers tags `0x01` to `0x04` only. The whole payload tag space and the actor space are REGISTRY sections 2.1 and 2.2: tag `0x05` is the SPEC-005 fire mode; `0x10` Dispatch, `0x12` Options and `0x13` Script belong to SPEC-007 (`0x11` is reserved, `0x14` to `0x1F` are its growth range). A Dispatch carries the unchanged `encode_order` bytes of section 11 after its own header, so `encode_order` and `decode_order` do not change.
- **Optional hash blocks.** `battle_hash` is the section 11 stream followed by the three SPEC-005 blocks (fire, smoke, casualty log). A layer that adds state to `BattleState` through an `Option` field appends one block after those, in the fixed order of REGISTRY section 5 (morale, command, environment). A block is written only when its option is `Some`, and it begins with its own `u32` magic: `0x4D4F_5231` morale, `0x434D_4431` command ("CMD1"), `0x454E_5631` environment. With every option `None` nothing is written, so the stream, and every golden recorded before the layer existed, is unchanged.
- **Direct-order refusal.** When `cmd` is `Some`, a replay input with tag `0x01` to `0x05` from a seat actor (`0x00`, `0x01`, `0x80`, `0x81`) is refused with `InputError::DirectOrder` and changes nothing, unless that seat's `instant_orders` option is set. Script actors (`0xFD`, `0xFE`, `0xFF`) and orders the engine issues itself are never refused. The refusal sits at the input boundary (`apply_command_input`); `issue_order` itself is unchanged. With `cmd` `None` every input is applied as before.
- **Tick order.** `step_command` is step TK-03 of REGISTRY section 6, after the boundary and weather steps and before movement. No SPEC-005 text changes.

## Amendment A3 (from T-141, SPEC-014)
Section 5 gains four terrain classes for the battlefield pipeline (SPEC-014 section 4.3). Classes 0 to 7 are unchanged.
- **Rows.** `terrain.toml` appends, ids ascending, all placeholders with calibration target CT-014-3 (CT-004-4 still bounds the woods rate):

| id | name | speed_pm | effort_pm | line_disorder_pm | cover_pm | blocks |
|---:|---|---:|---:|---:|---:|---|
| 8 | crop | 900 | 1100 | 120 | 50 | no |
| 9 | rocky | 500 | 1600 | 450 | 400 | no |
| 10 | yard | 800 | 1100 | 200 | 100 | no |
| 11 | building | 0 | 1000 | 0 | 0 | yes |

- **Blocking.** `blocks_movement` is true for classes 7 and 11 only; the loader rule that a passable class needs a positive `speed_pm` is unchanged.
- **Provenance prefix.** A row added by a later spec carries that spec's calibration prefix (ADR 0011), so rows 8 to 11 carry `CT-014-3` while rows 0 to 7 keep `CT-004-4`.
- **Slope.** Ground slope (`slope_pm`, SPEC-014 section 5) is data only; no movement, fatigue or fire rule reads it yet.
- **Grids and sidecars.** A scenario `legend` may name classes 8 to 11. No class has a hash of its own, so no recorded golden changes. Matching `environment.toml` `[[retain]]` rows belong to the SPEC-009 data ticket; `acoustics.toml` `[[ground]]` has no row for a class that adds no foliage loss, so none is added.

## Amendment A4 (from T-083, SPEC-008)
Sections 7 and 11 describe a regiment whose soldiers are always in the agent store. SPEC-008 lets a regiment be an aggregate in class 2, so `Regiment` gains five fields. None is hashed by `battle_hash`; the `Battle::hash` block of SPEC-008 section 9 writes them, so no golden recorded without the `Battle` wrapper changes.
- **Fields.** `status: RegimentStatus` (`NotArrived`, `Present`, `Departed`; `spawn_regiment` gives `Present`), `class: FidelityClass` (0 at spawn), `since: u64` (the tick of the last class change, 0 at spawn), `aggregate: Aggregate` (SPEC-008 section 5; all zero unless the regiment was condensed or carries wounded men from a hand-over) and `acct: RoundsAccount` (SPEC-005 amendment A7). `BattleState` gains `soldier_serial: u32` (REGISTRY section 16.1), 0 at construction.
- **Aggregate regiment.** A regiment of class 2 has an empty `roster` and an empty `slots`, `dirty` false; `anchor`, `heading`, `state`, `order` and `pending` persist and the drill countdown of section 7 keeps running. With an empty roster the `Move` rule of section 7 (a minimum over the roster) changes nothing, so the anchor stays put until SPEC-008 section 6.3 moves it.
- **Hand-over entry points.** `condense(&mut BattleState, RegimentId) -> bool`, `materialise(&mut BattleState, RegimentId, FidelityClass) -> bool` and `hand_over(&mut BattleState, RegimentId, FidelityClass) -> bool` (SPEC-008 section 5). Each returns `false` and changes nothing for an unknown regiment, a refused request (a pooled projectile still belongs to the regiment, or the regiment is not in the class the call leaves) or no change of class; each sets `class` and `since = world.tick` on success. Changing between class 0 and class 1 alters only `class` and `since`.
- **Materialised soldiers.** They stand on the slots of `state.target_formation()` for `n = fit + disabled count`, at the anchor and heading, as `spawn_regiment` places them, at 100 hit points, facing the heading, standing (posture flags are not kept by an aggregate), `F_GUN` set when the regiment's weapon is a field gun. The first `fit` of them (ascending slot) are fit and form the roster; the last `n - fit` take the `disabled` entries in ascending `until`, carry `F_DISABLED`, 60 hit points and `disabled_until = until`, and stay off the roster until released; the regiment is then `dirty`. Wounded men are not recreated: `aggregate.wounded` keeps them, and `killed` likewise.

## Amendment A5 (from T-092, SPEC-009)
Section 4 gains the heat multipliers of SPEC-009 section 7; REGISTRY section 6.5 owns the composition.
- **Moving.** `fatigue += drain_ppm_per_tick(gait) * effort_pm * drain_pm / 10^6` (one floor over the whole product); at `drain_pm = 1000` this equals `drain_ppm_per_tick * effort_pm / 1000`.
- **Resting.** `fatigue -= rest_recover_ppm_per_tick * rest_pm / 1000` (floor, saturating at 0); at `rest_pm = 1000` this equals the plain recovery.
- **Code.** `fatigue_step_scaled(fatigue, gait, effort, applied_speed, table, drain_pm, rest_pm)` is the general form; `fatigue_step` is it with both multipliers 1000, so every caller and golden with `env = None` is unchanged. The movement step passes the multipliers of `BattleState.env` (1000 and 1000 with `env = None`).

## Amendment A6 (from T-095, SPEC-009)
Section 9 gains the peek offset of SPEC-009 section 6; with `env` `None` every offset is 0 and nothing changes.
- **Fields.** `LocoExt` gains `peek_mm: u16`, `blind: bool` and `fired: bool` (all zero or false at spawn and when a soldier is materialised); `Regiment` gains `peek_mm: u16` (the largest `peek_mm` of its soldiers as of the last peek update, 0 after a `Move`) and `view_blocked: bool` (false at spawn). The `LocoExt` hash block of section 11 does not write them; the environment block of SPEC-009 section 9 does.
- **Steering target.** The slot a soldier steers to, and the slot it snaps onto, is `anchor + right * u + forward * (v + peek_mm)`, `peek_mm` in millimetres converted by `fx_ratio(peek_mm, 1000)`; slot assignment, the `catchup_mm` test and the close-up are unchanged and use the plain slot.
- **Reset.** When a `Move` becomes the standing order of a regiment (at once from `Steady`, or when a pending `Move` starts after a transition), the `peek_mm` of every soldier of its roster and the regiment's `peek_mm` are set to 0.
