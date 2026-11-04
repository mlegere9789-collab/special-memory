# ADR 0010: Resolution of the ten open design decisions from the specification consolidation

**Status:** accepted, 2026-11-04. Source: `docs/specs/CONFLICTS.md` section D (each decision lists the options considered).

**Decision.** For all ten items the recommended option (a) is adopted.

| Id | Decision |
|---|---|
| D-01 | The whole-map smoke grid is authoritative; sector tiles are a lazy memory layout with no effect on results. |
| D-02 | Ballistics stay on a flat datum (`z` is height above local ground); relief changes sight only. Revisit only if playtests show shots passing through ridges. A hand-made or survey grid may be used for sight until the single elevation source ships. |
| D-03 | Layered observation: SPEC-009 supplies the visibility oracle (visible, transmit, range); SPEC-007 consumes it when the environment is present, adds its own noise and strips true ids. Visibility is read through a small trait so both shapes fit. |
| D-04 | The SPEC-006 morale report is extended with a quantised margin band (steps of 50 permille), a trend, and all nine cause terms in permille of the ceiling; the raw hidden values are never exposed. |
| D-05 | SPEC-007 owns the order-status states and timings; SPEC-015 renders them and adds only the local `Drafted` state. |
| D-06 | One owner per historical row: arrivals, departures and weather stay in the SPEC-008 scenario; SPEC-011 scripts keep orders, observations, trigger hooks and overrides. |
| D-07 | Aggregate (F2) outcomes and F1 hits on aggregates append `Casualty` records with the sentinel target index `u32::MAX`; consumers skip agent-specific checks for it. Hash-stable. |
| D-08 | The player difficulty option selects a SPEC-010 AI level per side; initiative and risk map onto its rows; information noise and courier spread become symmetric scenario handicaps. |
| D-09 | Both trait uses are kept as they model different decisions; the AI sends orders as `Strict` once its own gate has approved them. |
| D-10 | `perf-kit` is a prototype baseline for the M1 measurements only; the production host consumes the SPEC-012 mix packets, and H16-9 is restated against SPEC-012's delay. |

**Consequence.** The blocked tickets named under each item in CONFLICTS.md may proceed once their spec text has been amended to match this table; that amendment is the first step of each affected ticket (spec first, then tests, then code). Any later reversal needs a new ADR.
