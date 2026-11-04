# SPEC-003: sim-cli, a headless harness and the determinism proof

> Shared namespaces (input tags and actors, random streams, flags, hash blocks, tick order, CT ids, amendment labels, ticket ids) are owned by [REGISTRY.md](REGISTRY.md); conflicts and their resolutions are in [CONFLICTS.md](CONFLICTS.md).

Crate: `shells/sim-cli` (binary `hd-sim`), depends on `core-math` and `sim-state` by path. Add it to the workspace `members`.
It runs a deliberately simple **toy scenario** so the determinism machinery is proven before any game rules exist.

## 1. Toy scenario (all arithmetic in `Fx`, all randomness from named streams)
- Arena `W x H` (default 2000 x 2000 metres, Fx). `K` agents (default 1000), two teams, positions chosen on a jittered grid using stream `Placement`.
- Per tick (`tick_hz` default 20): each agent's velocity is nudged by a vector drawn from stream `Wander` (bounded speed), position integrates by `vel / tick_hz`,
  reflects off arena walls, then the spatial grid is rebuilt, and every agent with a neighbour within a small radius gets its `hp` reduced by one with probability
  drawn from stream `Skirmish`; agents at zero hp despawn. Nothing here is a game rule; it exists to exercise movement, queries, randomness and despawn.
- Streams are separate ids so adding another system later cannot perturb these results.

## 2. Commands
- `hd-sim run --agents K --ticks N --seed S --checkpoint-interval C --out FILE`: runs, writes a replay (SPEC-002) with a checkpoint hash every C ticks.
- `hd-sim verify FILE`: re-runs from the replay header and compares every checkpoint and the final hash; exit code 0 on match, 1 on mismatch (print the first
  differing tick), 2 on a malformed file.
- `hd-sim hash --agents K --ticks N --seed S`: prints the final state hash (for scripts).
- `hd-sim bench --agents K --ticks N`: prints mean and p99 tick time in microseconds and total wall time (the only place wall-clock is allowed; never part of state).
- Argument parsing without external crates (small hand parser) or with `clap` pinned; either is fine. Errors are readable, exit codes as above.

## 3. Golden replays and the cross-platform proof
- Commit replays under `qc/golden-replays/`: `toy-small` (100 agents, 2000 ticks, seed 1), `toy-medium` (1000 agents, 2000 ticks, seed 7), `toy-large` (20000 agents, 600 ticks, seed 42),
  each with checkpoint interval 100, plus `qc/golden-replays/expected.json` listing file name and final hash.
- A test in the crate regenerates each golden from its parameters and asserts byte equality with the committed file, and that `verify` passes.
- CI runs `hd-sim verify` on the committed goldens on Linux, Windows and macOS. Generated once on Linux, verified everywhere. That is the determinism proof.

## 4. Tests
- Same seed gives byte-identical replays across two runs in one process and across two processes.
- Different seeds give different final hashes.
- `verify` fails (exit 1) when a single checkpoint hash byte in a replay is altered **and the body hash is recomputed**, and exits 2 for a corrupted body hash.
- Property: for random small parameters, `run` then `verify` always passes.
- `bench` smoke test (1000 agents, 50 ticks) runs and prints the expected fields.

## 5. Done when
fmt, clippy `-D warnings`, `cargo test --workspace`, `cargo build --release -p hd-sim`, then `hd-sim verify` passes on all committed goldens; `bench` for the large scenario prints
its numbers (record them in `docs/cycle-reports/` as the baseline for the 20,000-agent tick budget).

## Amendment A1 (from independent review, cycle 01)
`verify` and `run` enforce resource limits before running anything: `total_ticks <= 10_000_000`, `agents <= MAX_AGENTS` (the existing constant), and
`agents * total_ticks <= 20_000_000_000`; violations exit 2 with a readable message. A replay whose header has `checkpoint_interval == 0` while `total_ticks > 0` is malformed (exit 2).
A replay missing any scheduled checkpoint, or carrying extra ones, is malformed (exit 2). Boundary values of each limit are tested (value accepted, value plus one rejected).

## Amendment A2 (from T-084)
`hd-sim` gains the subcommand table of REGISTRY section 15 and `verify` dispatches on the replay header and the first input instead of assuming the toy scenario.
1. **Dispatch.** `verify` reads the header `scenario_id` and the inputs sorted by `(tick, actor, seq)`. `toy-v1` with its single setup input at `(0, 0, 0)` is the toy scenario of section 1. An input at `(0, 0xFF, 0)` whose payload starts with tag `0x20` (ScenarioBind, `u8 0x20, u64 scenario_hash`, exactly nine bytes) binds a data scenario: `data/scenarios/<scenario_id>.toml` is loaded with the rules in `data/rules`, and the replay is refused with exit 2 when the file is missing or invalid, when the stored hash differs from `scenario_hash` of the loaded scenario, or when the header `fidelity_class` differs from the scenario's. A bound replay carries no other input, a `tick_hz` of 20 and a checkpoint interval above 0; otherwise it is malformed (exit 2). Any other header is malformed (exit 2). The library-scenario binding of SPEC-004 and the `<scenario>/<variant>` binding of SPEC-011 are added by the tickets that own them.
2. **Re-run.** A bound replay is re-run with `Battle::run` from `Battle::from_scenario_with(scenario, seed, tables, fidelity)` and compared at every checkpoint and at the end exactly as in section 2 (exit 0 match, 1 mismatch with the first differing tick). The resource limits of amendment A1 apply with `agents` the smaller of the scenario's agent count (`men`, or `guns` for a battery, summed over non-headquarters units) and `agent_cap`.
3. **Data directory.** `verify` and `resolve` read `data/` relative to the working directory (the repository root, as `tools/verify_goldens.sh` runs them); `--data-dir DIR` replaces it.
4. **`resolve --scenario ID --seed S [--ticks N] [--out FILE] [--data-dir DIR]`.** Loads the scenario, runs `Battle::run(N)` (default `duration_s * tick_hz`; at most `MAX_TOTAL_TICKS`, with the limits of A1) and prints `key=value` lines: `scenario`, `seed`, `ticks`, `fidelity_class`, `final_hash` (16 hex digits), per side `ledger.<side>` (`present`, `killed`, `arrived`, `departed`, `held`, `spent`, `lost`, `initial`, `resupplied`), `casualties.<side>` (`killed`, `wounded`, `disabled`) and `objective.<id>` (`union`, `confederate` or `none`). With `--out` it writes a replay whose header is `(scenario_id = ID, seed, tick_hz 20, fidelity_class = fc, checkpoint interval 100)`, whose only input is `(tick 0, actor 0xFF, seq 0)` with the ScenarioBind payload, and which carries a checkpoint hash every 100 ticks and the final hash. `Battle::hash` is the hash of every checkpoint.
5. **Tests.** `verify` of a `resolve` replay exits 0; a different header class or a different stored scenario hash exits 2; a changed checkpoint hash with a recomputed body hash exits 1; the toy goldens verify unchanged.
