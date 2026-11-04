# CYCLE-command: SPEC-007 command bands, golden and performance (T-076)

Seeds 1 to 64 for every band; release build; Linux. All twelve rows are placeholders (`unverified` or
`partial`, never `verified`). No band, threshold or data row was changed to reach this result: the
rows of `data/rules/command.toml` are as merged by T-071.

## Bands (`qc/calibration/command-bands.toml`, `run_command_bands`)

Times are minutes; friction is per 100 legs; shares and confidences are fractions.

| Band | Target | Quantity | Mean or median | Min | Max | Band | Verdict |
|---|---|---|---|---|---|---|---|
| C1 | CT-007-1 | courier, 3 km, delivery | 19.484 | 19.406 | 19.568 | 8 to 40 | pass |
| C2 | CT-007-1 | courier relay, two 3 km hops | 39.733 | 39.544 | 39.891 | at most 70 | pass |
| C3 | CT-007-1 | telegraph delivery | 0.979 | 0.901 | 1.063 | at most 3 | pass |
| C4 | CT-007-1 | flags, 5 km | 1.179 | 1.101 | 1.263 | at most 6 | pass |
| C5 | CT-007-2 | lost per 100 legs of 3 km | 1.562 | 0 | 5 | 1 to 5 | pass |
| C6 | CT-007-2 | garbled per 100 legs of 3 km | 3.000 | 0 | 7 | 2 to 6 | pass |
| C7 | CT-007-3 | tired low-discretion declines | 1.000 | 1.000 | 1.000 | at least 0.70 | pass |
| C8 | CT-007-3 | rested neutral goes in | 1.000 | 1.000 | 1.000 | at least 0.90 | pass |
| C9 | CT-007-6 | army to regiment, four hops, median | 31.594 | 31.350 | 31.760 | 10 to 45 | pass |
| C10 | CT-007-6 | division to regiments within 500 m, latest | 4.332 | 4.167 | 4.514 | at most 10 | pass |
| C11 | CT-007-7 | sighting to army picture, median | 14.760 | 14.669 | 16.825 | 4 to 30 | pass |
| C12 | CT-007-7 | confidence at 1.5 km | 0.167 | 0.167 | 0.167 | under 0.70 | pass |

Calibration table (placeholder data rows changed): none. Old, new and why are therefore empty.

## How each quantity is measured

- Delivery (C1 to C4): two officers with fixed headquarters, one order sent at tick 0; the tick the order is
  read. Seeds where the courier is lost are left out of the mean. The relay is three officers in a line.
- Friction (C5, C6): 100 move orders sent by courier at tick 0 over one 3 km leg; the share lost and the share
  whose record shows a garble.
- Interpretation (C7, C8): `cemetery_hill` with loss and garbling off, so only the go score is measured; the
  formation has 210 men, the observed contact 100 men at quality 900 (odds 700 permille before noise).
- Chain (C9, C10): `command_tree` without links, loss and garbling off; a strict move; the time each regiment's
  order becomes the move. C9 is the median over the 27 regiments per seed and then the median over seeds.
- Observation (C11, C12): `command_tree` with its two links, real friction. C11 is the median over seeds of the
  first tick the army picture holds a contact; C12 the confidence of the nearest regiment's own sighting at the
  first observation, 1.5 km from the enemy.

## Findings

- Observation over a courier-only tree gave no value in any seed (the first run of C11 without links). The
  likely cause, from the rows and not instrumented: the army headquarters stands 3 km from the corps, the courier
  leg takes about 19 minutes, and a contact of quality 0.167 falls below `drop_pm` after about 14 minutes
  (`stale_ticks` 24000), so the report is ignored on arrival. With the telegraph the band holds (C11). This is a consequence
  of the placeholder rows `stale_ticks`, `drop_pm` and the courier speed together with the scenario geometry; it
  is recorded here and no row was changed.
- C7 and C8 show 1.000 for every seed: at these odds the go score is about 2.5 and 2.2 standard deviations from
  the threshold, so a miss within 64 seeds is possible but not expected.

## Golden

`command_chain` (27 regiments, one telegraph and one flag link, a discretionary move at tick 0), tick 6000,
seed 7: `413d05a4b1fbf3d1`, in `qc/golden-replays/expected-command.json`, checked in-process, in a second
process and through `hd-sim battle --scenario command_chain --ticks 6000 --seed 7`.

## Performance (T7-17, ignored, provisional)

330 officers, 270 own and 10 enemy regiments of 20 men, 329 messages in flight at tick 0 (peak 329), 2000 ticks
of `step_command`, release, one thread:

| Mean | p99 | Max | Hypothesis H7 |
|---|---|---|---|
| 0.027 ms | 0.222 ms | 8.126 ms | mean at most 0.3 ms, p99 at most 2 ms |

The maximum is the first tick, which reads the 329 messages' first legs.

## Deviations

- The golden literal is in `expected-command.json`, not `expected.json`: the `hd-sim` golden test renders
  `expected.json` byte for byte from its own list, so a row there would change an existing test (as for
  `expected-f0.json`).
- `hd-sim verify` on three platforms applies to `.hdrp` replays; `battle` writes none, so the cross-platform
  proof of this literal is the same-hash check in the test suite on each CI platform.
- `run_command_bands` lives in `command/calibration.rs` (the ticket's file) and is re-exported as
  `calibration::run_command_bands`.
