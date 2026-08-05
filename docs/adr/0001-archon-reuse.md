# ADR 0001: Depend on `tpt-archon` Directly for Capability Kernel, Audit Ledger, and Relational Storage

**Status**: Accepted
**Date**: 2026-08-05

## Context

The tpt-chrysalis specification (§4 Layer 2) assigns three foundational responsibilities to a capability-based microkernel:

1. **Capability-based access control** — unforgeable tokens gating every physical command
2. **Immutable audit ledger** — hash-chained, append-only WAL recording every sensor reading, actuator movement, and environmental condition
3. **PDSG graph/relational storage** — the Physical-Digital State Graph backed by a SQL engine

The ecosystem already provides `tpt-archon` which implements exactly these three primitives:
- `tpt-archon-kernel` — capability token issuance, verification, and enforcement via a message router
- `tpt-archon-core` — zero-copy, append-only WAL with page manager for the audit ledger
- `tpt-archon-relational` — SQL-backed storage for graph/relational data (the PDSG)

The sibling project `tpt-cerebrum` evaluated `tpt-archon` and found a scope mismatch (researcher/data-class vs. kernel/IPC), leading them to build a custom capability/audit stack. However, `tpt-chrysalis`'s physical-command model (`dispense(reagent, volume)`-style capability tokens gating a kernel) is **exactly** the kernel/IPC-scoped shape `tpt-archon` already provides.

## Decision

**Depend on `tpt-archon` directly** (as git/path dependencies initially, transitioning to versioned crates.io dependencies when published) for all three Layer 2 responsibilities. No custom capability kernel, audit ledger, or relational storage will be built in this repository.

Specifically:
- `tpt-chrysalis-core` depends on `tpt-archon-core`, `tpt-archon-bridge`, `tpt-archon-kernel`, and `tpt-archon-relational` for ledger, security, and PDSG storage
- `tpt-chrysalis-orchestrator` uses `tpt-archon-kernel`'s `MessageRouter` for capability enforcement on every HAL call
- `tpt-chrysalis-compliance` reads the hash-chained ledger via `tpt-chrysalis-core`'s `AuditLedger` (backed by `tpt-archon-core`)
- New capability-token resource classes (Phase 2 organoid/vision, Phase 3 active learning) extend the same token model rather than replacing it

## Consequences

### Positive
- **Zero duplication** of the most security-critical code (capability verification, hash-chained WAL, SQL storage)
- **Single source of truth** for the audit chain across the ecosystem — `tpt-archon` fixes benefit all dependents
- **Faster Phase 1 delivery** — the deterministic wedge (HAL + safety gate + capability gating + audit) works end-to-end without building kernel/ledger/storage from scratch
- **Ecosystem alignment** — matches the pattern `tpt-archon` itself uses (referencing sibling crates as path deps pre-publish)

### Negative / Risks
- **Coupling to `tpt-archon`'s pre-1.0 API** — breaking changes in `tpt-archon` require coordinated updates
- **Path-dependency friction** — local development requires `tpt-archon` checked out as a sibling; CI must clone it
- **No independent audit-ledger evolution** — if `tpt-chrysalis` needs ledger semantics `tpt-archon` doesn't support, we must upstream them or fork

### Mitigations
- Pin `tpt-archon` to a specific git commit in `Cargo.toml` (workspace dependencies) until it publishes to crates.io
- Track `tpt-archon` API changes via its CHANGELOG; allocate sprint capacity for version bumps
- If a genuine semantic gap appears (not just "we'd prefer a different API"), file an issue in `tpt-archon` and implement the minimal extension there rather than shadowing it here

## Alternatives Considered

1. **Build custom capability/audit stack** (what `tpt-cerebrum` did) — rejected: the spec's physical-command model matches `tpt-archon`'s kernel scope exactly; rebuilding would be NIH with no technical justification
2. **Use `tpt-eve` for the safety gate + ledger** — rejected: `tpt-eve` is a general cognitive architecture with an LLM neural-pattern layer; zero-hallucination-tolerance physical/chemical safety cannot have an LLM in the decision path (see ADR 0002)
3. **Use `tpt-keystone-db` for PDSG storage** — rejected: `tpt-keystone-db` is not yet confirmed to exist as a usable crate; `tpt-archon-relational` already provides the required SQL engine

## References
- `spec.txt` §4 Layer 2
- `spec.txt` §8 Phase 1 deliverable: "proving the safety/security/determinism story end-to-end"
- `tpt-archon` repository: https://github.com/tpt-solutions/tpt-archon
- ADR 0002 (safety gate separation from `tpt-eve`)
- ADR 0003 (simulation-first hardware strategy)