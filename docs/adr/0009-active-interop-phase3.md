# ADR 0009 — Phase 3 active-learning interop surface

- **Status**: Accepted
- **Date**: 2026-08-05
- **Deciders**: tpt-chrysalis maintainers

## Context

Phase 3 (spec.txt §8) introduces the autonomous closed loop: `tpt-anima`
(active-learning / Bayesian optimization) proposes the next protocol and
`tpt-eve` (symbolic/causal) advises on higher-level experiment design. Per the
Phase 0 sibling audit, both are real crates with defined APIs; they must be
integrated **without** creating a bypass around the safety gate.

## Decision

1. The learning components are defined as **traits** in `tpt-chrysalis-active`:
   `ProposalSource` (`tpt-anima`'s role) and `Advisor` (`tpt-eve`'s advisory
   role). The real sibling crates drop in behind these traits once available.
2. Every proposed protocol is evaluated by the Phase 1 `tpt-chrysalis-safety`
   gate **unchanged** inside `ClosedLoop`. An `Abort` means the protocol is not
   executed — the loop simply proposes again. No AI-originated path skips the
   gate.
3. A deterministic model-based learner (`QuadraticFitSource`, a stand-in for
   `tpt-anima`'s Bayesian optimizer) and a grid-search baseline ship so the
   Phase 3 benchmark (spec.txt §5C's 10×–100× reagent/time-cost claim) is
   measurable *now*, reporting the **actual** ratio rather than the target.

## Consequences

- `tpt-chrysalis-active` depends only on `tpt-chrysalis-safety` (no sibling
  dependency yet), so it builds/tests in isolation.
- The safety invariant is preserved by construction and unit-tested
  (`safety_gate_blocks_out_of_bounds_proposals`).

## Alternatives considered

- Calling the gate from inside `tpt-anima`: rejected — the gate is chrysalis's
  responsibility (ADR 0002); the AI runtime only proposes.
