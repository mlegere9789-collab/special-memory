# ADR 0005: Supply-chain gates

**Status:** accepted.

**Decision.** Every pull request runs a `supply` CI job (part of gate G4) with three checks. (1) `cargo deny check` against `deny.toml`: advisories (vulnerabilities,
unmaintained and yanked crates) are errors; only permissive licences (MIT, Apache-2.0 with or without the LLVM exception, BSD-2/3-Clause, ISC, Unicode-3.0, Zlib,
Unlicense, 0BSD, CC0-1.0) are allowed, so no copyleft code can enter a proprietary build; registries other than crates.io and all git sources are refused.
(2) `python3 tools/licence_ledger.py --check` fails when `docs/licences.md`, generated from `cargo metadata --locked`, no longer matches the lockfile. The ledger lists
every third-party package with exact version, licence expression and source, and is the evidence trail for counsel at gates G4 and G7. (3) `cargo-cyclonedx` writes a
CycloneDX SBOM that CI uploads as a build artifact.

**Pins.** The job installs `cargo-deny` 0.20.2 and `cargo-cyclonedx` 0.5.9 with `cargo install --locked --version`, cached by version; no `latest`. Crate versions stay
fixed by the committed `Cargo.lock` (ADR 0004).

**Dual licences.** An `OR` expression passes when one alternative is allowed (for example `MIT OR Apache-2.0 OR LGPL-2.1-or-later` is used under MIT). The ledger records
the full expression so the choice is visible. An exception to the allow list needs a written reason in `deny.toml` and a ticket; silent ignores are not accepted.

**Consequences.** A new dependency changes `Cargo.lock`, so the ledger must be regenerated in the same ticket. An advisory published against an existing dependency can turn
the job red without a code change; that is intended and is handled as a dependency-update ticket.
