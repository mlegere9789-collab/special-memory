# REGISTRY: shared namespaces for SPEC-001 to SPEC-016

This file is the single source of truth for every identifier, value and order that more than one spec touches. A spec that needs a new tag, stream, flag, hash block, tick step, calibration target, amendment label, ticket id or ADR number takes it here first (a small registry change, then the spec). Where a spec and this file disagree on a shared value, this file wins and the spec is corrected. Conflicts found while building the registry, and the reasons for each choice, are in [CONFLICTS.md](CONFLICTS.md); the parallel build schedule is [ROADMAP.md](ROADMAP.md).

Notation: "was" marks a value a spec claimed before this registry reassigned it. Hex values are written `0x..`; a bare decimal tag in an older text (for example "tag 16") means the decimal value (`0x10`). Section numbers in other specs that say "REGISTRY section N" point here.

## 1. Spec index

| Spec | Topic | Home | Tests | CT prefix | Hypotheses | Tickets |
|---|---|---|---|---|---|---|
| 001 | core-math | `crates/core-math` | (unnumbered) | none | none | done |
| 002 | sim-state, replay v1 | `crates/sim-state` | (unnumbered) | none | none | done |
| 003 | sim-cli, toy scenario | `shells/sim-cli` | (unnumbered) | none | none | done |
| 004 | movement, drill, formations | `crates/battle-rules` | `T4-nn` | `CT-004-n` | H4 | T-014 to T-019 |
| 005 | weapons, flight, hits, F0 | `crates/battle-rules` | `T5-nn` | `CT-005-n` | H5 | T-020 to T-024 |
| 006 | morale and cohesion | `battle-rules::morale` | `T6-nn` | `CT-006-n` | H6 | T-061 to T-066 |
| 007 | command friction, information | `battle-rules::command` | `T7-nn` | `CT-007-n` | H7 | T-071 to T-076 |
| 008 | fidelity scheduler, scenario loader | `battle-rules` (`scenario`, `fidelity`, `aggregate`, `f1`, `sector`, `runner`) | `T8-nn` | `CT-008-n` | H8-1 to H8-5 | T-081 to T-087 |
| 009 | smoke, weather, sight | `battle-rules::environment` and siblings | `T9-nn` | `CT-009-n` | H9 | T-091 to T-097 |
| 010 | battle AI | `crates/battle-ai` | `T10-nn` | `CT-010-n` | H10 (was H6) | T-101 to T-109 |
| 011 | historical replay and calibration | `crates/history-cal` (was `calibration`) | `T11-nn` | `CT-011-n` | H11 | T-111 to T-116 |
| 012 | acoustics model | `crates/acoustics-model` | `T12-nn` | `CT-012-n` | H12 | T-121 to T-127 |
| 013 | Gettysburg data and audit | `tools/gb_audit.py`, `data/history/gettysburg` | `T13-nn` | `CT-013-n` | H13 | T-130 to T-136 |
| 014 | terrain pipeline | `crates/terrain`, `shells/terrain-cli` | `T14-nn` | `CT-014-n` | H14 | T-141 to T-148 |
| 015 | interface, tutorial, accessibility | `crates/ui-model` | `T15-nn` | `CT-015-n` | none named | T-151 to T-159 |
| 016 | prototype and vendor briefs | `shells/sim-capi`, `shells/perf-kit`, `qc/perf` | `T16-nn` | `CT-016-n` | H16-1 to H16-11 | T-161 to T-167 |
| registry | this file | `battle-rules::registry`, `tools/check_registry.py` | `TR-nn` | none | none | T-025 to T-027 |

## 2. Replay inputs

An input is `(tick u64, actor u8, seq u16, payload)` sorted by `(tick, actor, seq)` (SPEC-002). The first payload byte is the tag. Inputs of one tick are applied in that order before the step (section 6, PR-1).

### 2.1 Actors

| Actor | Owner | Meaning | Was |
|---|---|---|---|
| `0x00`, `0x01` | SPEC-007 | human seat of side 0 or 1 (`actor = side`) | unchanged |
| `0x02` to `0x3F`, `0x40` to `0x5F` | reserved | further seats | `0x40 + side` was SPEC-010 `AI_ACTOR_BASE` |
| `0x60`, `0x61` | SPEC-010 | advisor reply of side 0 or 1 (`ADVISOR_ACTOR_BASE 0x60 + side`) | unchanged |
| `0x80`, `0x81` | SPEC-007 | AI seat of side 0 or 1 (`0x80 + side`); SPEC-010 `AI_ACTOR_BASE = 0x80` | SPEC-010 said `0x40` |
| `0xFD` | SPEC-011 | historical script (compiled arrivals, orders, triggers) | SPEC-011 said actor 0 |
| `0xFE` | SPEC-007 | scenario script events (tag `0x13`) | SPEC-007 said `0xFF` |
| `0xFF` | system | seq 0: scenario binding (SPEC-008); seq 1: difficulty record (SPEC-015); seq 2 and 3: AI configuration of side 0 and 1 (SPEC-010, tag `0xA1`) | SPEC-008 seq 0 unchanged; SPEC-015 said seq 0; SPEC-007 said `0xFF` for scripts |

Seat actors are `0x00`, `0x01`, `0x80`, `0x81` (and the reserved seat ranges). Only seat actors are subject to the direct-order refusal of SPEC-007 section 4 (`InputError::DirectOrder`). Script actors (`0xFD`, `0xFE`, `0xFF`) and orders the engine itself issues (morale flight orders of SPEC-006, trigger orders of SPEC-008) are not seats and are never refused. Within one tick seat inputs apply before script inputs (ascending actor); a seat can address a regiment only after it exists.

### 2.2 Payload tags

| Tag | Owner | Meaning | Was |
|---|---|---|---|
| `0x01` Hold, `0x02` Move, `0x03` Form, `0x04` Face | SPEC-004 | regiment order, `u8 tag, u16 regiment, ...` | unchanged |
| `0x05` | SPEC-005 | `set_fire_mode` payload | unchanged |
| `0x06` to `0x0F` | reserved | further regiment-level orders (SPEC-004 and SPEC-005 family) | free |
| `0x10` | SPEC-007 | Dispatch (`u8 0x10, u8 medium, u8 latitude, u16 from`, then `encode_order` bytes) | unchanged; SPEC-008 and SPEC-011 also claimed decimal 16 |
| `0x11` | SPEC-007 | reserved by SPEC-007 | decimal 17 was claimed by SPEC-011 |
| `0x12` | SPEC-007 | Options (`omniscient_view`, `instant_orders`) | unchanged |
| `0x13` | SPEC-007 | Script (officer down or up, link cut or restored) | unchanged |
| `0x14` to `0x1F` | reserved | SPEC-007 growth | free |
| `0x20` | SPEC-008 | ScenarioBind: `u8 0x20, u64 scenario_hash` | was tag 16 |
| `0x21` to `0x2F` | reserved | SPEC-008 growth | free |
| `0x30` | SPEC-011 | Arrive: `unit_index u16, x i64, y i64, heading u32, formation u8, count u16` | was tag 16 |
| `0x31` | SPEC-011 | Trigger: `index u16` | was tag 17 |
| `0x32` to `0x3F` | reserved | SPEC-011 growth | free |
| `0x40` | SPEC-015 | Difficulty record (`encode_difficulty`) | no tag was given |
| `0x41` to `0x4F` | reserved | SPEC-015 growth | free |
| `0x50` to `0x5F` | reserved | SPEC-006 (no input payload today) | free |
| `0xA0` | SPEC-010 | advisor reply (`u8 0xA0, u32 request_id, u8 n, ...`) | unchanged |
| `0xA1` | SPEC-010 | AiConfig: `u8 0xA1, u8 side, u8 level (0 easy, 1 standard, 2 hard), u8 advisor (0 off, 1 on)`, actor `0xFF`, seq `2 + side`; `hd-sim ai-verify` reads it (the replay header has no difficulty field); SPEC-010 owns the definition, ticket T-108 | missing |
| `0xA2` to `0xAF`, `0x60` to `0x9F`, `0xB0` to `0xFF` | reserved | free | free |

Decoders reject an unknown tag and any trailing byte. A new tag is added here before it is used.

## 3. Random streams (`StreamId(u32)`)

| Stream id | Owner | Meaning | Was |
|---|---|---|---|
| `1`, `2`, `3` | SPEC-003 | toy scenario `Placement`, `Wander`, `Skirmish` | unchanged |
| `0x0B01` | SPEC-004 | `STREAM_BATTLE_SETUP`, scenario jitter | unchanged; block `0x0B00..0x0BFF` is SPEC-004 |
| `0x0C02` | SPEC-007 | `STREAM_OBSERVE`; block `0x0C00..0x0CFF` is SPEC-007 | unchanged |
| `0x0D00..0x0DFF`, `0x0E00..0x0EFF` | reserved | SPEC-009 and SPEC-015 (neither draws today) | free |
| `0x4000_0000 + slot` | SPEC-005 | `STREAM_SOLDIER_BASE`, scenario-start soldiers | unchanged |
| `0x5000_0000 + regiment` | SPEC-006 | `STREAM_MORALE_BASE` | unchanged |
| `0x5100_0000 + message id` | SPEC-007 | one stream per message (id below 2^24) | was `0x5000_0000 + id` |
| `0x5200_0000 + regiment` | SPEC-008 | `STREAM_F1_COUNT` | was `0x5000_0000 + regiment` |
| `0x5400_0000 + side * 16 + layer` | SPEC-010 | `STREAM_AI_BASE`; layer 0 OL, 1 TL, 2 estimate noise, 3 belief and forecast seeds, 4 spare | was `0x5000_0000` |
| `0x5800_0000 + regiment` | SPEC-008 | `STREAM_F1_VICTIM` | unchanged |
| `0x8000_0000 + serial` | SPEC-008 | materialised soldiers (SPEC-005 amendment A7) | unchanged |
| `0x5300_0000`, `0x5500_0000..0x57FF_FFFF`, `0x5900_0000..0x7FFF_FFFF`, `0x9000_0000..` | reserved | free | free |

Not streams: projectile seeds and burst children (`SplitMix64`, SPEC-005), calibration seed lists (SPEC-005 seeds 1 to 8, SPEC-007 1 to 64, SPEC-008 1 to 32, SPEC-011 1 to 16 and 101 to 116) which are master seeds. Specs 009, 012, 013, 014 and 016 draw nothing. Every ticket that adds a stream adds its constant to `battle_rules::registry` (T-026) in the same change.

## 4. Flags and bit fields

### 4.1 `AgentStore.flags` (u16)

| Bit | Name | Owner | Meaning |
|---|---|---|---|
| 0 | `F_KNEEL` | SPEC-004 | kneeling |
| 1 | `F_PRONE` | SPEC-004 | prone |
| 2 | `F_DISABLED` | SPEC-004 | down for a number of ticks |
| 3 | `F_WOUNDED` | SPEC-004 | out for the battle |
| 4 | `F_GUN` | SPEC-004 | gun entity |
| 5 | `F_LOOKBACK` | SPEC-006 | looking back (symptom) |
| 6 | `F_SKULK` | SPEC-006 | left the ranks, alive, outside the roster |
| 7 to 11 | reserved | registry | later rules |
| 12 to 15 | reserved | registry | host and test use; never hashed meaning |

No two specs claimed the same bit. `SoldierRec.flags` (SPEC-016) is a copy of this field.

### 4.2 Other bit fields and enumerations

| Name | Owner | Values |
|---|---|---|
| `TraitMask` | SPEC-007 | `T_AGGRESSIVE 1`, `T_CAUTIOUS 2`, `T_HIGH_DISC 4`, `T_LOW_DISC 8` |
| Options payload flags (`0x12`) | SPEC-007 | bit 0 `omniscient_view`, bit 1 `instant_orders` |
| `Medium` | SPEC-007 | `Courier 1`, `Flags 2`, `Telegraph 3` (0 in a dispatch means automatic) |
| `Latitude` | SPEC-007 | `Strict 0`, `Discretionary 1` |
| `Level` | SPEC-007 | army 0, corps 1, division 2, brigade 3, regiment 4 |
| `Difficulty.assists` | SPEC-015 | bit 0 ETA window, 1 auto-pause, 2 order preview, 3 full view, 4 historical restrictions |
| Tile flags | SPEC-014 | bit 0 `NO_SURVEY`, 1 `ADJ_RESTORE`, 2 `ADJ_PLAY`, 3 `POLISH`, 4 `STEEP` |
| `BattleEvent.kind` | SPEC-016 | 0 shot, 1 misfire, 2 burst, 3 casualty |
| `FireRecordKind` | SPEC-012 | `Shot`, `Burst` |
| `CasualtyKind` | SPEC-005 | `Killed`, `Wounded`, `Disabled`; `Captured` is added by a later capture spec (SPEC-011 assumption A2) |
| `MoraleKind` | SPEC-006 | `Steady`, `Wavering`, `FallingBack`, `Broken`, `Rallied` |

## 5. Hash blocks and their order

`battle_hash(&BattleState)` is one `StateHasher` stream (FNV-1a 64). Blocks are written in this order. An optional block is written only when its option is `Some`, and begins with its `u32` magic so that no combination of present and absent blocks can produce the same bytes as another.

| Order | Block | Owner | Written when | Magic |
|---|---|---|---|---|
| 1 | world (`write_world`) | SPEC-004 | always | none |
| 2 | regiments (ascending id) | SPEC-004 | always | none |
| 3 | slots (`LocoExt`) | SPEC-004 | always | none |
| 4 | fire block (all `FireExt` fields ascending, then `seq`, then the pool) | SPEC-005 | always; SPEC-004 writes `u32 0` here until T-022 | none |
| 5 | smoke cells | SPEC-005 | always; zero block until T-022 | none |
| 6 | casualty records | SPEC-005 | always; zero block until T-023 | none |
| 7 | morale | SPEC-006 | `morale` is `Some` | `0x4D4F_5231` ("MOR1"); was no magic |
| 8 | command | SPEC-007 | `cmd` is `Some` | `0x434D_4431` ("CMD1"); was `u8 1` |
| 9 | environment | SPEC-009 | `env` is `Some` | `0x454E_5631` ("ENV1"); unchanged |

The three SPEC-004 reserved zero blocks are the SPEC-005 blocks 4 to 6; with every option `None` the stream is exactly the SPEC-005 stream, so no existing golden changes when a layer lands.

Other hashes (none of them part of `battle_hash`):

| Hash | Owner | Contents |
|---|---|---|
| `Battle::hash` | SPEC-008 | the `battle_hash` byte stream (through `battle_hash_into(&mut StateHasher)`), then block `0x4649_4431` ("FID1") with the SPEC-008 fields; `BattleState::hash` stays `battle_hash` |
| `history_hash` | SPEC-011 | `Battle::hash` (or `battle_hash` for a class-0 `BattleState`), `u32` log length, timeline log entries |
| `scenario_hash` | SPEC-008 | canonical encoding of the parsed, sorted scenario |
| `Ai::hash` | SPEC-010 | AI state, never mixed into the battle |
| `snapshot_hash` | SPEC-015 | canonical `Snapshot` encoding |
| capi snapshot hash | SPEC-016 | `tick`, counts, then records (section 4 of SPEC-016) |
| `packet_hash`, `report_hash` | SPEC-012 | acoustics outputs |
| tile `body_hash`, pack `content_hash` | SPEC-014 | terrain files and index |

Excluded from every hash because they are copies of what the step already hashed: `sound_log` (SPEC-012), the event buffer (SPEC-016), counters (SPEC-005, SPEC-009, SPEC-010), UI state (SPEC-015). `HashMap` iteration never feeds a hash.

## 6. Tick order

One ordered list covers every optional layer. A layer that is off (`None`, class 0 only, no `Battle` wrapper) skips its steps and the rest keep their order. The list replaces the six-step list of SPEC-005 section 7 as the full order (SPEC-005 amendment A5); the SPEC-005 list is the F0 core inside it (steps TK-04 to TK-07, TK-10, TK-14, TK-15).

### 6.1 Before the step (the shell or driver)

| Step | What | Owner |
|---|---|---|
| PR-1 | apply the tick's inputs in `(actor, seq)` order: seat inputs through `apply_command_input` when `cmd` is `Some` (SPEC-007), otherwise `issue_order` and `set_fire_mode` (SPEC-004, SPEC-005); script inputs (`0xFD`, `0xFE`, `0xFF`) | SPEC-002, 004, 005, 007, 011 |
| PR-2 | the AI step (`Ai::step`) and the UI capture run between ticks, through ports; they change state only through PR-1 inputs of the next tick | SPEC-010, 015 |

### 6.2 `BattleState::step` and `Battle::step`

| Step | When | What | Owner and section |
|---|---|---|---|
| TK-01 | `tick % 20 == 0`, `Battle` only | departures, arrivals, triggers, weather rows (when `env` is `None`), classification, hand-overs (ascending regiment id), sector update | SPEC-008 section 4 |
| TK-02 | `env` is `Some`, `tick % 4 == 0` | refresh `WeatherNow`, powder damp, ground wetness, heat, `Weather` | SPEC-009 sections 3.2, 7 |
| TK-03 | `cmd` is `Some` | `step_command`: officer positions, observation, reports, arrivals ascending `(due, id)`, staff completions ascending `(ready, id)` | SPEC-007 section 9 |
| TK-04 | always | movement: rebuild grid; regiments ascending (orders, drill, anchor, close-up); phase A; phase B; heat multipliers (SPEC-009), peek offset in the steering target (SPEC-009), aggregate regiments move by SPEC-008 6.3 | SPEC-004 section 11; SPEC-009 sections 6, 7; SPEC-008 6.3 |
| TK-05 | always | rebuild grid; release expired disables | SPEC-005 section 7 (2) |
| TK-06 | always | fire phase, ascending shooter slot, gates in section 6.3 | SPEC-005 sections 3, 4, 7 (3); SPEC-006 5.8; SPEC-008; SPEC-009 section 6 |
| TK-07 | always | flight and hit, ascending `seq`, children join the tail | SPEC-005 sections 2, 5, 7 (4) |
| TK-08 | `Battle` with class 1 regiments | F1 bundle release at `tick % bundle_ticks == regiment % bundle_ticks`; delay-ring release at `due` | SPEC-008 6.1 |
| TK-09 | `Battle` with class 2 regiments | F2 exchange at `tick % f2_interval_ticks == regiment % f2_interval_ticks` | SPEC-008 6.2 |
| TK-10 | always | apply casualties (grouped by target ascending), append the casualty log | SPEC-005 section 6 |
| TK-11 | `morale` is `Some` | `step_morale`: casualty ingestion every tick, slow period when `tick % 20 == 0` | SPEC-006 section 5 |
| TK-12 | always | smoke: `env` is `None`: per-tick decay of SPEC-005 section 8; `env` is `Some` and `tick % 4 == 3`: the SPEC-009 update | SPEC-005 section 8; SPEC-009 section 4 |
| TK-13 | `env` is `Some` | `tick % 20 == 19`: peek update and `EnvInputs`; observers when `tick % 20 == observer id % 20` | SPEC-009 sections 6, 8 |
| TK-14 | always | roster updates (dirty flags for close-up) | SPEC-005 section 7 (6), SPEC-004 section 8 |
| TK-15 | always | `advance_tick` | SPEC-002 |

### 6.3 After the step

| Step | What | Owner |
|---|---|---|
| PS-1 | checkpoint hash at the replay interval | SPEC-002, 003 |
| PS-2 | evaluate history observations at `world.tick` | SPEC-011 |
| PS-3 | drain `sound_log` and events (copies); `AcousticsEngine::end_tick` | SPEC-012, SPEC-016 |

### 6.4 Fire-phase gates (TK-06), per shooter in ascending slot

1. Skip a shooter that is despawned, `F_DISABLED`, `F_WOUNDED`, `F_SKULK` (SPEC-006) or out of rounds (SPEC-005).
2. Loading progress: when `morale` is `Some`, `left -= 1` only if `load_gate(tick, slot_index, fire_pm)` (SPEC-006); otherwise every tick (the `ram` multiplier of SPEC-005 applies in both cases).
3. Not Ready: next shooter.
4. The regiment may fire: SPEC-004 `can_fire` and, with morale, kind in `Steady`, `Wavering`, `Rallied`, and `FireMode::AtWill` (SPEC-005).
5. Target: SPEC-005 `target_roster[j % len]` within `extended_mm`; with `env` `Some` the SPEC-009 gate tries `(j + k) % n`, `k = 0..=retries`, first visible and in reach; none: hold fire (no round, no draw, no loading change, `blind` set). In a `Battle`, the shooter class and the target representation choose projectile or F1 shot (SPEC-008 section 4).
6. A shot draws exactly 50 words in the SPEC-005 order whatever the outcome (an F1 shot keeps the misfire word and discards 49).
7. Effects: round spent, fouling, firing fatigue (multiplied by `drain_pm / 1000` with heat), smoke source (SPEC-005 or `add_smoke` of SPEC-009), counters, `sound_log` record (A10), event record (A12), loading restart.

### 6.5 Composition rules for shared formulas

- `sigma_s` is the SPEC-005 chain (aim sigma times `aim_scale_pm`, times the fatigue factor, times the smoke factor `1000 + 4 * smoke_at_shooter`), then times `shake_pm / 1000` (SPEC-006), each factor floored separately, in that order. Square-root combination with `sigma_b` follows.
- `smoke_at_shooter` is `smoke_pm` of the shooter's cell (SPEC-009). With `env` `None` SPEC-005 supplies it as `min(1000, density * 1000 / SMOKE_REF_UNITS)` with `SMOKE_REF_UNITS = 20000`, a placeholder owned by SPEC-005 (CT-005-8), recorded by T-022.
- Fatigue per tick: SPEC-004 section 4 order with the SPEC-009 multipliers (SPEC-004 amendment A5): moving `fatigue += drain_ppm * effort_pm * drain_pm / 10^6`; resting `fatigue -= rest_recover * rest_pm / 1000`; firing drain times `drain_pm / 1000`; with neutral weather every multiplier is 1000 and the result equals the plain formula.
- Range estimate error: `range_sigma_pm * g` of SPEC-005, then times `(1000 + range_smoke_gain_pm * (1000 - transmit_pm) / 1000) / 1000` of SPEC-009.

## 7. Calibration target ids

Every calibration target is written `CT-<spec>-<n>` with the three-digit spec number and the spec's own local number (local numbering is kept). The rule schema pattern is `^CT-[0-9]{3}-[0-9]+$`. A bare `CT-n` is refused from ticket T-025 on. A target is named in full wherever another spec refers to it.

| Spec | Ids | Local numbering kept | Was |
|---|---|---|---|
| 004 | `CT-004-1` to `CT-004-5` | 1 to 5 | `CT-1` to `CT-5` (data rows `locomotion.toml`, `terrain.toml`, `drill.toml`) |
| 005 | `CT-005-1` to `CT-005-9` | 1 to 9 | `CT-1` to `CT-9` (data rows `ammo.toml`, `weapons.toml`, `damage.toml`); collided with SPEC-004 |
| 006 | `CT-006-61` to `CT-006-68` | 61 to 68 | `CT-61` to `CT-68` |
| 007 | `CT-007-1` to `CT-007-7` | 1 to 7 | `CT-1` to `CT-7` |
| 008 | `CT-008-80` to `CT-008-88` | 80 to 88 | `CT-80` to `CT-88` |
| 009 | `CT-009-1` to `CT-009-6` | 1 to 6 | `CT-1` to `CT-6` |
| 010 | `CT-010-1` to `CT-010-7` | 1 to 7 | `CT-1` to `CT-7` |
| 011 | `CT-011-1101` to `CT-011-1105` | 1101 to 1105 | `CT-1101` to `CT-1105` |
| 012 | `CT-012-1` to `CT-012-5` | 1 to 5 | `CT-1` to `CT-5` |
| 013 | `CT-013-1301` to `CT-013-1305` | 1301 to 1305 | `CT-1301` to `CT-1305` |
| 014 | `CT-014-1` to `CT-014-11` | 1 to 11 | `CT-1` to `CT-11` with `note = "SPEC-014"` |
| 015 | `CT-015-1` to `CT-015-11` | 1 to 11 | `CT-1` to `CT-11` |
| 016 | `CT-016-1` to `CT-016-11` | 1 to 11 | `CT-1` to `CT-11` |

Existing data rows migrate by file: `locomotion.toml`, `terrain.toml`, `drill.toml` rows get prefix `CT-004`; `ammo.toml`, `weapons.toml`, `damage.toml` rows get `CT-005`. The loader (`provenance.rs`), the schemas (`rules-common`, `ammo`, `damage`, `drill`, `weapons`), the validator test fixtures and ADR 0011 change in ticket T-025.

## 8. Other id families

### 8.1 Acceptance test, band and rule ids

| Family | Owner | Note |
|---|---|---|
| `T4-nn` to `T16-nn` | the spec of that number | unique; ticket tests are named by these |
| `TR-nn` | registry | `TR-01` constants disjoint (T-026), `TR-02` CT format (T-025), `TR-03` registry checker (T-027) |
| `B1` to `B11` | SPEC-005 | bands in `qc/calibration/bands.toml`, family `fire` |
| `E1` to `E8` | SPEC-009 | same file, family `env` |
| `M1` to `M8` | SPEC-006 | `qc/calibration/morale-bands.toml` |
| `CB1` to `CB6` | SPEC-012 | `qc/calibration/acoustics-bands.toml` |
| `HB-01` to `HB-24` | SPEC-011 | Gettysburg bands (was `GB-01` to `GB-24`, which read like the `GBnn` rules of SPEC-013) |
| `TL-01` to `TL-15`, `CF-01` to `CF-10` | SPEC-011 | timeline rows, conflict fixtures |
| `GB01` to `GB19` | SPEC-013 | consistency rules |
| `C01` to `C14` | SPEC-013 | known-conflict fixtures |
| `U1` to `U8` | SPEC-015 | usability criteria |
| `D-01` to `D-10` | CONFLICTS | open design decisions |

### 8.2 Ticket ids

Ticket ids are `T-NNN`, three digits: `T-014` to `T-024` (SPEC-004 and SPEC-005), then `T-0S1..` for spec `S` (SPEC-006 to SPEC-009) and `T-S01..` for specs 10 to 16, the registry at `T-025` to `T-027`. Lists that used other forms are renamed so that ticket ids and test ids (`T12-01`, `T16-01`) can no longer be confused.

| Old | New | Spec |
|---|---|---|
| `TC-1` to `TC-6` | `T-071` to `T-076` | 007 |
| `T-A1` to `T-A8` (and `T-A5a`, `T-A5b`) | `T-101` to `T-108` (`T-105a`, `T-105b`) | 010 |
| `T12-1` to `T12-7` | `T-121` to `T-127` | 012 |
| `S14-1` to `S14-8` | `T-141` to `T-148` | 014 |
| `T-15.1` to `T-15.8` | `T-151` to `T-158` | 015 |
| `T-16-1` to `T-16-7` | `T-161` to `T-167` | 016 |
| unchanged | `T-061` to `T-066`, `T-081` to `T-087`, `T-091` to `T-097`, `T-111` to `T-116`, `T-130` to `T-136` | 006, 008, 009, 011, 013 |
| added | `T-025` to `T-027` (registry), `T-109` (HqPort adapter), `T-159` (UI adapters) | registry |

### 8.3 Hypothesis ids

`H4` SPEC-004, `H5` SPEC-005, `H6` SPEC-006 (SPEC-010 used H6 as well and now uses `H10`), `H7` SPEC-007, `H8-1` to `H8-5` SPEC-008, `H9` SPEC-009, `H10` SPEC-010, `H11` SPEC-011, `H12` SPEC-012, `H13` SPEC-013, `H14` SPEC-014, `H16-1` to `H16-11` SPEC-016. SPEC-015 states budgets under `CT-015-10`.

### 8.4 Cycle and calibration report names (`docs/cycle-reports/`)

`CYCLE-00-baseline` (exists), `CYCLE-02-movement` (T-019), `CYCLE-03-f0` (T-024), `CYCLE-03-morale` (T-066), `CYCLE-04-fidelity` (T-087), `CYCLE-environment` (T-097), `CYCLE-command` (T-076), `CYCLE-ai` (T-108; was `CYCLE-AI-01`), `CYCLE-terrain-slab` (T-148), `CYCLE-acoustics` (T-127), `CYCLE-M1-renderer` (SPEC-016 gate), `CAL-NN` (SPEC-011). Placeholders `CYCLE-0N-`, `CYCLE-xx-`, `CYCLE-NN-` in older text mean the slug above.

### 8.5 Library scenario names

`battle_rules::scenarios` (one submodule per spec, section 14): SPEC-004 `march_single`, `deploy_pair`, `terrain_strip`, `march_large`; SPEC-005 `cal_line`, `cal_rof`, `cal_range`, `cal_canister`, `f0_large`; SPEC-006 `cal_attrition`; SPEC-007 `command_chain`, `cemetery_hill`; SPEC-008 `fid_mixed`, `gb_scale_lite`; SPEC-009 `env_smoke_line`, `env_hill_hq`, `env_heat_march`, `smoke_line`, `crest`. `battle_ai::scenarios`: `fs_*`, `ai_duel`. SPEC-011 scenario ids `gb_*` are data (`data/scenarios`), and SPEC-015 `drill_ground` is data. `hd-sim battle --scenario` takes library names; `hd-sim resolve --scenario` takes data ids. No name is used twice.

## 9. Amendments to base specs

Labels are sequential per base spec in landing order (wave, then ticket id). A label is written into the base spec by the ticket named, as an "Amendment An (from Tnnn)" section, before the tests and the code of that ticket. Labels already landed are unchanged.

### 9.0 SPEC-001 (core-math)

| Label | Content | Source | Ticket | Status |
|---|---|---|---|---|
| A1 | `Rng::try_from_state`, `from_state` zero-state rule, `coin` boundary behaviour | review | T-H1 | landed |

### 9.1 SPEC-002 (sim-state)

| Label | Content | Source | Ticket | Status |
|---|---|---|---|---|
| A1 | replay reader and grid hardening | review | T-H1 | landed |
| A2 | `SpatialGrid::query_segment` (the text SPEC-005 calls "amendment A1 to SPEC-002") | SPEC-005 | T-014 | landed |
| A3 | grid constructor tweak for the hot-window grid, only if needed | SPEC-008 | T-087 | conditional |

### 9.2 SPEC-003 (sim-cli)

| Label | Content | Source | Ticket | Status |
|---|---|---|---|---|
| A1 | resource limits on `run` and `verify` | review | T-H1 | landed |
| A2 | `verify` binding dispatch and the subcommand table (section 15) | SPEC-008, 010, 011 | T-084 | pending |

### 9.3 SPEC-004 (movement)

| Label | Content | Source | Ticket | Wave | Status |
|---|---|---|---|---|---|
| A1 | `Gait` reuse, `from_rows` takes the table, `fatigue_step` takes `Fx`, blocking per axis | T-016 | T-016 | landed | landed |
| A2 | section 11: tag space, optional hash blocks with magics, direct-order refusal when `cmd` is `Some` (REGISTRY sections 2, 5) | SPEC-007 | T-071 | 2 | pending |
| A3 | section 5: terrain classes 8 to 11, slope as data (was SPEC-014 "A2") | SPEC-014 | T-141 | 2 | pending |
| A4 | section 7 and 11: `Regiment` gains `status`, `class`, `since`; aggregate regiments | SPEC-008 | T-083 | 4 | pending |
| A5 | section 4: heat and rest multipliers in the fatigue step | SPEC-009 | T-092 | 4 | pending |
| A6 | section 9: steering target offset `peek_mm` | SPEC-009 | T-095 | 5 | pending |
| A7 | sections 7 and 8: `issue_order` refusal in FallingBack and Broken, roster skips `F_SKULK`, `BattleState::can_fire` morale gate | SPEC-006 | T-064 | 6 | pending |
| A8 | public setters (fatigue, strength) and read accessors for belief building and the interface | SPEC-010, 015 | T-103 | 7 | pending |
| A9 | movement details found while building steering: snap needs `\|vel\| <= accel / TICK_HZ`, T4-03 against the fatigue-aware reference, gait defaults, anchor and scenario geometry (was labelled A2 in the T-019 commit; A2 belongs to SPEC-007) | T-019 | T-019 | 1 | landed |

### 9.4 SPEC-005 (weapons and fire)

| Label | Content | Source | Ticket | Wave | Status |
|---|---|---|---|---|---|
| A1 | the `query_segment` text; lives in SPEC-002 A2 | SPEC-005 | T-014 | landed | landed |
| A3 | data deviations (shot speed, pellet masses, ammo rows) | T-020 | T-020 | landed | landed |
| A4 | sub-step form, ground interpolation, range table, `angle_from_urad` | T-021 | T-021 | landed | landed |
| A5 | section 7: the full tick order is REGISTRY section 6 | registry | T-023 | 3 | pending |
| A6 | section 8: `SmokeGrid::density_at` | SPEC-007 | T-073 | 4 | pending |
| A7 | section 4: streams of materialised soldiers (`0x8000_0000 + serial`), shot-word discard, hand-over fields | SPEC-008 | T-083 | 4 | pending |
| A8 | sections 4 and 8: with `env` `Some`, the SPEC-009 smoke source and update replace section 8; firing drain scaled by heat; `Weather` filled by the weather timeline | SPEC-009 | T-092, T-093 | 4 | pending |
| A9 | sections 3 and 4: targeting gate, range-error multiplier, `smoke_at_shooter` | SPEC-009 | T-095 | 5 | pending |
| A10 | section 4 (was SPEC-012 "A5"): `sound_log` of `FireRecord` | SPEC-012 | T-121 | 5 | pending |
| A11 | sections 3 and 4: `load_gate` and `shake_pm` | SPEC-006 | T-064 | 6 | pending |
| A12 | section 6 (was SPEC-016 "A5"): `BattleEvent` buffer and `drain_events` | SPEC-016 | T-164 | 6 | pending |
| A13 | `FireExt` rounds setter and read accessors (fire state, smoke, casualties) | SPEC-010, 015 | T-103 | 7 | pending |
| A14 | section 7: counter `obstacle_stops` | SPEC-010 | T-106 | 10 | pending |
| A15 | section 5: `resolve_chord` tests only the obstacles of overlapped sectors | SPEC-008 | T-087 | 9 | pending, depends on D-01 |
| A16 | sections 2 and 5: ground-relative geometry over the terrain pack (was SPEC-014 "A4", which collided with the landed A4) | SPEC-014 | none yet | blocked | blocked on D-02 |

### 9.11 SPEC-011 (historical replay and calibration)

| Label | Content | Source | Ticket | Wave | Status |
|---|---|---|---|---|---|
| A1 | sections 1 to 5 and 10: one owner per historical row (ADR 0010 D-06), `[[override]]` rows, unit anchors, compile rules, policy rows | ADR 0010 | T-111 | 2 | landed |

Reserved: SPEC-009 A1 (weather fields reconciled with SPEC-013, SPEC-013 assumption A3) and SPEC-005 A17 (weapons beyond the five rows, SPEC-013 assumption A4) have no ticket and take the next free label when a ticket exists. No spec proposes amendments to SPEC-001 or to specs 006 to 016 other than these.

## 10. Clock, units of time, angles and weather conventions

| Quantity | Convention | Owners |
|---|---|---|
| `clock_s` | integer seconds since 00:00 local on 1863-07-01, no zone | SPEC-008 `clock_start_s`, SPEC-011 |
| scenario row times | `at_s`, `low_s`, `high_s`, `from_s` in SPEC-008 rows are seconds from scenario start: `tick = s * 20` | SPEC-008 |
| script times | `at_s`, `lo_s`, `hi_s`, `early_s`, `late_s` in SPEC-011 are `clock_s`: `tick = (at_s - clock_start_s) * 20` (`clock_start_s` defaults to 0) | SPEC-011 |
| `t_min` | `clock_s / 60`, minutes since 1863-07-01T00:00 (may be negative) | SPEC-009 `t_min`, `start_minute`; SPEC-013 `t_min` |
| ticks per minute | 1200 | SPEC-009 |
| `heading_deg` | whole degrees counter-clockwise from +x (east), converted by `Angle::from_degrees_int`; `0..359` | SPEC-008, SPEC-011 |
| `wind_*_mm_s` | motion of the air, x east, y north (the vector the wind blows toward) | SPEC-005, 009, 012 |
| `wind_from_deg` | compass bearing the wind comes from, clockwise from north, SPEC-013 data only; converts to the vector above | SPEC-013 |
| temperature | SPEC-009 `temp_f10` (tenths of F); SPEC-012 `temp_dc` (tenths of C); SPEC-013 `temp_f` (F) | see below |

Conversions (all floor division): `temp_dc = floor_div((temp_f10 - 320) * 10 + 9, 18)` (88 F, `temp_f10 = 880`, gives 311); from `temp_f` multiply by 10 first; SPEC-013 `temp_dc` formula is the same function.

## 11. Unit suffixes (additions to ADR 0004)

ADR 0004 lists `_mm`, `_mm_s`, `_pm`, `_ppm`, `_ticks`. The suffixes below are in use or required by specs; ADR 0011 (T-025) records them. A key whose suffix is not listed is refused by the schemas.

| Suffix | Meaning | Owner |
|---|---|---|
| `_mm_s2` | mm per second squared | SPEC-004 |
| `_urad`, `_urad_per_100m` | micro-radians | SPEC-005 |
| `_g`, `_mg` | grams, milligrams | SPEC-005, SPEC-009 |
| `_j` | joules | SPEC-005 |
| `_ppm_per_tick`, `_ppm_per_km`, `_cdb_per_km` | rates | SPEC-004, 007, 012 |
| `_cdb` | centibel (0.01 dB) | SPEC-012 |
| `_dc` | tenth of a degree C | SPEC-012 |
| `_f10` | tenth of a degree F | SPEC-009 |
| `_mk_per_m` | millikelvin per metre | SPEC-012 |
| `_hz`, `_ms` | hertz, milliseconds | SPEC-012 |
| `_s`, `_min` | seconds, minutes of clock or duration | SPEC-008, 009, 011 |
| `_m`, `_deg` | whole metres, whole degrees | SPEC-008, 011 |
| `_cm`, `_dm`, `_dmm_per_h` | centimetres, decimetres, tenths of mm per hour | SPEC-013, 014, 016 |
| `_pc` | percent (SPEC-016 recordings `humidity_pc` only; other specs use `_pm`) | SPEC-016 |
| `_x10`, `_us`, `_mb` | tenths (loudness), microseconds, megabytes | SPEC-016 |

## 12. Data layout

### 12.1 Scenario file and sidecars

A scenario is the closed file `data/scenarios/<id>.toml` (SPEC-008; unknown keys refused). Everything else about it is a sidecar in `data/scenarios/<family>/`, where `family` is the optional top-level scenario key (default the scenario `id`).

| Sidecar | Owner | Schema |
|---|---|---|
| `officers.toml` (officers and links) | SPEC-007 (was a section of the scenario file) | `officers.schema.json` |
| `weather.toml` | SPEC-009 (`data/scenarios/gettysburg/weather.toml` is family `gettysburg`) | `weather.schema.json` |
| `relief.toml` | SPEC-009 (was a `[relief]` table in the scenario file) | `relief.schema.json` |
| `script.toml` | SPEC-011 | `replay-script.schema.json` |
| `data/scenarios/terrain/<name>.rows` | SPEC-008 terrain `source` | hash in the scenario |

### 12.2 Rules files (`data/rules/<name>.toml`, schema `<name>.schema.json`, validated by name)

`locomotion`, `terrain`, `drill` (004); `ammo`, `weapons`, `damage` (005); `morale` (006); `command` (007); `fidelity`, `f1` (008); `environment` (009); `ai`, `ai_traits` (010); `acoustics` (012); `obstacles` (014). Terrain classes added by SPEC-004 A3 need matching rows in `environment.toml` `[[retain]]` (SPEC-009) and `acoustics.toml` `[[ground]]` (all zero for classes 8 to 11 unless data says otherwise).

### 12.3 Other roots

`data/history/` (battles, commanders, `gettysburg/` SPEC-013); `data/survey/`, `data/terrain/`, `qc/terrain/` (014); `data/ui/`, `data/guide/`, `data/text/`, `qc/usability/` (015); `data/animation/`, `data/audio/`, `data/provenance/`, `qc/perf/` (016); `qc/calibration/` files: `bands.toml` (005 and 009), `f0-curves.json` (005), `morale-bands.toml` (006), `command-bands.toml` (007), `acoustics-bands.toml` (012), `policy.toml`, `tunables.toml`, `split.toml`, `accepted-ranges.toml`, `holdout-log.toml`, `baseline.toml`, `gettysburg-bands.toml`, `golden/` (011); `qc/fixtures/` shared, each spec uses its own file names; `qc/golden-replays/expected.json` (section 15) and `expected-acoustics.json` (012); `qc/audit/gettysburg/` (013).

### 12.4 Id character sets

Scenario and unit ids: SPEC-008 `[a-z0-9_-]{1,32}`; SPEC-011 unit ids `[a-z0-9_]+` (a subset); SPEC-013 record ids `us_` or `cs_` plus slug. New data uses underscores only; `fid-small` keeps its hyphen.

## 13. Terrain cells and elevation

| Grid | Cell | Owner |
|---|---|---|
| simulation terrain (`TerrainGrid`), `SmokeGrid`, `ElevationGrid` | 8000 mm, all three equal (SPEC-009 rejects a mismatch) | SPEC-004, 009 |
| agent `SpatialGrid` | 4000 mm | SPEC-008 |
| compiled terrain pack, `TerrainWorld` | 4000 mm (`CELL_MM`) | SPEC-014 |
| sectors | 500,000 mm | SPEC-008 |
| sound clusters | 100,000 mm | SPEC-012 |

Until open decision D-02 closes, the simulation reads the pack only through `hd-terrain export-rows --cell-mm 8000`; `TerrainWorld` serves authoring, quality checks and the viewer.

## 14. Crates, modules and dependency order

### 14.1 Crates

| Crate | Path | Depends on | Created by |
|---|---|---|---|
| `core-math` | `crates/core-math` | none | done |
| `sim-state` | `crates/sim-state` | `core-math` | done |
| `battle-rules` | `crates/battle-rules` | `core-math`, `sim-state`, `serde`, `toml` (loader only) | T-015 |
| `terrain` | `crates/terrain` | `core-math` | T-142 |
| `perf-kit` | `shells/perf-kit` | `core-math` (optional) | T-162 |
| `acoustics-model` | `crates/acoustics-model` | `core-math`, `sim-state`, `battle-rules` | T-122 |
| `battle-ai` | `crates/battle-ai` | `core-math`, `sim-state`, `battle-rules` | T-102 |
| `history-cal` | `crates/history-cal` | `core-math`, `sim-state`, `battle-rules` | T-111 |
| `ui-model` | `crates/ui-model` | `core-math`, `sim-state`, `battle-rules` | T-151 |
| `sim-capi` | `shells/sim-capi` | `core-math`, `sim-state`, `battle-rules` | T-165 |
| `hd-terrain` | `shells/terrain-cli` | `terrain`, `core-math`, `battle-rules` (compatibility test) | T-144 |
| `sim-cli` (`hd-sim`) | `shells/sim-cli` | all of the above except `terrain`, `perf-kit`, `sim-capi`; glue between `terrain` and `battle-rules` obstacles lives here | done |

Dependency order (a crate only depends on earlier lines): `core-math`, `sim-state`, `battle-rules`; then `terrain`, `perf-kit`, `acoustics-model`, `battle-ai`, `history-cal`, `ui-model`, `sim-capi` (mutually independent); then `hd-terrain` and `sim-cli`. No crate depends on `battle-ai`, `history-cal` or `ui-model` except `sim-cli`. `unsafe_code` is forbidden workspace-wide; `sim-capi` alone opts out with its own `[lints]` table (ADR 0008). The `battle-ai` crate never names `BattleState` outside `lookahead.rs` (SPEC-010 T10-04); the `HqPort` implementation lives in `sim-cli` (T-109).

### 14.2 Module layout inside `battle-rules`

To keep parallel tickets off one file, each spec owns its own files: `scenarios/` and `calibration/` are directory modules with one file per spec (`march`, `fire`, `morale`, `command`, `fidelity`, `env`), `command/`, `morale/` and `environment.rs`, `sight.rs`, `observers.rs`, `smoke.rs` as the specs name them; the SPEC-008 wrapper is `runner.rs` (`battle.rs` already holds `BattleState`). `run_band_suite(family)` takes a band family (`fire` for B1 to B11, `env` for E1 to E8); `run_morale_bands`, `run_command_bands` stand alone.

Edits to `lib.rs` (exports), `Cargo.toml` members, `Cargo.lock`, `qc/golden-replays/expected.json` and `ledger/usage.csv` are one-line appends and are merged mechanically by the orchestrator. Behaviour edits to `fire.rs`, `aim.rs`, `fatigue.rs`, `smoke.rs`, `slots.rs`, `hit.rs` and the steering file never share a wave (ROADMAP).

## 15. `hd-sim`, replay binding and golden rows

| Subcommand | Owner | Ticket | Notes |
|---|---|---|---|
| `run`, `verify`, `hash`, `bench` | SPEC-003 | done | `verify` dispatch below |
| `battle --scenario NAME --ticks N --seed S` | SPEC-004 | T-019 | library scenarios; `--morale` on or off (SPEC-006, T-065); `--ai A:LEVEL,B:LEVEL` (SPEC-010, T-108) |
| `resolve --scenario ID --seed S [--ticks N] [--out FILE]` | SPEC-008 | T-084 | data scenarios |
| `fit-f1` | SPEC-008 | T-085 | writes `data/rules/f1.toml` |
| `ai-verify FILE` | SPEC-010 | T-108 | reads the `0xA1` records |
| `cal run`, `cal report`, `cal fit`, `cal holdout` | SPEC-011 | T-115, T-116 | |

`verify` binds by the replay header `scenario_id` and the first inputs: `toy` (SPEC-003); a library scenario name (SPEC-004); an id with a `0x20` ScenarioBind at `(0, 0xFF, 0)` (SPEC-008, rejects a hash or class mismatch with exit 2); `<scenario>/<variant>` (SPEC-011, scripted inputs from actor `0xFD`). SPEC-003 amendment A2 (T-084) writes this down.

`qc/golden-replays/expected.json` keeps `format = 1` and the `replays` array. Rows are `{file, kind, scenario, agents?, ticks, seed, checkpoint_interval, fidelity_class, final_hash}` with `kind` in `toy`, `battle`, `resolve`, `history`, `ai`; the three toy rows keep their current fields. Extra top-level sections: `terrain` (SPEC-014 pack content hashes), `reports` (SPEC-011 report hashes). `tools/verify_goldens.sh` iterates all of them.

## 16. Public API additions on existing types

### 16.1 `BattleState` fields

| Field | Type | Owner | Ticket |
|---|---|---|---|
| `fire`, `pool`, `obstacles`, `smoke`, `casualties`, `weather`, `seq` | as SPEC-005 section 10 | SPEC-005 | T-022, T-023 |
| `morale` | `Option<MoraleState>` | SPEC-006 | T-062 |
| `cmd` | `Option<CommandState>` | SPEC-007 | T-072 |
| `env` | `Option<Environment>` | SPEC-009 | T-092 |
| `sound_log` | `Vec<FireRecord>` | SPEC-012 | T-121 |
| `events`, overflow counter | bounded buffer of `BattleEvent` | SPEC-016 | T-164 |
| `soldier_serial` | `u32` | SPEC-008 | T-083 |

`BattleState` already derives `Clone` (T-018); every added field is `Clone`. `Regiment` gains `status`, `class`, `since` (SPEC-004 A4, T-083) and `peek_mm`, `view_blocked` (SPEC-004 A6, T-095).

### 16.2 Counters

`BattleState::counters()` returns one `Counters` struct; fields are appended, never reordered: `shots_fired`, `misfires`, `projectiles_spawned`, `unit_level_rolls` (SPEC-005, T-023); `sight_queries`, `shots_withheld`, `sight_blocked` (SPEC-009, T-095); `obstacle_stops` (SPEC-010, SPEC-005 A14, T-106). Counters are never hashed.

### 16.3 Record and event types

`Casualty` (SPEC-005, in the hash), `FireRecord` and `FireRecordKind` (SPEC-012, not hashed), `BattleEvent` (SPEC-016, not hashed), `MoraleEvent` (SPEC-006, hashed), `Sighting` and `SightLog` (SPEC-009, hashed), `OrderRecord` (SPEC-007, hashed).

### 16.4 Methods and functions

`battle_hash_into(&BattleState, &mut StateHasher)` (T-084, used by `Battle::hash`); `BattleState::{enable_morale, set_leader, set_supply, set_rally_point, step_morale, morale_report}` (SPEC-006); `{enable_command, dispatch, apply_command_input, step_command, order_views, picture_view}` (SPEC-007); `{set_environment, add_observer, sightings, env_inputs, view_blocked}` (SPEC-009); `sound_log()` (SPEC-012); `drain_events` (SPEC-016); fatigue, strength and rounds setters and the read accessors needed by `ui-model`, `battle-ai` and `sim-capi` (SPEC-004 A8, SPEC-005 A13, T-103). `SmokeGrid::density_at` (SPEC-005 A6, T-073); `SmokeGrid::{add_smoke, update, smoke_pm, total}` (SPEC-009).

### 16.5 Type names defined more than once (crate-qualified; never glob-imported together)

| Name | Definitions | Rule |
|---|---|---|
| `MoraleReport` | `battle_rules::morale` (SPEC-006), `ui_model` (SPEC-015) | `ui-model` adapter `MoraleFeed` (T-159); shape difference is D-04 |
| `OrderRecord` | `battle_rules::command` (SPEC-007), `ui_model` (SPEC-015) | adapter `CommandInfo` (T-159); status difference is D-05 |
| `Sighting` | `battle_rules::environment` (SPEC-009), `ui_model` (SPEC-015) | adapter only; no shared fields |
| `Contact` | `battle_rules::command` (SPEC-007), `battle_ai` (SPEC-010) | `BattleHq` (T-109) converts `ContactView` |
| `OrderBody` | `battle_rules::command` (SPEC-007), `battle_ai` (SPEC-010) | SPEC-010 `Reg` maps to SPEC-007 `OrderBody`; `Objective` uses the fallback |
| `Weather` | `battle_rules` (SPEC-005, wind and wetness), `acoustics_model` (SPEC-012, air state) | `Weather::from_sim` converts; never imported together |
| `Difficulty` | `battle_ai` (SPEC-010 levels), `ui_model` (SPEC-015 struct) | open decision D-08 |
| `Cluster` | `acoustics_model` (SPEC-012), `perf_kit` (SPEC-016) | prototype versus production, open decision D-10 |
| `Relief`, `ElevationGrid` | SPEC-012, SPEC-009 | open decision D-02 |

## 17. ADR numbers

| ADR | Topic | Owner | Ticket | Was |
|---|---|---|---|---|
| 0001 to 0005 | existing (0005 is the supply-chain gates) | | | |
| 0006 | command layer is optional state, its hash block is conditional | SPEC-007 | T-071 | was "ADR 0005" |
| 0007 | Gettysburg history data: integer-only records, claims and lineage, local clock stamps | SPEC-013 | T-130 | was "ADR 0005" |
| 0008 | C interface and the `unsafe` exception of `sim-capi` | SPEC-016 | T-165 | new |
| 0009 | renderer host decision | SPEC-016 | M1 gate | was "next free number" |
| 0010 | resolution of design decisions D-01 to D-10 | | | existing |
| 0011 | shared identifiers: `CT-<spec>-<n>`, unit suffixes of section 11, this registry as authority | registry | T-025 | was "ADR 0005" (number already taken) |
| 0012 and above | free | | | |

The acoustics unit suffixes do not need their own ADR (SPEC-012 asked for one): they are in section 11 and ADR 0011.

## 18. Registry tickets

All three start now, in wave 1, and every ticket in a later wave depends on them (ticket tables list the dependency where it is direct).

**T-025: CT identifier migration.** Class S. Spec: section 7. Work: pattern `^CT-[0-9]{3}-[0-9]+$` in `rules-common.schema.json` and the four schemas that copy it, `provenance.rs` `parse_ct` and its tests, rewrite the `calibration_target` values of the six rules files by the file mapping, `tools/test_validate_data.py` fixtures, ADR 0005 (identifiers and unit suffixes). Files: `crates/battle-rules/src/provenance.rs`, `data/rules/*.toml`, `data/schemas/`, `tools/test_validate_data.py`, `docs/adr/0011-*`. Acceptance **TR-02**: a bare `CT-n` and a three-digit prefix with a letter are refused by the Rust loader and by the validator; every existing row validates in the new form; the T4 and T5 data tests pass unchanged apart from the id strings; golden hashes unchanged.

**T-026: shared constants module.** Class S. Spec: sections 2 to 5. Work: `crates/battle-rules/src/registry.rs` holding every actor, tag, stream base, flag bit and hash magic of sections 2 to 5 as `pub const`, re-exporting the existing constants without changing their values. Later tickets use these names and add rows here. Files: `crates/battle-rules/src/registry.rs`, `lib.rs`. Acceptance **TR-01**: actors, tags, streams (as ranges of their blocks), flag bits and magics are pairwise disjoint; the existing constants equal their previous values; the table in this file equals the constants (parsed from the markdown by the checker of T-027 in CI).

**T-027: registry checker and validator plugins.** Class S. Spec: sections 7, 8, 12. Work: `tools/check_registry.py` (every `CT-NNN-n` in `data/`, `docs/` and `qc/` has the prefix of a registered spec; ticket ids are unique across `TICKETS-*.md`; the ranges and hex values in sections 2 and 3 are disjoint; every ADR number is used once); change `tools/validate_data.py` to load `tools/validators/*.py` plugins, each exposing `check(registry) -> (files, problems)`, so each later ticket adds a file instead of editing the validator. Files: `tools/check_registry.py`, `tools/validate_data.py`, `tools/validators/`, `tools/test_check_registry.py`. Acceptance **TR-03**: the checker passes on the repository and fails on fixtures with a duplicate ticket id, a bare `CT-n`, an overlapping stream range and a reused ADR number; `validate_data.py` output on the repository is unchanged.
