# ADR 0004: Rules data format

**Status:** accepted.

**Decision.** Rules data (`data/rules/*.toml`) are integer-only TOML. Every number is an integer in a named unit (`_mm`, `_mm_s`, `_pm`, `_ppm`, `_ticks`);
a TOML float is refused by the loader and by `tools/validate_data.py`. Integers become `Fx` only through `fx_ratio`. Unknown keys are refused. Every row carries
provenance: a placeholder names a calibration target (`CT-<spec>-<n>`, ADR 0011), any other row lists sources, and `verified` needs a `second_source`.

The `crates/battle-rules` loader depends on `toml` and `serde`, each pinned with an exact version (`=1.2.0` and `=1.0.229`) and resolved through the committed
`Cargo.lock`, which CI builds with `--locked`. The `toml` parser feeds a serde visitor that keeps no floating-point payload, so no float value reaches a loader
output type. The loader is the only code that uses these two crates: no other third-party dependency is allowed in the simulation path, and a version change is a
deliberate ticket with the golden hashes re-verified.

**Why.** Floating point differs across compilers and optimisation levels (ADR 0001), so tuning data must not be able to introduce it. A single shared reader makes the
float, unknown-key and provenance rules identical for every table. Exact pins keep parsing, and therefore the loaded tables and replay hashes, reproducible.

**Consequences.** Schemas live in `data/schemas/` (`rules-common.schema.json` holds the shared provenance definition). The Rust loader and the validator enforce the same
rules (test T4-01). Fractions are written as integers (permille, parts per million), never as decimals.
