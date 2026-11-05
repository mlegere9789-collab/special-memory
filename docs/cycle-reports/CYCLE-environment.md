# CYCLE-environment: SPEC-009 bands, goldens and performance (T-097)

Seeds 1 to 8 for every band; release build; Linux x86_64 cloud container, load average about 10 during the runs.
All rows involved are placeholders (CT-009-1 to CT-009-6). No band, threshold or expected range was changed; one data
row and one scenario parameter were (SPEC-009 amendment A2).

## Bands E1 to E8 (`run_band_suite("env")`, `qc/calibration/bands.toml`), after calibration

Transmissions are permille, E3 is seconds, E4 to E8 are ratios and shares (per seed minimum and maximum next to the mean).

| Band | Target | Quantity | Mean | Min | Max | Band | Result |
|---|---|---|---:|---:|---:|---|---|
| E1 | CT-009-1 | transmit between 91 m lines at tick 1200, calm | 15 | 13 | 20 | 0 to 150 | pass |
| E2 | CT-009-1 | the same, 3 m/s crosswind | 356 | 336 | 369 | 200 to 900 | pass |
| E3 | CT-009-1 | seconds to recover transmit 500 after ceasing fire | 62 | 62 | 64 | 30 to 180 | pass |
| E4 | CT-009-1 | casualties with smoke over without, rifle 183 m | 0.760 | 0.677 | 0.865 | 0.3 to 0.9 | pass |
| E5 | CT-009-4 | fatigue after 900 s at quick, 14:00 day 2 over 04:30 | 1.321 | 1.321 | 1.321 | 1.1 to 1.5 | pass |
| E6 | CT-009-5 | misfire at full damp over dry | 3.000 | 3.000 | 3.000 | 2.5 to 3.5 | pass |
| E7 | CT-009-3 | share of `crest` men with a shot after 60 s | 1.000 | 1.000 | 1.000 | 0.8 to 1.0 | pass |
| E8 | CT-009-3 | share of clear-line enemy regiments the hill logs | 1.000 | 1.000 | 1.000 | 0.7 to 1.0 | pass |

## Calibration (placeholder data rows changed)

| Row | Old | New | Band | Target |
|---|---|---|---|---|
| `environment.toml` `[smoke]` `yield_pm` | 550 | 220 | E2 | CT-009-1 |

First measurement with the rows as authored: E1 3, E2 17 (miss), E3 73 s, E4 0.332, E6 3.000, E8 1.000 (E5 at 1 h 1.000, miss; E7 as
below). The yield was tried at 300 (E2 about 190, miss), 250 (about 290) and 220 (about 355); 220 keeps E1, E3 and E4
inside their bands with margin. Tests that pinned the old value were re-pinned to exactly the new one: `t9_01_loader`
(the loaded value and the float-rejection edit), `t9_06_schedule` (the expected density), `t9_04_conservation` (the
expected densities 858 and 214 and, to keep the saturation case exercised, a test-local cap of 1000 instead of 3000).

## Scenario parameter: crest height (E7)

Crest of 3 m, as the spec table reads: no man fires in 60 s (0 of 30 in every seed). The eye is 1.7 m above the
shooter's ground and the aim point 1.02 m, so a ray cannot clear a crest taller than the shooter's rise (sight
formula, `peek_max_mm` 15 m). Heights tried on seeds 1 to 8: 1.5 m, 30 of 30 fire in every seed; 1.6 m, 15 to 18 of 30;
1.7 m and above, none. `crest` uses 1.5 m, the tallest crest inside the band (amendment A2).

## E5 window (amendment A2)

As first written E5 read fatigue after 1 h at quick. The SPEC-004 quick drain is 28 ppm per tick (CT-004-2: half at 900 s,
tested by T4-05), which saturates fatigue at 1,000,000 after about 1790 s cool and 1350 s hot, so at 1 h the ratio was 1.000
(1,000,000 over 1,000,000, every seed) for any weather and measured nothing about heat. Ratios at other lengths
(14:00 over 04:30, seed 1): 900 s 1.321 (666,000 over 504,000), 1200 s 1.315, 1800 s 1.000. The quantity E5 tests is the heat multiplier
on drain, visible only before saturation, so amendment A2 sets the window to 900 s; the band limits (1100 to 1500 permille),
SPEC-004 and CT-004-2 are unchanged. Open follow-up: when CT-004-2 is replaced by sourced data, the original 1 h window
must be revisited.

## Goldens (`qc/golden-replays/expected-env.json`)

| Scenario | Tick | Seed | Hash |
|---|---:|---:|---|
| `env_smoke_line` | 2400 | 1 | `a477372036e44c0b` |
| `env_hill_hq` | 1200 | 2 | `49f929a0a3d19c6a` |
| `env_heat_march` | 4000 | 3 | `a3cc5f8d9a051b8c` |

Checked in process and in a second process. No existing golden changed: every earlier golden runs with `env = None`
or without smoke, and `yield_pm` is read only with `env` `Some`.

## Performance (T9-18, ignored, provisional)

Release, one thread, measured while five other builds ran on the same machine, so every figure is provisional. `f0_large` with the environment on against the same battle with it off,
1200 ticks, and the full 750 by 625 smoke update.

| Measure | Mean | p99 | Max |
|---|---:|---:|---:|
| `f0_large` step, environment on | 60.0 ms | 173.3 ms | 234.2 ms |
| `f0_large` step, environment off | 10.5 ms | 16.5 ms | 27.3 ms |
| full-grid smoke update, 750 x 625 | 11.8 ms | 20.2 ms | 20.2 ms |

H9 asks for at most 2 ms mean and 6 ms p99 added. The loaded machine cannot confirm the figure and the ignored test stays ignored; the added mean
(about 50 ms under load) is far over the budget; the full-grid update is inside its 12 ms. Hypothesis H9 is not altered and stays a hypothesis until a quiet-machine run.

## Deviations

- Goldens are in `expected-env.json`, not `expected.json`: the `hd-sim` golden test renders `expected.json` byte for
  byte from its own list (as for `expected-f0.json` and `expected-command.json`).
- `hd-sim battle` does not list the environment scenarios (shells are outside this ticket); the cross-platform proof is the
  in-suite hash check on each CI platform.
- E5 uses a 100-man column.
