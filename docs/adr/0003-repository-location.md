# ADR 0003: Repository location

**Status:** superseded. The project began under a subdirectory of another repository so that work could start inside the session's attached
repository. On 2026-11-04 it moved to its own repository (`special-memory`), where the project root is the repository root, with the same layout,
toolchain pin and continuous integration (now `.github/workflows/ci.yml`, unfiltered by path).

**Consequence.** No product file refers to anything outside this repository. The `audits/` folder remains the reference environment: build and
implementation sessions must not read it (see `docs/gates/G0-charter.md`). Before any public release, decide whether `audits/` should stay in the
repository at all or move to a private location, because it describes a third party's product and is Restricted under the charter.
