# ADR 0007: Schema Evolution Policy for PDSG Node/Edge Types and Capability-Token Payloads

**Status**: Accepted
**Date**: 2026-08-05

## Context

The `schemas/` directory will contain shared schema definitions for:
- **PDSG node/edge types** (Digital Intent Node, Physical Actuator Node, Sensor/Observer Node, Edge kinds)
- **Capability-token payloads** (action IDs, scopes, expiries, resource classes)
- **API request/response contracts** (protocol submission, PDSG queries, compliance reports)

These schemas are **cross-crate** (Rust crates, frontend Wasm, API) and **cross-language** (Rust, TypeScript via JSON Schema, potentially Python for tooling). They must evolve additively without breaking existing consumers.

## Decision

**Adopt an additive-only, versioned schema evolution policy** documented in `schemas/POLICY.md` and enforced by CI.

### Core Principles

1. **Additive only** — fields are added, never removed or renamed. Optional fields default to sensible values.
2. **Explicit versioning** — every schema file has a `version: "1.x"` field (semver minor for additive changes, major for breaking — but breaking changes require a new schema file, not a version bump).
3. **JSON Schema as source of truth** — `schemas/*.json` (JSON Schema Draft 2020-12) is the canonical definition; Rust types are generated via `schemars` or hand-written to match.
4. **Forward + backward compatibility** — consumers must ignore unknown fields; producers must populate all required fields.
5. **Deprecation, not deletion** — fields can be marked `deprecated: true` with a `deprecationMessage`; they remain in the schema for at least 2 minor versions.

### Schema File Structure

```
schemas/
├── pdsg/
│   ├── node-intent.v1.json
│   ├── node-actuator.v1.json
│   ├── node-sensor.v1.json
│   ├── edge.v1.json
│   └── subgraph.v1.json
├── capability/
│   ├── token.v1.json
│   ├── action-registry.v1.json
│   └── scope-registry.v1.json
├── api/
│   ├── submit-intent.v1.json
│   ├── pdsg-snapshot.v1.json
│   └── alcoa-report.v1.json
└── POLICY.md
```

### Versioning Mechanics

- **Minor version bump** (1.0 → 1.1) = additive field addition
- **New schema file** (v2) = breaking change (field removal, type change, required-field addition)
- **No in-place major version bumps** — a v2 schema lives alongside v1; consumers migrate explicitly

### CI Enforcement

1. **Schema validation** — `jsonschema` crate validates example payloads against schemas in CI
2. **Breaking change detection** — `schemaver` or custom script compares PR schemas vs `main`; fails on non-additive changes
3. **Rust type sync** — `cargo test --features schema-gen` regenerates Rust types from JSON Schema; fails if checked-in types drift
4. **Frontend type sync** — `npm run types:generate` (or `wasm-bindgen` + `tsify`) generates TypeScript types; fails if drift

### Rust Integration

- `tpt-chrysalis-core` re-exports schema-aligned types (via `schemars` derive or manual impl)
- `tpt-chrysalis-api` uses the same types for request/response (serde)
- Frontend imports generated TS types from `schemas/` output

## Consequences

### Positive
- **Zero breaking changes by accident** — CI catches schema drift before merge
- **Cross-language contract** — Rust, TS, Python all share the same source of truth
- **Explicit migration path** — v1 and v2 coexist; consumers migrate on their schedule
- **Auditability** — schema history in git shows exactly when fields were added/deprecated

### Negative / Risks
- **Schema bloat** — deprecated fields accumulate; mitigated by 2-minor-version cleanup policy
- **Generator maintenance** — `schemars`/`tsify` must stay in sync with JSON Schema features
- **Initial overhead** — writing JSON Schema by hand is more verbose than Rust derives

### Mitigations
- Start with hand-written JSON Schema for v1; adopt `schemars` derive for Rust → JSON Schema generation once types stabilize
- Document the cleanup policy: deprecated fields removed in `v{N+2}.0` (never in minor)
- Use `serde(skip_serializing_if = "Option::is_none")` + `default` for optional additive fields

## Alternatives Considered

1. **Rust types as source of truth, generate JSON Schema** — rejected: frontend needs TS types; round-trip Rust → JSON Schema → TS is lossier than JSON Schema → (Rust + TS)
2. **Protobuf/gRPC** — rejected: spec requires REST/JSON API; protobuf adds tooling complexity for marginal gain here
3. **No formal schema, just Rust types + serde** — rejected: cross-language contract (frontend, compliance tooling) needs a language-neutral definition

## References
- `spec.txt` §3 (PDSG node/edge types)
- `spec.txt` §4 Layer 2 (capability-token payloads)
- `schemas/` directory (to be created)
- ADR 0004 (frontend framework — Leptos + TS type generation)
- ADR 0006 (deployment — schema version pinned in Helm values)