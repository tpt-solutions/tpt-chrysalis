# ADR 0004: Rust/Wasm Frontend Framework Choice — Leptos

**Status**: Accepted
**Date**: 2026-08-05

## Context

The specification (§7) explicitly requires a **Rust/Wasm** frontend for "real-time microfluidic-state and high-content-imaging visualization" with a stated performance requirement. This is not an open ecosystem-consistency choice — the sibling projects `tpt-soma`/`tpt-cerebrum` use React/TypeScript, but the spec mandates Rust/Wasm for this project.

Three main Rust/Wasm frontend frameworks are viable:
1. **Leptos** — fine-grained reactivity, server-side rendering (SSR) support, mature ecosystem, good Wasm size, signals-based
2. **Dioxus** — Virtual DOM, cross-platform (desktop/mobile/web), RSX syntax, growing ecosystem
3. **egui + wgpu** — immediate-mode GUI, 3D rendering via wgpu, excellent for custom 3D visualizations, but less "web-native" for DOM-heavy UIs

## Decision

**Choose Leptos** as the Rust/Wasm frontend framework for `tpt-chrysalis`.

Rationale:
- **Fine-grained reactivity** matches the real-time sensor-stream display requirement (PDSG node updates, sensor values) without virtual DOM diffing overhead
- **SSR + hydration** enables fast initial loads for the compliance/reporting views while keeping the real-time dashboard fully client-side
- **Wasm size** is competitive (~100-200KB gzipped for a typical app) — important for the spec's performance requirement
- **Ecosystem maturity** — Leptos has the most production deployments among Rust/Wasm frameworks (as of 2024-2025)
- **Signals-based model** maps cleanly to the PDSG's reactive node/edge updates
- **No "React-like" mental model** — avoids the ecosystem-consistency trap of mimicking React in Rust when the spec explicitly chose Rust/Wasm for performance

For the **3D Organ-on-a-Chip/microfluidic view (Phase 2)**, Leptos interops with `wgpu` via `leptos-wgpu` or a canvas overlay — the 3D rendering is a focused component, not the whole UI paradigm.

## Consequences

### Positive
- Single language (Rust) across backend (`tpt-chrysalis-api`) and frontend — shared types via `schemas/`
- Fine-grained reactivity = minimal re-renders for high-frequency sensor updates
- SSR supports the compliance-reporting view (ALCOA+ reports) with fast FCP
- Leptos Router enables deep-linking to specific experiment/run PDSG views

### Negative / Risks
- **Learning curve** for team members coming from React/TypeScript (`tpt-soma`/`tpt-cerebrum` stack)
- **Smaller hiring pool** vs. React — mitigated by Rust-first team composition
- **3D integration** requires `wgpu` interop rather than a native three.js/React-three-fiber ecosystem

### Mitigations
- Document Leptos patterns in `frontend/CLAUDE.md` for onboarding
- Keep the 3D view as a self-contained `wgpu` component mounted in a Leptos `<canvas>`
- Share PDSG types via `schemas/` (JSON Schema + Rust types) so frontend/backend stay in sync

## Alternatives Considered

1. **Dioxus** — strong contender; RSX is familiar to React devs. Rejected: Virtual DOM adds overhead for real-time sensor streams; cross-platform (desktop/mobile) not needed for this web-only dashboard
2. **egui + wgpu** — best for pure 3D/custom rendering. Rejected: immediate-mode is a poor fit for the DOM-heavy compliance/reporting/dashboard views; no SSR; accessibility/SEO harder
3. **React/TypeScript (matching `tpt-soma`/`tpt-cerebrum`)** — rejected: spec §7 explicitly requires Rust/Wasm for performance; this is a stated technical requirement, not an open choice

## References
- `spec.txt` §7: "Frontend is **Rust/Wasm**, per spec §7's explicit performance requirement for real-time microfluidic-state and high-content-imaging visualization"
- Leptos: https://leptos.dev
- `frontend/` scaffold (to be created)
- `schemas/` — shared PDSG/capability-token types for frontend/backend contract
- ADR 0007 (schema evolution policy)