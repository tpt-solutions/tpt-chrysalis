# AGENTS.md — tpt-chrysalis

> Instructions for AI agents when working in this repository.

This file mirrors `CLAUDE.md` for compatibility with other AI coding agents. See `CLAUDE.md` for the full content.

## Quick Reference

**Project**: tpt-chrysalis — Deterministic lab automation for Organ-on-a-Chip
**Language**: Rust (std + no_std) + Wasm (Leptos)
**Key Principle**: Safety-critical, deterministic, auditable — no LLM in safety path

## Commands

```bash
just help          # All commands
just lint          # fmt + clippy
just test          # workspace tests
just build         # release build
just dev           # docker compose up
just wasm-dev      # trunk serve
just ci            # full CI locally
just helm-install  # deploy to kind
```

## Architecture (see `docs/adr/`)

1. `tpt-archon` for kernel/ledger/storage
2. `tpt-chrysalis-safety` (deterministic) not `tpt-eve` (LLM)
3. Simulation-first (Phase 1)
4. Leptos + Trunk frontend
5. MinIO/S3 for imaging
6. Docker Compose + Helm parity
7. Additive-only schemas

## Safety Rules

- No LLM in safety path
- Capability tokens gate all physical commands
- Immutable audit via tpt-archon WAL
- Fail-closed (deny by default)
- Simulation-first, no real hardware Phase 1

## Sibling Repos (local)

- `tpt-archon` — kernel, ledger, relational (git dep)
- `tpt-flight-control` — `tpt-math`, `tpt-abstractions` (crate deps)
- `tpt-crucible` — `tpt-catalyst` (Phase 3 only)
- `tpt-anima` — `anima-sse`, `anima-inp` (Phase 3)
- `tpt-eve` — symbolic/causal (Phase 3 advisory)

## Testing

- Unit >80% coverage
- Integration: intent → safety → capability → HAL → audit → PDSG → compliance
- Security: forged tokens, bypass attempts, tampering
- no_std target: `thumbv7em-none-eabihf`
- Wasm: `trunk build --release`

## CI Jobs

1. rust (fmt, clippy, build, test)
2. wasm-frontend (trunk build)
3. hal-no-std (target check, no alloc/std)
4. helm (lint, template)
5. security (cargo deny)
6. kind-integration (scheduled)
7. summary

## Pre-commit

`pre-commit install` — runs fmt, clippy, test, deny, schema validation, linting

---

**Full details in `CLAUDE.md` and `spec.txt`**