# CYCLE-03 F0: calibration harness, reference curves, determinism and cost (T-024)

Recorded 2026-11-04 on a Linux x86_64 cloud container, 4 vCPU shared with other jobs (load average above 15 during the
runs), release profile. Specification: SPEC-005 sections 9, 11 (T5-19 to T5-21) and 12. All data rows are still
placeholders (CT-005-1 to CT-005-9). The first measurement, with the rows as authored, missed eight bands; the rows listed
under "Calibration" were then fitted, one band group at a time, and the suite passes B1 to B11. No band and no test
threshold was changed.

## Bands B1 to B11 (`run_band_suite("fire")`, seeds 1 to 8, `qc/calibration/bands.toml`), after calibration

Means over the seeds with the per-seed minimum and maximum. Casualties are killed, wounded and disabled per 100 men of
both lines (the log of `BattleState::casualties`).

| Band | Point | Mean | Min | Max | Band | Result |
|---|---|---:|---:|---:|---|---|
| B1 rifle | 45 m | 43.7 | 37.8 | 48.5 | 20 to 60 | pass |
| B1 | 91 m | 27.0 | 24.0 | 29.0 | 15 to 50 | pass |
| B1 | 137 m | 18.1 | 14.3 | 20.3 | 10 to 40 | pass |
| B1 | 182 m | 14.1 | 11.8 | 15.5 | 6 to 30 | pass |
| B1 | 274 m | 2.75 | 1.75 | 4.75 | 2 to 15 | pass |
| B1 | 365 m | 1.41 | 0.25 | 3.00 | 0.5 to 8 | pass |
| B2 smoothbore | 45 m | 36.4 | 32.8 | 38.8 | 8 to 40 | pass |
| B2 | 91 m | 14.3 | 12.0 | 16.3 | 2 to 15 | pass |
| B2 | 137 m | 4.41 | 2.50 | 5.75 | 0 to 5 | pass |
| B2 | 182 m | 0.44 | 0.00 | 1.00 | 0 to 2 | pass |
| B3 rifle / smoothbore | 182 m | 32.2 | 11.8 | 1000 (cap) | at least 3 | pass |
| B4 rises with range | B1, B2 | 0 | 0 | 0 | 0 | pass |
| B5 smoothbore rounds/min | trained | 2.410 | 2.403 | 2.416 | 2.3 to 2.7 | pass |
| B5 | raw | 2.037 | 2.033 | 2.043 | 1.9 to 2.2 | pass |
| B5 | veteran | 2.890 | 2.878 | 2.901 | 2.8 to 3.2 | pass |
| B6 gun rounds/min | trained | 0.968 | 0.950 | 0.983 | 0.9 to 1.1 | pass |
| B7 range to first ground contact | shot 5 deg | 1610 m | | | 1382 to 1690 | pass |
| B7 | shell 3.75 deg | 1270 m | | | 1070 to 1308 | pass |
| B7 | case 3.75 deg | 1057 m | | | 934 to 1142 | pass |
| B8 canister casualties/round | 183 m | 3.94 | 3.80 | 4.28 | 3 to 12 | pass |
| B8 | 320 m | 0.86 | 0.36 | 1.56 | 0 to 1.97 | pass |
| B8 | 366 m | 0.21 | 0.12 | 0.32 | 0 to 1.18 | pass |
| B8 | 457 m | 0.00 | 0.00 | 0.00 | 0 to 0.39 | pass |
| B9 wall, attacker / defender | 91 m | 2.08 | 1.64 | 2.74 | at least 2.0 | pass |
| B10 killed share of killed + wounded | all runs | 0.242 | 0.000 | 0.394 | 0.15 to 0.30 | pass |
| B11 friendly hits / hits | all runs | 0 | 0 | 0 | 0 to 0.02 | pass |

Margins are thin at B2 91 m and 137 m, B1 274 m and B9; the means are exact for the fixed seeds.

### The lethality finding (before calibration)

The flag raised by T-023 was confirmed and measured. With the rows as authored, both lines of 200 at 91 m (rifle,
trained, seed 1) lost 385 of 400 men in 2400 ticks (381 distinct men), 240 of them in ticks 300 to 499, the first volley.
The causes found in the rows: a musket ball (1345 J at 91 m) kept 745 J after one body (absorption 600 J, continuation
limit 150 J) and so pierced several bodies of the two dense ranks; nearly every body hit was a casualty (band 2 and 3
cells sum to 90 to 100 percent); the aim error (6000 urad) left almost every shot inside the vertical extent of a line of
standing men. After calibration the first-volley curve at 91 m (cumulative casualties per 100 men before tick 300, 600,
...) is 0, 8.0, 8.9, 14.5, 20.0, 20.8, 26.6, 27.0 (`f0-curves.json`).

## Calibration (data rows only)

Order followed the targets of SPEC-005 section 12: gun ammunition (CT-005-2, B7), then dispersion and aim (CT-005-3,
CT-005-7, B1, B2), damage and absorption (CT-005-6, B1, B2, B10), canister (CT-005-2, B8), then the wall check (B9) by the
rifle aim row. Every changed row stays `placeholder = true` with its CT id and a note saying what it was fitted to.

| File, row, key | Old | New | Band | Why the new value is plausible |
|---|---:|---:|---|---|
| ammo `shot_12` `cd_pm` | 470 | 530 | B7 | iron shot with windage and a rough surface drags more than a smooth sphere; 1610 m is inside the band |
| ammo `case_12` `muzzle_mm_s` | 400000 | 350000 | B7 | a case round carries its balls in a thin tin canister with a lighter charge; range 1057 m |
| ammo `canister_12` burst `spread_sigma_urad` | 60000 | 10000 | B8 | canister leaves the muzzle as a tight cone of a few degrees; 60 mrad sent 92 percent of the balls over the line |
| ammo `canister_12` burst `pellet_cd_pm` | 470 | 900 | B8 | the cloud of balls slows faster than single spheres (wake, ricochet loss); the casualty rate falls off with range as the band requires |
| weapons `rifle_58` `aim_scale_pm` | 1000 | 5000 | B1, B9 | volley fire by a line under stress has a vertical error of metres at 100 m |
| weapons `rifle_58` `sigma_slope_urad_per_100m` | 250 | 5000 | B1 | spread grows with range from handling and powder variation |
| weapons `smoothbore_69` `aim_scale_pm` | 1000 | 5500 | B2 | a smoothbore ball is less accurate than a rifle ball |
| weapons `smoothbore_69` `sigma0_urad` | 6000 | 22000 | B2 | the ball fits loosely in the bore (windage) |
| weapons `smoothbore_69` `sigma_slope_urad_per_100m` | 4000 | 45000 | B2, B3 | the smoothbore ball is ineffective beyond about 100 m; the dispersion formula caps the spread at 46 mrad |
| damage `[absorb]` `ball_j`, `conical_j` | 600 | 1100 | B1, B2 | a musket ball stops in the first body it strikes at ordinary ranges |
| damage `[energy_bands]` `b2_min_j` | 500 | 800 | B2, B1 | a smoothbore ball beyond 100 m (under 800 J) is a spent ball |
| damage outcome band 0, legs / torso / head (killed, wounded, disabled per mille) | 0/50/200, 20/150/300, 150/350/300 | 0/10/40, 5/40/100, 30/110/100 | B1, B10 | spent balls mostly bruise |
| damage outcome band 1 | 10/500/300, 150/600/200, 500/350/100 | 0/24/36, 6/48/60, 36/72/48 | B2, B10 | a weakened ball mostly bruises; killed share kept near 0.2 |
| damage outcome band 2 | 50/800/100, 300/650/40, 800/180/10 | 12/160/120, 72/240/120, 200/200/40 | B1, B2, B10 | many hits glance, lodge in clothing or equipment, or cause no lasting loss |
| damage outcome band 3 | 300/650/40, 650/330/10, 950/50/0 | 60/260/100, 150/450/100, 350/300/30 | B10 | the same, for the highest energies |
| `qc/fixtures/ref_flight.csv` | 18 rows of `shot_12`, `case_12` | regenerated | T5-05 | derived output of `ref_flight.py` from the changed ammo rows; other rows identical |

The dispersion formula (`sigma_urad`) limits `sqrt(sigma_b^2 + sigma_s^2)` to 46,340 urad because the sum of squares is
held in an `i32`; the smoothbore rows reach that cap beyond about 90 m, so the B2 shape at 91 m to 137 m comes from the
energy band, not from more spread. No band was unreachable.

## Tests whose pinned literals follow the amended rows (SPEC-005 amendment A18)

Only the literal numbers changed; every assertion, tolerance and ordering check is kept.

- `t5_01_tables.rs`: canister spread 60000 to 10000; rifle dispersion `(700, 250)` to `(700, 5000)`; band 2 torso cell
  `(300, 650, 40)` to `(72, 240, 120)`; band 3 head cell `(950, 50, 0)` to `(350, 300, 30)`; ball and conical absorption
  600 to 1100; the float mutation of the first weapon row `sigma0_urad = 6000` to `22000`; the over-1000 mutation, which
  needs a cell of the new table (`350/300/30` to `350/300/351`, still the message with 1001).
- `t5_10_damage.rs`: band 2 legs expected counts 10,000 / 160,000 / 20,000 to 2,400 / 32,000 / 24,000 (same 1000-roll tolerance).
- `t5_damage_lookup.rs`: band 0 legs boundaries 50,000 / 250,000 to 10,000 / 50,000; the band edges 499.999 J and 500 J to
  799.999 J and 800 J; the no-effect test of a cell summing to 1000 builds that cell by mutating the table text, because no
  cell of the calibrated table sums to 1000. `lethality_rises_with_energy_in_the_data` is unchanged; it required the band 0
  head kill share to be lowered (40 to 30 permille) so that kills rise with energy.

## T5-20 determinism (`crates/battle-rules/tests/t5_20_determinism.rs`)

| Scenario | Tick | Seed | Hash |
|---|---:|---:|---|
| cal_line(rifle_58, trained, 91440, dry) | 2400 | 1 | 11f34cade0460ac4 |
| cal_canister(183000) | 1200 | 1 | 16912c8c2c6c08cf |
| f0_large (20,000 soldiers) | 600 | 42 | c1d9af2c3bbe283e |

The literals are in `qc/golden-replays/expected-f0.json` (see the deviation below), checked in one process (twice) and
in a child process, debug and release (`f0_large` in release only).

## T5-21 performance (`f0_large`, 20,000 soldiers, all at will, `BattleState::step`, 600 ticks)

| Run | Mean (us) | p99 (us) | Max (us) | Shots | Peak in flight |
|---:|---:|---:|---:|---:|---:|
| 1 | 14789 | 27252 | 41898 | 23814 | 1152 |
| 2 | 13429 | 23265 | 25701 | 23814 | 1152 |

H5 (whole tick at most 30 ms mean and 45 ms p99): met, with the whole tick (movement included) at 13.4 to 14.8 ms mean and
23 to 27 ms p99 (the 42 ms maximum of run 1 is one tick on a shared machine). The scenario averages 40 shots per tick
against H5's 42 and peaks at 1152 projectiles in flight against 1,250. One-tick volley of all 20,000 (`f0_volley`, every
soldier ready): 57 to 62 ms for the volley tick, against the limit of 250 ms; the following tick, with about 19,200
projectiles in flight, took 46 to 51 ms. The machine was shared; the figures are an upper bound.

## Deviations and readings of the specification

- `f0_large` is 25 regiments of 400 in line facing 25 regiments of 400, 200 m apart, all armed with `rifle_58` and set to
  fire at the regiment opposite. Each soldier's place in the loading cycle is drawn once from `STREAM_BATTLE_SETUP`.
- The T5-20 literals are in `qc/golden-replays/expected-f0.json`, not `expected.json`: `hd-sim`'s golden test renders
  `expected.json` from its own list byte for byte, so extra rows there would change an existing test. Replay files
  (`.hdrp`) for the battle scenarios need the `hd-sim battle` subcommand and are not part of this ticket.
- `cal_range` is a single round flown by `step_projectile` from a gun muzzle (1000 mm) to its first ground contact; shot,
  shell and case are measured with no fuse.
- Band values are integers times the band's `scale`; `qc/calibration/bands.toml` carries the tags of SPEC-005 section 9.
