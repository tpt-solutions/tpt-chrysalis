# ADR 0008 — Phase 2 organoid/vision crates and capability-token extension

- **Status**: Accepted
- **Date**: 2026-08-05
- **Deciders**: tpt-chrysalis maintainers

## Context

Phase 2 (spec.txt §8) expands the platform from a single simulated liquid
handler to a multi-organ microphysiological system with high-content imaging.
This requires (a) a multi-organ "blood flow" linking model and Patient-Derived
Organoid screening, (b) S3/MinIO object storage with checksum-on-write for
microscopy images, and (c) a deterministic local CV pipeline. The capability
token registry (ADR 0001) must also grow new resource classes for organoid and
vision commands/data.

## Decision

1. `tpt-chrysalis-organoid` and `tpt-chrysalis-vision` are built **in-repo**
   (not deferred to sibling crates), because their scope is chrysalis-specific
   (chip-level flow graphs, PDO screening, imaging feature extraction) and they
   must route through the same safety/audit story as every other component.
2. The organoid/vision specific constraints are **folded into the existing
   `tpt-chrysalis-safety` gate** (new `Var`s `FlowRateUlMin`,
   `CrossChipIncompatible`, `BarrierIntegrity`; new `ActionId`s `CaptureImage`,
   `StoreObject`, `LinkChip`, `ScreenPdo`). No second safety path is introduced.
3. Imaging storage uses a single `ObjectStore` trait with memory / filesystem /
   MinIO backends and SHA-256 checksum-on-write; the real MinIO HTTP client is
   behind a `minio` feature so the crate builds and tests without network.

## Consequences

- Two new crates with full unit-test coverage; they do not depend on the
  (sibling) `tpt-archon` crates, so they build and test in isolation.
- The capability registry grows four Phase 2 action ids plus `ORGANOID_SCOPE`
  and `VISION_SCOPE` constants (see `schemas/examples/capability/*`).
- Deployment (compose + Helm) gains MinIO bucket-init and the new scopes.

## Alternatives considered

- Putting organoid/vision logic in `tpt-cerebrum`/`tpt-soma`: rejected — those
  are pre-implementation (Phase 4) and the worked scope is chrysalis-owned.
- A relaxed "imaging safety" path: rejected — the gate stays the single
  choke-point per ADR 0002.
