# CYCLE-acoustics: SPEC-012 performance against H12 (T-127)

Release build; Linux, 4 virtual cores, Intel Xeon at 2.10 GHz; single thread; shipped
`data/rules/acoustics.toml`. Command:
`cargo test --release -p acoustics-model --test t12_15_perf -- --ignored --nocapture --test-threads=1`.
The numbers are provisional: one shared machine, three runs, no pinning. They are to be repeated on the
reference hardware of the performance gate before H12 is called confirmed. No limit, data row or test was
changed to obtain them.

## Results (microseconds unless stated; ranges are the three runs)

| Quantity | Scene | Mean | p99 | Max | H12 limit | Verdict |
|---|---|---|---|---|---|---|
| One path, ns | 128 clusters, calm_sunny, no relief | 294 to 374 | 480 to 621 | 691 to 960 | at most 5 us (5000 ns) | pass |
| Camera packet, one listener | 128 clusters | 57 to 75 | 79 to 107 | 80 to 427 | at most 1000 per window | pass |
| Headquarters, reports due on one tick | 64 listeners staggered over 20 ticks, 128 clusters | 172 to 221 | 229 to 291 | 252 to 296 | mean 2000, p99 6000 | pass |
| Aggregate and flush | `volley_20k` (20,000 events, 40 units) | 965 to 1047 | 1057 to 1503 | 1057 to 1503 | at most 3000 | pass |

Samples: 200 paths-per-listener loops, 200 camera packets, 400 staggered ticks, 50 volleys.
An earlier run on the same machine under load showed a single staggered tick at 2.5 to 3.5 ms (max), so the
maximum of the headquarters tick is noisy; its mean and p99 are the figures H12 constrains.

## How each quantity is measured

- Path: `path_result` for one listener against the 128 cluster centroids, total time divided by 128.
- Camera: `render_packet` for one camera listener over 128 clusters (16 units, 8 cells each, count 20).
- Headquarters: per tick, the `hearing_report` calls of the listeners whose turn it is (every 20th of 64),
  which is what `AcousticsEngine::end_tick` does with the stagger of SPEC-012 section 8.
- Volley: `Aggregator::push` of every event of `scenes::volley_20k()` then `flush`.

## Reading

Every quantity is at most about half of its limit on this machine, so no tuning of the model was needed. The
assertions in `t12_15_perf.rs` use the H12 numbers (the volley against its maximum, the other rows against
mean or p99) and are in an ignored test so noisy shared runners cannot fail a merge.
