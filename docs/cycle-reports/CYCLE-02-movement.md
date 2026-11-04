# CYCLE-02 movement: steering, avoidance, anchor movement (T-019)

Recorded 2026-11-04 on a Linux x86_64 cloud container, 4 vCPU, release profile for timings. Specification:
SPEC-004 sections 7, 9, 12, 13 and Amendment A2. All data rows are still placeholders (CT-1 to CT-5).

## T4-03 march (`march_single_at`, 200 m, 400 men in column, trained)

| Gait | Arrival tick | Reference model (`ref_march.csv`) | Ideal `ceil(d * 20 / v)` | Over ideal | Largest slot error (m) |
|---|---:|---:|---:|---:|---:|
| walk | 3757 | 3757 | 3749 | 0.2 % | 0.16 |
| quick | 3096 | 3096 | 3070 | 0.8 % | 0.25 |
| double_quick | 1779 | 1779 | 1736 | 2.5 % | 0.83 |
| run | 1848 | 1848 | 1112 | 66 % | 2.06 |

The ideal formula ignores fatigue; at run the soldiers reach 1,000,000 ppm after 45 s and slow to 40 percent of
the gait speed, so the 200 m march takes 1.66 times the ideal. The limit `max_lag_mm` is 6 m. See Amendment A2.

## T4-04 terrain (`terrain_strip`, 40 m woods strip, quick)

Ticks the anchor spends inside the strip: 1031. Formula `40 / (1.303 * 0.6) * 20`: 1024 (0.7 % apart). Reference
model with woods effort and fatigue: 1034 (0.3 % apart). The anchor stays at woods speed until the last rank
has left the strip, because the anchor speed is the minimum over the regiment.

## T4-06 acceleration and T4-08 settle (`deploy_pair`)

Largest velocity change in one tick over 2000 ticks: 644245094 raw, the limit `accel / 20` is 644245094 raw
(plus one ulp allowed). Both regiments are within `settle_tol_mm` of their slots when the transition ends:
raw column to line at tick 1560, veteran at tick 900; the budget allows 600 further ticks.

## T4-11 avoidance

- Coincident pair (slots 600 mm apart): 500 mm apart at tick 12 (limit 40).
- Two 100-man single files marching through each other: closest cross pair 0.00001 m during the pass,
  0.45 m after both regiments have held for 400 ticks (limit 0.4 m).
- `desired_velocity` in descending order equals the ascending order while the files overlap.

## T4-12 determinism

| Scenario | Tick | Seed | Hash |
|---|---:|---:|---|
| march_single | 4000 | 1 | 49a618015d2d9397 |
| deploy_pair | 3000 | 2 | bd8a5938d0a77d43 |
| march_large | 600 | 42 | cac6ac5cd49ef04b |

The literals are checked in `crates/battle-rules/tests/t4_12_determinism.rs`, in one process and in a child
process (debug and release). The `hd-sim battle` subcommand and its replay goldens are not part of this ticket.

## T4-13 performance (`march_large`, 20,000 soldiers, `step_movement`, 600 ticks)

| Run | Mean (us) | p99 (us) | Max (us) |
|---:|---:|---:|---:|
| 1 | 12322 | 20730 | 21363 |
| 2 | 12223 | 21046 | 23591 |

Hypothesis H4 (mean at most 10 ms, p99 at most 18 ms) is not met: the mean is 22 percent and the p99 about 16
percent over. With 8 m grid cells the personal-space query (`SpatialGrid::query_radius`, which allocates a
result list per call) was 7 ms of a 15.4 ms mean; the scenarios now use 2 m cells (a 1 m cell costs 7 ms more
through the rebuild). Candidates for the next measurement: a non-allocating radius query and a cheaper
terrain class lookup.
