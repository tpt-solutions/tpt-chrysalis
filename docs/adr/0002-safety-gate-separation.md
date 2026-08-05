# ADR 0002: Dedicated Deterministic Safety Gate (`tpt-chrysalis-safety`) Instead of `tpt-eve` for Physical/Chemical Safety

**Status**: Accepted
**Date**: 2026-08-05

## Context

The specification (§4 Layer 3) calls for a "safety constraint checker" that evaluates every protocol against hard rules (e.g., "IF reagent_A AND reagent_B THEN temperature MUST BE < 4°C ELSE abort") before physical execution. The spec names this component `tpt-eve`.

However, `tpt-eve` is documented as a **general cognitive architecture** with an **LLM neural-pattern layer**. Its GitHub description emphasizes causal reasoning, hypothesis generation, and cross-domain abstraction — all valuable for *experiment design* (Phase 3+), but fundamentally incompatible with a **zero-hallucination-tolerance** physical/chemical safety gate.

A safety gate that decides whether a valve opens or a reagent dispenses must be:
- **Purely deterministic** — same inputs always produce the same allow/deny decision
- **Zero external dependencies** — no network calls, no LLM inference, no non-deterministic scheduling
- **Fail-closed** — missing data or malformed protocols abort, never default to permissive
- **Formally verifiable** — the rule schema and evaluator must admit proof of absence of buffer overflows, race conditions, and logic errors on the critical path

An LLM-backed system cannot satisfy these constraints. Even with guardrails, the architectural presence of a neural layer in the decision path violates the spec's zero-tolerance requirement.

## Decision

**Build `tpt-chrysalis-safety` as a dedicated, purely-deterministic symbolic constraint engine inside this repository.** It will:
- Define a typed rule schema (preconditions over reagents, temperature, pressure, volume)
- Provide a pure evaluator with no LLM, no network, no randomness, no time dependence
- Run **before** the orchestrator compiles any protocol to the HAL
- Make physical execution **unreachable** on a failing check (not merely logged)
- Be unit-testable in isolation with known-safe/known-unsafe protocol vectors

**Defer real `tpt-eve` integration to Phase 3** for **non-safety-critical, higher-level experiment-design reasoning only**:
- Hypothesis generation from PDSG sensor data
- Cross-domain abstraction (e.g., "this pattern resembles neurotoxicity")
- Advisory input to `tpt-anima`'s Bayesian optimization loop
- **Never** as a substitute for or bypass of `tpt-chrysalis-safety`'s gate

## Consequences

### Positive
- **Zero hallucination risk** on the physical-command decision path — the gate is pure Rust logic
- **Formal verification friendly** — the `no_std`, zero-allocation evaluator can be proven correct with `tpt-telos`/`kani`
- **Clear architectural boundary** — AI reasoning (Phase 3) and physical safety (Phase 1) are separate crates with separate trust bases
- **Regulatory reviewability** — the rule schema and evaluator are human-readable, auditable, and deterministic

### Negative / Risks
- **Duplication of "safety" concept** — `tpt-eve` also has a safety/reliability layer; we must ensure they don't drift into conflicting semantics
- **Maintenance burden** — we own the rule engine instead of reusing `tpt-eve`'s
- **Phase 3 integration complexity** — must explicitly wire `tpt-eve` as advisory only, with tests proving it cannot bypass the gate

### Mitigations
- Document the rule schema as the **single source of truth** for physical/chemical constraints; `tpt-eve` may *reference* it but never *override* it
- Add a regression test in Phase 3: AI-generated protocols that `tpt-eve` would approve but `tpt-chrysalis-safety` rejects must be blocked
- Keep the rule schema in `schemas/` (shared, versioned) so both crates can depend on the same definitions

## Alternatives Considered

1. **Use `tpt-eve` as-is for the safety gate** — rejected: LLM in the decision path violates zero-hallucination-tolerance
2. **Strip the LLM out of `tpt-eve` and use only its symbolic layer** — rejected: `tpt-eve`'s architecture is not designed for this; the symbolic layer (if any) is not documented as a separable, verified crate
3. **Build a thin wrapper around `tpt-eve` that filters out LLM calls** — rejected: still couples the safety-critical path to a cognitive architecture's release cycle and transitive dependencies

## References
- `spec.txt` §4 Layer 3 (names `tpt-eve` as the safety constraint checker)
- `spec.txt` §5C / §8 Phase 3 (active learning with `tpt-anima` + `tpt-eve`)
- `tpt-eve` repository: https://github.com/tpt-solutions/tpt-eve
- ADR 0001 (archon reuse — `tpt-archon` provides the ledger `tpt-eve` might also want)
- `tpt-chrysalis-safety` crate: `crates/tpt-chrysalis-safety/src/lib.rs`