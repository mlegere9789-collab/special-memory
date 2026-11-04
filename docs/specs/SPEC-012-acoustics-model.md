# SPEC-012: acoustics-model (sound propagation, acoustic shadow, aggregation, listeners)

> Shared namespaces (input tags and actors, random streams, flags, hash blocks, tick order, CT ids, amendment labels, ticket ids) are owned by [REGISTRY.md](REGISTRY.md); conflicts and their resolutions are in [CONFLICTS.md](CONFLICTS.md).

Crate: `crates/acoustics-model` (new; add to workspace `members`). Depends on `core-math`, `sim-state` (hasher only) and `battle-rules` (the shared integer-only TOML reader, `fx_ratio`, `FireRecord`; no other third-party crate), `proptest` (dev). `#![forbid(unsafe_code)]`, `#![deny(clippy::float_arithmetic)]` outside `#[cfg(test)]`. No audio device, clock or I/O.
Source key: **CWV pNNN** = "The American Civil War: Research Volume and Game Design", PDF page NNN, with the tag the page gives. Units follow SPEC-004 section 1 (mm, mm/s, permille, ticks at 20 Hz) plus, new here, `_cdb` (centibel, 0.01 dB), `_dc` (tenth of a degree C), `_mk_per_m` (millikelvin per metre), `_hz`, `_ms`. Invented numbers are `placeholder` with a calibration target CT-012-n (section 11).

## 1. Purpose and scope
Sound is a simulation (CWV p514, p446): a gun is heard after its smoke is seen (1 km is about 2.9 s at 343 m/s, CWV p457, arithmetic); thousands of shots become a bounded set of sources (CWV p457 Estimate); wind shear, temperature gradients, ground and woods create zones of audibility and silence, so a commander may not hear the battle that matters (acoustic shadow: Seven Pines, Perryville, CWV p457, V S16, S17; design use CWV p458, p514). The crate turns sim sound events into data events for a separate mixer and into per-headquarters hearing reports. **Out of scope:** mixing, reverb and echoes (CWV p457 echo probes), Doppler, music, what the command layer does with a report, smoke absorption (CWV p511 4.3).
The crate only reads sim output and never writes to `BattleState`. Results are deterministic integers outside `battle_hash`; F0 (SPEC-005 section 7) is untouched because nothing is culled from the simulation.

## 2. Definitions
Band: low 125 Hz, mid 500 Hz, high 2000 Hz (index 0..2). Level: `cdb` re 1 m, sound exposure level (one event, 1 s equivalent). Cluster: all events of one source row, one unit and one 100 m cell in one window. Window: `window_ticks` ticks ending at a tick with `tick % window_ticks == window_ticks - 1`. Along-wind `w`: wind vector dotted with the unit vector source to listener (positive blows toward the listener). `g`: sound-speed gradient with height, mm/s per metre.

## 3. Data (`data/rules/acoustics.toml`, schema `acoustics.schema.json`, `schema = "acoustics/1"`)
Integer-only TOML, unknown keys refused, provenance on every row (ADR 0004). Tables: `[globals]`, `[[band]]` (3), `[[source]]` (9), `[[ground]]` (per terrain class id, SPEC-004 section 5), `[[weather]]`, `[[budget]]`.
**Globals.** `c0_mm_s = 331300`, `c_per_k_mm_s = 606` (linear law, `partial`: CWV p457 gives 343 m/s, the law gives 343420 at 20 C); `shear_ppm = 70000` (near-ground wind gradient per metre as a share of the along-wind speed; placeholder CT-012-2); `g_min = 5`, `shadow_sat_g = 300`, `duct_loss_cdb_per_km = 100`, `barrier_cap_cdb = 2400`, `profile_step_mm = 20000`, `max_samples = 64`, `max_range_mm = 100000000`, `cluster_cell_mm = 100000`, `hyst_cdb = 300`, `drop_below_cdb = -1000`, `class_faint_cdb = 0`, `class_clear_cdb = 1000`, `class_loud_cdb = 3000`, `camera_window_ticks = 4`, `hq_window_ticks = 20`. Placeholders (CT-012-2, CT-012-5) unless stated.
**Bands** (placeholder CT-012-3; absorption magnitudes are textbook order of magnitude, `unverified`): `f_hz` 125/500/2000; `abs_cdb_per_km` 40/280/900; `ambient_cdb` 4000/3500/3000; `foliage_pm` 200/1000/2000; `shadow_slope_cdb_per_km` 2000/3000/4000; `shadow_cap_cdb` 4000/5000/6000.
**Sources** (`id, name, class, level_low/mid/high_cdb, priority_cdb`; placeholder CT-012-1; class 0 musketry, 1 artillery, 2 burst, 3 voice):
0 musket 10600/10800/10200; 1 carbine 10200/10400/9800; 2 revolver 9400/9600/9400 (class 0); 3 gun_launch 13000/12800/11600 (1); 4 burst_shell 12200/12200/11800; 5 burst_case 11400/11600/11400 (2); 6 command 7600/8400/8000, priority 1500; 7 cheer 7000/7800/7600; 8 cry 6600/7200/6800 (3). Priority is 0 unless stated.
**Ground** (`class_id, canopy_mm, foliage_cdb_per_100m`, placeholder CT-012-4): 2 orchard 6000/150; 3 woods 15000/600; 4 thicket 3000/900; all other classes 0/0.
**Weather presets** (`id, temp_dc, wind_x_mm_s, wind_y_mm_s, lapse_mk_per_m`, placeholder CT-012-2): `calm` 200,0,0,0; `calm_sunny` 200,0,0,-20; `toward_minus_x` 200,-6000,0,-20; `toward_plus_x_inversion` 200,4000,0,30.
**Budgets:** `max_total = 32`, class caps `[12, 10, 6, 8]` (placeholder CT-012-5; hundreds of voices real, thousands virtual, CWV p458 Estimate).

## 4. Inputs
`SoundEvent { seq: u64, source: u8, unit: u32, team: u8, pos: Vec2Fx (m), height_mm: u16, count: u16 (at least 1) }`; `unit < 2^24`. Order of arrival never matters. Positions convert by `to_mm(x) = floor(raw * 1000 / 2^32)`.
**SPEC-005 amendment A10 (required; REGISTRY section 9; the log is appended at tick steps TK-06 and TK-07 and cleared at the start of the step):** `BattleState` keeps `sound_log: Vec<FireRecord>`, cleared at the start of `step` and appended in ascending `seq` by section 7 phases 3 and 4: `FireRecord { kind: Shot | Burst, seq: u64, unit: u32, team: u8, ammo: u8, pos: Vec2Fx, z_mm: i32 }`. A misfire appends nothing (assumption). The log is excluded from `battle_hash`. Accessor `BattleState::sound_log(&self) -> &[FireRecord]`. `adapt_fire(&FireRecord, &AmmoTable) -> Option<SoundEvent>` maps: `ball`/`conical` to musket (carbine and revolver ammo ids to their rows), launch of `shot`/`shell`/`case`/`canister` to `gun_launch`, burst of `shell` to `burst_shell`, of `case` to `burst_case`, canister bursts none.
`Relief` (a read-only adapter built from the elevation source chosen by open decision D-02 in CONFLICTS; until then a scene-local table) `{ origin: Vec2Fx, cell_mm: i32, width: u32, height: u32, elev_mm: Vec<i32>, class: Vec<u8> }` (row-major, ascending; cell index `floor((p - origin) / cell_mm)` clamped to the grid). `Weather { temp_dc: i16, wind_mm_s: [i32; 2], lapse_mk_per_m: i16 }`; `Weather::from_sim(temp_dc, wind: Vec2Fx m/s, lapse)` converts by `to_mm`. `Listener { id: u32, kind, pos: Vec2Fx, height_mm: u16, ambient_cdb: [i32; 3] }`.

## 5. Integer math (`intmath`)
- `isqrt_u128(n)`: floor square root (Newton; the core-math one is crate-private).
- `log2_q16(x: u128) -> u32`, `x >= 1`: `n = 127 - leading_zeros`; `y = (x << (127 - n)) >> 95` (Q1.32 in `[2^32, 2^33)`); 16 times: `y = (y*y) >> 32`, and if `y >= 2^33` then `y >>= 1` and the bit is 1; result `(n << 16) | bits`. Error below 3 units of 2^-16.
- Powers are Q16: `P = 10^(L/10) * 2^16`. `cdb_of_power(p: u128) -> i32 = floor((log2_q16(p) - 2^20) * 301_029_996 / (65536 * 10^6))` (floor division; `p = 0` gives `-20000`). 10 log10 of a Q16 ratio is the same call.
- `power_of_cdb(l: i32) -> u128` for `l` in `-6000..=19000` (else clamped): `x = floor(l * 14_267_572_527 / (1000 * 65536))` (Q16.16, `log2(10)` in Q32), `n = x >> 16`, `f = x & 0xFFFF`, `i = f >> 8`, `r = f & 255`, `t = T[i] + (T[i+1] - T[i]) * r / 256`, result `t` shifted so that it equals `t * 2^(n + 16 - 62)`. Table `T[k] = floor(2^62 * 2^(k/256))`: `T[0] = 2^62`, `T[k+1] = (T[k] * C) >> 62` with `C = 4624189567668517720` (floor of `2^62 * 2^(1/256)`, the integer 256th root of `2^15873`), and `T[256] = 2^63`.
- Sums of levels add `power_of_cdb` values in `u128`, then `cdb_of_power`.

## 6. Propagation (`propagation`)
Per cluster and listener, with positions in mm, `d = isqrt(dx^2 + dy^2)` clamped to at least 1000, `c = c0 + c_per_k * temp_dc / 10` (truncating).
1. **Delay.** `w` from `(wx*dx + wy*dy) / d` (truncating, `i128`), clamped to `+-100000`; `c_eff = max(c + w, c/2)`; `delay_ms = ceil(d * 1000 / c_eff)`. The cluster uses `d_near = max(1000, d - extent/2)` for `arrival_ms = window_start_tick * 50 + delay_ms(d_near)` and `span_ms = window_ticks * 50 + ceil(extent * 1000 / c_eff)`, where `extent` is the larger of the cluster's x and y ranges (CWV p457: each cluster has its own delay and dispersion).
2. **Gradient.** `g = lapse_mk * 606 / 1000 + w * shear_ppm / 10^6` (each truncating). `w > 0` or `lapse > 0` bends sound down; the opposite bends it up (CWV p457: wind shear, inversion, ground reflection).
3. **Horizon.** If `|g| >= g_min`: `R = c * 1000 / |g|` (mm), `d_h = isqrt(2*R*hs) + isqrt(2*R*hr)` with `hs`, `hr` the source and listener heights in mm (limit of a circular ray; both signs). Else there is no refraction.
4. **Loss per band `b`** (cdB, all integer, non-negative):
 - spherical `G(x) = max(0, R((x * x) << 16) - 6000)` with `R(p)` the `10 log10` of the Q16 power `p` in cdB rounded to nearest (one rounded logarithm of the square, so exact powers of ten land on their value and the error stays below 0.6 cdB); if `g > 0` and `d > d_h` the spreading turns cylindrical: `G(d_h) + cdb_of_power((d << 16) / d_h) + duct_loss * (d - d_h) / 10^6`; else `G(d)`.
 - absorption `abs_b * d / 10^6`.
 - shadow, only if `g < 0` and `d > d_h`: `min(cap_b * min(|g|, shadow_sat_g) / shadow_sat_g, slope_b * (d - d_h) / 10^6)`. A weak gradient gives a weak shadow.
 - terrain, only with a `Relief` (items 4b and 4c).
 4b. **Barrier.** `n = clamp(ceil(d / profile_step), 1, max_samples)`; samples `i = 1..n-1` at `P_i = S + (L - S) * i / n` (truncating), `s_i = d * i / n`. Ground `e_i` is the relief elevation at `P_i`; the line is `line_i = zs + (zl - zs) * i / n` with `zs = elev(S) + hs`, `zl = elev(L) + hr`. The dominant edge is the sample with the largest `h = e_i - line_i` (ties to the smaller `i`); if `h > 0`: `E(a) = isqrt(((a*a + h*h) << 16))` (Q8 mm), `delta_q8 = E(s) + E(d - s) - E(d)` (floored at 0), `lambda = c / f_hz`, `N_q16 = delta_q8 * 512 / lambda`, loss `min(barrier_cap, cdb_of_power(3*65536 + 20*N_q16))` (Maekawa, a knife edge). Loss is 0 when `h <= 0`.
 4c. **Foliage.** Over all samples with `line_i - e_i < canopy(class_i)`: `sum(foliage_i * (d / n)) * foliage_pm_b / (100000 * 1000 * 1000)` where the sum is accumulated unscaled in `i128` and divided once.
5. Path result `PathResult { distance_mm, delay_ms, loss_cdb: [i32; 3], bearing: Angle }`. `bearing = Angle::atan2(dy, dx)` of listener to source centroid (metres via `fx_ratio(.., 1000)`), or zero when `d = 0`. Clusters beyond `max_range_mm` are skipped.

## 7. Aggregation (`aggregate`)
`Aggregator::new(window_ticks)`, `push(&SoundEvent)`, `flush(window_end_tick) -> Vec<Cluster>`. Key: `(source, unit, cx, cy)` with `cx = floor(x_mm / cluster_cell_mm)` (same for `y`, clamped to `i16`); `id = source << 56 | unit << 32 | (cx as u16) << 16 | (cy as u16)`. A cluster accumulates in `i64`/`u32` saturating: `count` (sum of `count`), sums of x, y, height, min and max of x and y, `team`, first `seq`. Only commutative operations are used, so the order of events cannot matter. `flush` sorts by `id` ascending and clears. Centroid and mean height use floor division. Cluster level per band: `level_b = SEL_b + cdb_of_power(count << 16) - cdb_of_power(window_ticks * 65536 / 20)`, capped at 18300; windows of different length stay comparable.

## 8. Listener pipelines (`listener`)
Two aggregators run: camera (window 4 ticks) and headquarters (window 20 ticks). For each flushed cluster set:
**Camera packet** `render_packet(&Listener, &mut ListenerState, &[Cluster], &Tables, &Weather, Option<&Relief>) -> MixPacket`:
1. For each cluster in ascending `id`: path (section 6), `lev_b = level_b - loss_b`, `loudness = max_b(lev_b - ambient_b)`; keep if `loudness >= drop_below_cdb`.
2. `score = loudness + priority_cdb + (hyst_cdb if id in state.prev else 0)`. Per class, take the highest scores (ties to the lower `id`) up to its cap, then at most `max_total` overall by the same order. `state.prev` becomes the selected ids, ascending.
3. Unselected kept clusters go to the bed: sector `((bearing as u64 + 2^28) >> 29) & 7`; per sector and band the power sum of `lev_b`; `count`, earliest `arrival_ms`, `span_ms` to the latest end.
4. `MixPacket { listener: u32, window_start_tick: u64, window_ticks: u16, sources: Vec<SourceOut>, bed: Vec<BedOut> }`. `SourceOut { id, source, count, arrival_ms, span_ms, distance_mm, bearing: Angle, level_cdb: [i32; 3] }` in ascending `id`; `BedOut { sector, level_cdb: [i32; 3], count, arrival_ms, span_ms }` ascending sector, only non-empty sectors.
**Hearing report** `hearing_report(&Listener, &[Cluster], ...) -> HearingReport { listener, tick, margin_cdb: i32, class: HearClass, loudest_sector: u8, audible_clusters: u32, earliest_arrival_ms: u64 }`: power sum of `lev_b` over all clusters in range (no budget, no drop threshold: many faint clusters add up), `margin_cdb = max_b(cdb_of_power(sum_b) - ambient_b)`; `Silent` below `class_faint_cdb`, `Faint` below `class_clear_cdb`, `Clear` below `class_loud_cdb`, else `Loud`; `loudest_sector` is the sector of the largest power; `audible_clusters` counts `loudness >= 0`; `earliest_arrival_ms` is over clusters with `loudness >= drop_below_cdb`. Headquarters `j` (ascending `id`) is evaluated on tick `window_end + 1 + (j % 20)` from the last completed window, spreading the cost.

## 9. Public API (exact)
`acoustics_model::{AcousticTables::from_toml_str, Weather, Relief, Listener, ListenerKind, ListenerState, SoundEvent, adapt_fire, Aggregator, Cluster, MixPacket, SourceOut, BedOut, HearingReport, HearClass, PathResult, render_packet, hearing_report, packet_hash, report_hash}`, `intmath::{isqrt_u128, log2_q16, cdb_of_power, power_of_cdb}`, `propagation::{sound_speed_mm_s, delay_ms, gradient, horizon_mm, path_result}`, `AcousticsEngine::{new, push, end_tick}` (owns both aggregators and states; `end_tick(tick, &[Listener]) -> TickOutput { packets, reports }`). `packet_hash` and `report_hash` are FNV-1a 64 over little-endian fixed-width fields in output order through `sim_state::StateHasher`.

## 10. Performance budget (hypothesis H12)
At most 128 clusters per window and 64 listeners. A path costs at most 5 microseconds; camera `end_tick` at most 1 ms per window; headquarters at most 2 ms mean and 6 ms p99 per tick after staggering; a 20,000-event volley aggregates in at most 3 ms. Hypothesis until T12-15 is recorded.

## 11. Acceptance tests (tests first; goldens recorded once from the stated reference)
- **T12-01 loader:** the file loads; float, unknown key and provenance violations are refused; `validate_data.py` agrees.
- **T12-02 intmath:** `isqrt_u128` exhaustive to 2^20 and a proptest of `r*r <= n < (r+1)^2` including `u128::MAX`; `log2_q16` within 3 units of `f64 log2` over 100,000 values; `cdb_of_power(2 << 16) == 301`, `cdb_of_power(65536) == 0`; `power_of_cdb(3000)` within 0.01 percent of `1000 << 16`; `cdb_of_power(power_of_cdb(l))` within 1 for every `l` in `0..=19000`;
- **T12-03 delay:** `c(200) = 343420`, `c(0) = 331300`, `c(-100) = 325240`; 1,000,000 mm gives 2912 ms calm, 2871 ms with `w = +5000`, 2955 with `w = -5000`; proptest against `f64` within 1 ms (1 m..100 km, -20..40 C, wind +-15 m/s).
- **T12-04 gradient and horizon:** `gradient(lapse -20, w -6000) = -432`; `horizon_mm(c 343420, g -100, hs = hr = 1500) = 203002`; `|g| < 5` gives `None`; shadow loss is 0 at `d_h`, non-decreasing beyond, never above the cap, scaled by `|g|/300` below saturation.
- **T12-05 loss terms:** flat, calm: loss within 1 cdB of `2000 * log10(d_m)` plus absorption at 100 distances from 1 m to 50 km; at 100 m the spherical term is 4000; mid-band absorption at 1,000,000 mm is 280; reference output `qc/fixtures/ref_acoustics.csv` from an independent `f64` script `qc/fixtures/ref_acoustics.py`, tolerance 2 cdB (also covers ducting and shadow).
- **T12-06 knife edge:** `d = 2,000,000` mm, `hs = hr = 1500`, flat relief (cell 10,000 mm) with one ridge cell (x index 100) at 41,500 mm elevation; losses are 1420, 1983, 2400 cdB within 3 (Maekawa, `f64` reference; the high band hits the cap). Without the ridge terrain loss is 0; a 500 m woods strip adds the section 6 foliage loss.
- **T12-07 aggregation invariance:** 20,000 events over 40 units in any shuffled order give identical clusters; `sum(count) == sum(event.count)`; centroids lie in the bounding box; `flush` is ascending by `id` and the cluster count never exceeds the distinct keys.
- **T12-08 cluster level:** 40 muskets in one window of 20 ticks: `level_b = SEL_b + 1602` within 1 cdB; window of 4 ticks adds 699 within 1.
- **T12-09 budgets and conservation:** 20,000 events from 100 units give `sources.len() <= 32`, per class at most the cap, ascending ids; the power sum of sources plus bed equals the power sum of kept clusters per band within 1 cdB.
- **T12-10 stability:** the same inputs give an identical packet and hash; two windows with scores differing by less than `hyst_cdb` keep the same selected set; a source dropping out and returning keeps its id.
- **T12-11 bands:** with shipped data the bands CB1 to CB6 of section 12 hold.
- **T12-12 terrain ordering:** ridges and woods never lower loss; an all-class-0 zero-elevation `Relief` equals no relief; with zero wind, swapping source and listener gives the same loss.
- **T12-13 determinism:** scenes `seven_pines`, `perryville`, `bombardment`, `ridge_pair`, `volley_20k` (fixed events in `scenes.rs`) hash to literals recorded once on Linux in `qc/golden-replays/expected-acoustics.json`, verified on three platforms and across processes;
- **T12-14 adapter:** over a short run `sound_log` shot records equal `shots_fired - misfires`, the log is empty after a tick with no fire, `battle_hash` ignores it, `adapt_fire` maps every ammo id to the stated row or `None`.
- **T12-15 perf (ignored test, prints mean, p99, max):** `volley_20k`, and 64 listeners with 128 clusters.

## 12. Calibration targets
CT-012-1 source levels (CB6 sets the order of magnitude). CT-012-2 weather presets, shear, gradient constants (CB2 to CB5). CT-012-3 band absorption and shadow slope and cap. CT-012-4 ground table. CT-012-5 budgets, hysteresis, thresholds (playtest, Q5). Bands (`partial` unless stated; they change only by amendment citing evidence, before the first playable-battle milestone, then freeze):

| band | scene | expectation | source |
|---|---|---|---|
| CB1 | 1000 m calm | delay 2900 to 2930 ms | CWV p457 arithmetic |
| CB2 | `seven_pines`: 6 clusters of 40 muskets, listener 3,218,688 mm (2 miles) | `toward_minus_x`: `Silent`; `calm`: `Clear` or `Loud` | CWV p457 V S16 (Johnston heard nothing at 2 miles) |
| CB3 | same scene | `toward_plus_x_inversion`: margin at least 0 at 8,046,720 and at 16,093,440 mm (5 and 10 miles) | CWV p457 V S16 (Richmond heard at 5 to 10 miles) |
| CB4 | `perryville`: 20 clusters of 40, listener 4,828,032 mm (3 miles) | `toward_minus_x`: `Silent` | CWV p457 V S17 |
| CB5 | `seven_pines`, `calm`, listener 16,093,440 mm | not `Clear` | CWV p457 V S16 (10 miles not heard at Gettysburg) |
| CB6 | `bombardment`: 3 guns per second in the window, 15,000,000 mm, `calm` | margin at least 0; `toward_minus_x` is `Silent` | CWV p446, plan 7.2; the reports of very great distances are `unverified` and not reproduced (cap 100 km) |

## 13. Assumptions
One linear gradient per path; straight-line terrain profile (no ray bending in the barrier test); misfires are silent; counts add as `10 log10 n`; weather is uniform and constant per scene.

## 14. Risks
R1 placeholder levels and shadow caps could mislead; a bad guess shows as a failing band. R2 one gradient hides layered atmospheres. R3 cost of 64 listeners (staggered). R4 Amendment A10 touches SPEC-005, and the new unit suffixes are listed in REGISTRY section 11 and recorded by ADR 0005. R5 unverified reports of distant hearing (Pittsburgh) are not reproduced.

## 15. Done when
fmt, clippy `-D warnings`, `cargo test --workspace` (also `--release`) and T12-01 to T12-15 pass; schema, data, goldens and `ref_acoustics.csv` committed and verified on three platforms; scans pass; public items documented; no floating point outside `#[cfg(test)]`; T12-15 numbers recorded in a cycle report.
