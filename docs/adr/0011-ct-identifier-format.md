# ADR 0011: Shared identifiers, calibration target format and unit suffixes

**Status:** accepted, 2026-11-04. Source: `docs/specs/REGISTRY.md` sections 7 and 11 (ticket T-025). The registry first assigned this
decision the number 0005; that number is the supply-chain ADR, so the registry now lists this decision as 0011.

**Decision.**

1. A calibration target is written `CT-<spec>-<n>`: the three-digit number of the owning spec, a dash, and the spec's own local
   number (`CT-004-1`, `CT-005-9`, `CT-011-1101`). Local numbers are kept, so only a prefix is added. The pattern is
   `^CT-[0-9]{3}-[0-9]+$` in `rules-common.schema.json` and in the `ammo`, `damage`, `drill` and `weapons` schemas that copy it
   (the `locomotion` and `terrain` schemas reference the common definition).
2. The old bare form `CT-n` is refused from this ticket on, by the schemas and by the Rust loader (`Table::take_provenance`).
   The loader accepts exactly `CT-`, three ASCII digits, `-`, one or more ASCII digits (no sign, space or suffix) and exposes the two
   parts as `Provenance::calibration_spec` and `Provenance::calibration_target`.
3. Existing rows migrated by file: `locomotion.toml`, `terrain.toml`, `drill.toml` carry `CT-004-n`; `ammo.toml`, `weapons.toml`,
   `damage.toml` carry `CT-005-n`. Row values and ids are untouched, so no golden hash changes.
4. The unit suffixes of REGISTRY section 11 extend the list in ADR 0004; a key whose suffix is in neither list is refused by the
   schemas. The registry is the authority for every identifier, constant and order that more than one spec touches; where a spec
   and the registry disagree, the registry wins and the spec is corrected.

**Test.** TR-02: `crates/battle-rules/tests/tr_02_ct_format.rs` and `CalibrationTargetFormat` in `tools/test_validate_data.py`
accept the full form, refuse a bare `CT-n`, a one-, two- or four-digit prefix, a letter in the prefix or number, and the old form
on every shipped file.

**Consequences.** Later specs that add a data row with a calibration target use their own prefix from the start. The registry
checker of T-027 verifies that every prefix in `data/`, `docs/` and `qc/` belongs to a registered spec.
