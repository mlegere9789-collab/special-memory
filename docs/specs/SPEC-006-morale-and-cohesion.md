# SPEC-006: battle-rules III, morale and cohesion

> Shared namespaces (input tags and actors, random streams, flags, hash blocks, tick order, CT ids, amendment labels, ticket ids) are owned by [REGISTRY.md](REGISTRY.md); conflicts and their resolutions are in [CONFLICTS.md](CONFLICTS.md).

Crate: `crates/battle-rules` (adds module `morale`; data `data/rules/morale.toml`, schema `morale.schema.json`). Same conventions, data units and provenance as SPEC-004 sections 1 and 2; `Casualty`, `FireExt`, `ObstacleLayer` come from SPEC-005. Integers only; ascending-id iteration; no `HashMap`.
Source key: **CWV pNNN** = PDF page NNN of the research volume; pages carry no verification tag unless stated. Every number below is a `placeholder` with a calibration target (CT-006-61 to CT-006-68, section 10). The pages give only qualitative claims: morale is a regiment property built from casualties, officer losses, supply, fatigue, terrain, neighbours breaking, flanking or enfilade and a respected commander; veterans waver too but recover faster; the player sees symptoms and a coarse indicator, never a hit-point bar (CWV p511 4.6). Raw regiments were "wildly uneven" and commands "came apart through confusion rather than casualties" (CWV p030 1.3); flanking decided attacks and rout phenomena are documented (CWV p065 9.3, 9.4); Wheatfield regiments took 30 to 50 percent losses with flank-fire panic and charges "worked via morale rather than melee" (CWV p085, p082, no tag); the 1st Minnesota held at about 82 percent casualties (CWV p082, no tag).

## 1. Purpose and scope
A regiment-level model deciding whether a regiment fires, holds, retires or runs, driven by quantities the core already computes. It adds no hit chances (casualties come from F0 fire, SPEC-005 section 7) and runs identically in every fidelity class (ADR 0002). Out of scope: melee, command friction, campaign morale, rendering of symptoms.

## 2. Definitions
- **Cohesion** `C`: `u32` ppm, 0 to 1,000,000, hidden. **Ceiling** `Tc`: the level `C` relaxes towards. **Cumulative loss** `L`: ppm of ceiling permanently removed by losses.
- **Slow period**: `SLOW_PERIOD_TICKS = 20` (1 s). Casualties act every tick; everything else on ticks with `tick % 20 == 0`.
- **MoraleKind**: `Steady, Wavering, FallingBack, Broken, Rallied`. **Indicator** (interface): `Firm, Uneasy, Shaken, Retiring, Routed, Rallying`.
- **Absent**: a soldier who left the ranks (`F_SKULK`), alive but outside the roster. `S0`: roster size when morale was enabled.
- Constants: `STREAM_MORALE_BASE = 0x5000_0000` (regiment stream `STREAM_MORALE_BASE + id`); flags `F_LOOKBACK = 1<<5`, `F_SKULK = 1<<6`. `lerp(c, c_hi, c_lo, v_hi, v_lo)` = `v_hi + (v_lo - v_hi) * (c_hi - c) / (c_hi - c_lo)` in `i64`, `c` clamped to `[c_lo, c_hi]`, division truncating toward zero.

## 3. Data (`data/rules/morale.toml`, `schema = "morale/1"`)
`[[training]]` rows (ids `raw, trained, veteran`):

| id | base_ppm | cap_pm | up_pm | rally_pm | return_pm | nbr_pm |
|---|---:|---:|---:|---:|---:|---:|
| raw | 800000 | 355 | 30 | 8 | 500 | 1500 |
| trained | 880000 | 455 | 40 | 14 | 650 | 1000 |
| veteran | 950000 | 535 | 55 | 25 | 800 | 700 |

`cap_pm`: casualty capacity; `up_pm`, `rally_pm`: permille of the gap closed per second; `return_pm`: absentees returning on rally; `nbr_pm`: neighbour-shock multiplier.
`[globals]` (ppm unless named): `waver 600000`, `steady 680000`, `fall 350000`, `fall_exit 450000`, `break 150000`, `rally 400000`, `nerve_spread 50000`, `delay 400000`, `down_pm 80`, `perm_pm 600`, `w_killed_pm 1000`, `w_wounded_pm 1000`, `w_disabled_pm 700`, `w_officer_pm 6000`, `w_colonel_pm 10000`, `calm_pm 250`, `broken_ceiling 500000`, `repeat_pen 60000` (times `min(broken_count,3)`), `fatigue_pen 150000`, `fatigue_knee 400000`, `ammo_pen 100000`, `ammo_knee_pm 250`, `supply_pen 60000`, `flank_pen 350000`, `flank_f_pm 600`, `enfilade_f_pm 1000`, `rear_f_pm 1000`, `charge_pen 120000`, `disorder_pen 60000`, `cover_bonus 80000`, `wall_bonus 60000`, `leader_bonus 60000`, `nbr_shock 90000`, `nbr_fall_pm 500`, `enemy_boost 40000`, `rally_leader_pm 1000`, `cause_min 20000`; lengths in mm: `nbr_radius 150000`, `threat_range 400000`, `pressure_radius 120000`, `charge_range 40000`, `leader_radius 60000`, `wall_reach 4000`, `skulk_rear 60000`, `flight_distance 300000`; ticks: `min_fall 200`, `min_broken 400`, `min_rallied 400`, `officerless_rally 3600`, `dwell 60`.
`[[symptom]]` rows by state (`fire_pm`, `shake_pm`, `look_pm` per second, `skulk_ppm` per second): Steady 1000/1000/10/0; Wavering ramps (section 5.6); FallingBack 0/1500/400/1500; Broken 0/1500/800/6000; Rallied 800/1250/120/0. Wavering: fire 1000 to 550 and shake 1000 to 1500 as `C` goes `steady` to `fall`; look 80 to 300; skulk 0 to 400 as `C` goes 500000 to `fall`.
Schema: every field an integer in range (`*_pm` 0..=10000, `*_ppm` 0..=1000000, thresholds strictly ordered `break < fall < rally < fall_exit < waver < steady`). The loader returns `DataError::Range` otherwise.

## 4. State
`MoraleState { regs: Vec<RegimentMorale>, leaders: Vec<Leader>, cursor: usize, events: Vec<MoraleEvent>, rally_points: Vec<Option<Vec2Fx>> (by team) }`, `BattleState.morale: Option<MoraleState>`.
`RegimentMorale { kind, state_ticks: u32, c: u32, loss: u32, window: u32, nerve_offset: i32, broken_count: u8, s0: u16, rounds_full: u32, supply_pm: u16 (default 1000), leader_pm: u16, officers: Vec<AgentId> (ascending), colonel: AgentId, absent: Vec<AgentId> (ascending), rng: Rng, indicator, dwell: u16, shock_unit: u32 }`.
`Leader { id: u16, team: u8, pos: Vec2Fx, prestige_pm: u16 (0..=1000), alive: bool }`, ascending id; set by the scenario until a command spec supplies them. `MoraleEvent { kind: Broke | Fell, regiment: RegimentId, pos: Vec2Fx }`.
`enable_morale(&mut self)` (once, after spawns and `equip_regiment`) per regiment: `s0 = roster.len()`; `shock_unit = ceil(10^9 / (s0 * cap_pm))` (`s0` at least 1); `c = base`; `n_off = max(2, s0 / 40)`; officers are roster entries at index `(2k + 1) * s0 / (2 * n_off)` for `k = 0..n_off`, the colonel is `k = 0`; the stream is `RngStreams::stream(StreamId(STREAM_MORALE_BASE + id))`; `nerve_offset = range_i32(-nerve_spread, nerve_spread)` from that stream; `rounds_full` is the sum of `FireExt.rounds`.

## 5. Algorithm
`step_morale` is step TK-11 of the tick order in REGISTRY section 6: after casualties are applied (TK-10) and before smoke and environment work (TK-12). Regiments are processed ascending. Slow-period work reads a frozen snapshot first and writes after.

**5.1 Casualties (every tick).** For each log entry from `cursor` to the end (ascending, as appended): skip if `unit` has no morale or the target is in `absent`. Weight `w` = `w_colonel_pm` if the target is the colonel, `w_officer_pm` if an officer (then remove it from `officers`), else the kind weight. `shock = shock_unit * w / 1000`; with leader, `shock = shock * (1000 - calm_pm * leader_pm / 1000) / 1000`. Then `c = c.saturating_sub(shock)`, `loss += shock * perm_pm / 1000` (cap 10^6), `window += shock`. Set `cursor`; `state_ticks += 1`. A log entry whose `target_index` is `u32::MAX` is an aggregate outcome (ADR 0010 D-07): it carries the kind weight only, and the checks that need a soldier (absent list, colonel, officers) are skipped.

**5.2 Inputs (slow period, per regiment).**
- Mean fatigue `F` of the roster (`loco[].fatigue`, floor): `fatigue_pen * max(0, F - fatigue_knee) / (10^6 - fatigue_knee)`.
- Ammunition `r = 1000 * sum(rounds) / rounds_full` (0 if `rounds_full` is 0): `ammo_pen * max(0, ammo_knee_pm - r) / ammo_knee_pm`. Supply: `supply_pen * (1000 - supply_pm) / 1000`.
- Terrain at the anchor: `+ cover_bonus * cover_pm / 1000 - disorder_pen * line_disorder_pm / 1000`; `+ wall_bonus` if an obstacle of height at least 600 mm has its nearest point within `wall_reach` of the anchor and ahead of it (`dot(p - anchor, forward) >= 0`).
- Exposure: for each enemy regiment with anchor within `threat_range`, weight `wt = roster_len` (a quarter of it if it cannot fire) and direction `d = enemy - anchor`, `f = dot(d, forward)`, `r = abs(dot(d, right))` (raw `i128`). Class: **front** if `f > 0 and r*1000 < 1732*f`; **rear** if `f < 0 and r < -f`; **enfilade** if not front or rear and `abs(f)*1000 <= 364*r`; else **flank**. Coefficients `f_pm`: front 0, flank `flank_f_pm`, enfilade `enfilade_f_pm`, rear `rear_f_pm`. `share = sum(wt * f_pm) / sum(wt)` (0 with no enemy); `flank_pen_applied = flank_pen * share / 1000`. Also record the dominant class for reporting.
- Charge: an enemy within `charge_range` in `Steady(Line|Column)` with `Order::Move` and `dot(enemy_forward, anchor - enemy_anchor) > 0` gives `charge_pen * min(1000, 1000 * enemy_len / own_len) / 1000`; the largest applies.
- Pressure: any enemy anchor within `pressure_radius`. Leader: highest `prestige_pm` among living same-team leaders within `leader_radius`, else 0; `leader_bonus * leader_pm / 1000`.

**5.3 Ceiling and relaxation.** `T = base - repeat_pen * min(broken_count, 3) - penalties + bonuses`, clamped to `[0, 10^6]`; `Tc = max(0, T - loss)`; in `Broken`, `Tc = min(Tc, broken_ceiling)`. Officer factor `o = 500 + 500 * officers_alive / officers_total` (permille). `gap = Tc - c` (`i64`). If `gap < 0`: `c += gap * down_pm / 1000`. If `gap > 0` and not (pressure and `kind` in FallingBack or Broken): rate = `up_pm * o / 1000` (Steady, Wavering, Rallied), or `rally_pm * o / 1000 * (1000 + rally_leader_pm * leader_pm / 1000) / 1000` (FallingBack, Broken); `c += gap * rate / 1000`. Then `window -= window / 8`.

**5.4 Rolls (fixed draw count).** Per regiment per slow period exactly one `u32` from its stream decides `hit = roll % 10^6 < delay` (always drawn), then one `u64` per roster member in ascending order (section 5.6). Absentees returning (5.5) draw one `u32` each, ascending, in that period after the soldier draws.

**5.5 Transitions.** Thresholds are `X + nerve_offset`. One transition at most per period.
- Steady: `c < waver` to Wavering. Wavering: `c >= steady` to Steady; else `c < fall` and `hit` to FallingBack.
- FallingBack: `c < break` and `hit` to Broken; else `c >= fall_exit`, `state_ticks >= min_fall`, no pressure to Rallied.
- Broken: `c >= rally`, no pressure, `state_ticks >= min_broken`, and (living officers, or `leader_pm > 0`, or `state_ticks >= officerless_rally`) to Rallied.
- Rallied: `c < fall` and `hit` to FallingBack; else after `min_rallied`: Steady if `c >= steady`, Wavering if `c >= fall`.
Entry actions: `state_ticks = 0`. FallingBack and Broken: record `MoraleEvent` (Fell, Broke), cancel `pending`, cap a `Forming` countdown at 20 ticks, set `order = Move { to: retreat, gait: Quick | Run }` where `retreat` is the team rally point or `anchor + unit(anchor - threat_centre) * flight_distance` (threat centre: nearest enemy anchor, else behind the heading), recomputed every 100 ticks while in state. Entering Broken from any state adds `broken_count` (cap 255). Rallied: `order = Hold`; each absentee in ascending order returns if `roll % 1000 < return_pm`; a returner clears `F_SKULK`, is inserted into the roster in ascending order, and sets `dirty`.
Neighbour events from the previous period apply now, before transitions, in recorded order (all additive, so order cannot matter): for each friendly regiment within `nbr_radius` of the event, `c -= nbr_shock * (R - d) / R * nbr_pm / 1000` (times `nbr_fall_pm / 1000` for Fell); for each enemy regiment within it, a Broke event adds `enemy_boost * (R - d) / R` to `c`. Clear `events` after applying. Events created in this period wait for the next one (a cascade moves one period per hop).

**5.6 Soldier symptoms.** Per roster soldier, `u64` draw `x`: `look = (x as u32) % 10^6 < look_pm * 1000`, `skulk = (x >> 32) as u32 % 10^6 < skulk_ppm`, using the current state's values. Clear `F_LOOKBACK` on all, then set it where `look`. Officers never skulk. A skulker gets `F_SKULK`, enters `absent`, and its `loco.slot` becomes `(u, -skulk_rear)`; it leaves the roster and `dirty` is set. Absentees killed or despawned are dropped from `absent`.

**5.7 Interface.** Indicator from state: Wavering is `Uneasy` if `c >= 500000` else `Shaken`; others map one to one. It worsens immediately and improves only after `dwell` ticks. `window + loss` and the section 5.2 penalties are ranked as causes (`Losses, Officers, Fatigue, Ammunition, Supply, Flank, Charge, Neighbours, Terrain`; ties by that order; officer losses count in `Officers` as `officers_lost * w_officer_pm * shock_unit / 1000`). `MoraleReport` carries the two largest at least `cause_min`; `c` and `loss` are never public (`#[cfg(test)]` accessors only).

**5.8 Fire interplay (amendments: SPEC-004 A7 and SPEC-005 A11, REGISTRY section 9; the fire-phase gate order is REGISTRY section 6, step TK-06).**
- **SPEC-004 section 7:** `issue_order` returns `false` for a regiment in FallingBack or Broken. `can_fire(&Regiment)` is unchanged; `BattleState::can_fire(id)` is `drill can_fire && kind in {Steady, Wavering, Rallied}`.
- **SPEC-004 section 8 and roster:** `regiment_roster` and `sync_roster` also skip `F_SKULK`.
- **SPEC-005 section 4:** loading progress (`left -= 1`, the `ram` multiplier unchanged) occurs on a tick only if `load_gate(tick, slot_index, fire_pm)`: `let a = (tick + slot_index) * fire_pm; (a + fire_pm) / 1000 > a / 1000` in `u64`. Shooters with `F_SKULK` never fire.
- **SPEC-005 section 3:** `sigma_s` is multiplied by `shake_pm / 1000`, applied last in the chain (after the fatigue and smoke factors of SPEC-005 section 3), each factor floored separately.
Regiments without morale (`morale = None`) behave as before.

## 6. Hash
After SPEC-005's three blocks, only when `morale` is `Some` (block order and magics: REGISTRY section 5): `u32 0x4D4F_5231`, then `u32` regiment count; per regiment ascending: `kind u8, state_ticks u32, c u32, loss u32, window u32, nerve_offset i32, broken_count u8, s0 u16, rounds_full u32, supply_pm u16, leader_pm u16, colonel u32, officer count u32 and indices, absent count u32 and indices, rng 4 x u64, indicator u8, dwell u16`; `u32` leader count and per leader `id u16, team u8, alive u8, x i64, y i64, prestige u16`; `cursor u64`; `u32` event count and per event `kind u8, regiment u16, x i64, y i64`; rally points as `u8 present, x i64, y i64` per team. With `None` nothing is written, so SPEC-004 and SPEC-005 goldens are unchanged. Soldier flags are already hashed by `write_world`.

## 7. Public API (exact)
`MoraleTable::from_toml_str(&str) -> Result<MoraleTable, DataError>` (and `Tables.morale`); `MoraleKind`, `Indicator`, `Cause`, `MoraleReport { indicator, causes: [Option<Cause>; 2], fire_pm: u16, shake_pm: u16, looking: u16, absent: u16, thinning_pm: u16 }` (`thinning_pm` = permille of `S0` that is dead, wounded, disabled or absent), `Leader`, `MoraleEvent`, `MoraleState`; `shock_unit(s0, cap_pm) -> u32`; `classify_exposure(f: i128, r: i128) -> Exposure`; `load_gate(tick: u64, slot_index: u32, fire_pm: u16) -> bool`; `lerp`; `BattleState::{enable_morale, set_leader, set_supply, set_rally_point, step_morale, morale_report(RegimentId) -> Option<MoraleReport>, fire_pm, shake_pm}`; constants above. `hd-sim battle` gains `--morale on|off` (default off).

## 8. Performance (hypothesis H6)
Per-tick work is the casualty scan (about 42 entries per tick, SPEC-005 H5). A slow period is one pass over 20,000 roster entries, 100 by 100 regiment pairs and one obstacle scan per regiment: at most 2.5 ms on slow ticks, 0.2 ms otherwise, release, single thread (T6-15).

## 9. Acceptance tests (tests first; goldens recorded once from the stated reference)
- **T6-01 loader:** `morale.toml` loads and `validate_data.py` agrees; float, unknown key, provenance and threshold-order violations return the named `DataError`.
- **T6-02 shock arithmetic:** `shock_unit(400, 455) == 5495`; killed 5495, disabled 3846, colonel 54950 (script `qc/fixtures/ref_morale.py` with `Fraction`, output committed).
- **T6-03 gate:** `load_gate` is always true at 1000 and never at 0; over any 1000 ticks the true count is `floor((t0 + 1000 + i) * p / 1000) - floor((t0 + i) * p / 1000)` (proptest).
- **T6-04 exposure:** enemies at bearings 0, 59, 61, 90, 100, 105, 115, 134, 136, 180 degrees classify as front, front, flank, enfilade, enfilade, enfilade, flank, flank, rear, rear; checked by an independent script.
- **T6-05 ceiling and relaxation:** each penalty and bonus equals its formula; `T` stays in `[0, 10^6]`; constant inputs give monotone convergence to `Tc` within 1000 ppm without overshoot (proptest).
- **T6-06 states:** `c` pinned inside `(waver, steady)` never changes state in either direction after the first entry; no casualties and neutral inputs for 10,000 ticks leave every regiment Steady; every downward step needs `hit`, so a forced `hit = false` blocks FallingBack indefinitely.
- **T6-07 rally:** Broken stays Broken while an enemy is within `pressure_radius`; with none it rallies after the conditions of 5.5, never before `min_broken`; an officerless, leaderless regiment waits `officerless_rally`.
- **T6-08 cascade:** two regiments breaking in one period shock a third by the sum of their shocks, identical when the event list is applied in reverse (test hook); a hop takes exactly one period.
- **T6-09 streams:** the draw count per period is `1 + roster_len`; removing an unrelated regiment leaves another regiment's first 50 rolls identical.
- **T6-10 interface:** `MoraleReport` exposes neither `c` nor `loss`; the indicator improves only after `dwell` ticks and worsens at once; causes sort by magnitude then enum order; a flanked regiment lists `Flank`, one out of ammunition `Ammunition`.
- **T6-11 fire:** `can_fire` is false in FallingBack and Broken for `Steady(Line)`; `issue_order` returns false there; over 6000 ticks of `cal_line` the shots per soldier scale with `fire_pm` within 3 percent; skulkers never fire; `sigma_s` changes by exactly `shake_pm / 1000`.
- **T6-12 absentees and officers:** skulkers leave the roster and set `dirty`; returners on Rallied are within 3 sigma of `return_pm`; officers never skulk; a killed absentee is dropped; losing the colonel shocks like ten kills.
- **T6-13 calibration:** `run_morale_bands` passes M1 to M8 (section 10).
- **T6-14 hash:** `None` leaves the SPEC-004 and SPEC-005 goldens unchanged; with morale a golden for `cal_attrition(trained, 3 per s)` (tick 1200, seed 1) is recorded once on Linux in `qc/golden-replays/expected.json`; changing any listed field changes the hash; verified on three platforms and across processes.
- **T6-15 perf (ignored test, prints mean, p99, max):** `f0_large` with morale on.

## 10. Calibration targets
`cal_attrition(training, rate_per_s, conditions)`: one regiment of 400, `Steady(Line)`, neutral inputs, casualties injected at `rate_per_s` by a fixture stream (a test hook that appends `Casualty` records, no firing), seeds 1 to 8. Metric: loss fraction of `S0` at first entry to FallingBack (mean over seeds). Bands in `qc/calibration/morale-bands.toml`; they change only by amendment citing evidence, before the first playable-battle milestone.

| band | quantity | raw | trained | veteran | tag and source |
|---|---|---|---|---|---|
| M1 | fast loss fraction at 8 per s (percent) | 12 to 20 | 18 to 28 | 25 to 38 | unverified placeholder CT-006-61 |
| M2 | slow loss fraction at 0.5 per s (percent) | 18 to 35 | 30 to 48 | 40 to 65 | CT-006-61; Wheatfield 30 to 50 percent (CWV p082, p085) |
| M3 | flank (90 degrees, all enemy firing) over frontal M1 | at most 0.70 | at most 0.70 | at most 0.70 | partial: flanking decided attacks (CWV p065 9.3); CT-006-62 |
| M4 | rear over frontal M1 | at most 0.55 | at most 0.55 | at most 0.55 | CT-006-62 |
| M5 | rally time, unpressed, from Broken (s) | 120 to 300 | 60 to 150 | 30 to 90 | veterans recover faster (CWV p511 4.6); CT-006-63 |
| M6 | veteran with leader prestige 1000 and wall: M1 plus points | n/a | n/a | at least 8 | partial: 1st Minnesota (CWV p082); CT-006-64 |
| M7 | Wavering dwell before FallingBack at 8 per s (median, s) | at least 6 | at least 6 | at least 6 | symptoms must be visible before the break; CT-006-65 |
| M8 | neighbour cascade: trained neighbour 100 m away with 10 percent losses is at least Wavering within 3 periods of a Broke event | n/a | yes | n/a | rout phenomena (CWV p065 9.4); CT-006-66 |

Also CT-006-67 (veteran over raw M1 ratio 1.5 to 2.5; CWV p030) and CT-006-68 (a Broken regiment loses 30 to 60 percent of its roster to absence in 120 s; straggling cost thousands of men, CWV p065). Hand check, M1 trained: `(880000 - 350000) / 5495 = 96` casualties, 24 percent of 400; the implementer records all bands once with the script.

## 11. Assumptions
Officers are chosen by roster index. Neighbours are felt by distance, without line of sight or smoke. Leaders come from scenario data until a command spec exists. Recovered disabled soldiers do not undo their shock. Retiring regiments turn and walk.

## 12. Risks
Flicker near thresholds (hysteresis and `dwell`). Hidden cohesion frustrates players unless `causes` is reliable (genre checklist 7, 12). Cascades may chain too eagerly (CT-006-66). The anchor lag halt (SPEC-004 section 7) may stall a running Broken line; T6-07 includes a movement check.

## 13. Done when
fmt, clippy `-D warnings`, `cargo test --workspace` (also `--release`) and T6-01 to T6-15 pass; schema, data, bands and goldens committed and verified on three platforms; scans pass; public items documented; no floating point outside `#[cfg(test)]`; amendments of 5.8 applied to SPEC-004 and SPEC-005 text in the same change that lands the code; T6-15 numbers recorded in a cycle report.
