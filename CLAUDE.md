# CLAUDE.md — tpt-chrysalis

> Instructions for Claude Code when working in this repository.

## Project Overview

tpt-chrysalis is a deterministic lab automation platform for Organ-on-a-Chip and microphysiological systems. It implements the Physical-Digital State Graph (PDSG) model with capability-based security, immutable audit ledgering, and a formally-verified `no_std` Hardware Abstraction Layer (HAL).

**Key architectural decisions (see `docs/adr/`):**
1. Depend on `tpt-archon` for capability kernel + audit ledger + relational storage
2. Dedicated `tpt-chrysalis-safety` (deterministic) instead of `tpt-eve` (LLM-based) for safety-critical path
3. Simulation-first hardware strategy for Phase 1
4. Leptos + Trunk for Rust/Wasm frontend
5. MinIO/S3 for imaging object storage
6. Dual Docker Compose + Helm deployment parity
7. Additive-only schema evolution policy

## Development Commands

Use `just` for all common tasks:

```bash
just help          # Show all commands
just lint          # cargo fmt --check + cargo clippy -- -D warnings
just test          # cargo test --workspace
just build         # cargo build --workspace --release
just dev           # Start local dev stack (docker compose)
just dev-stop      # Stop local dev stack
just dev-logs      # Follow dev stack logs
just wasm-build    # Build frontend Wasm (trunk build --release)
just wasm-dev      # Run frontend dev server (trunk serve)
just ci            # Run full CI pipeline locally
just kind-up       # Create kind cluster for Helm testing
just kind-down     # Delete kind cluster
just helm-lint     # Lint Helm chart
just helm-template # Render Helm templates
just helm-install  # Install Helm chart to kind
just helm-test     # Run Helm test suite
just fmt           # Format code (cargo fmt)
just check         # Check code (cargo check --workspace)
just clean         # Clean build artifacts
just gen-secrets   # Generate local dev secrets
```

## Code Style & Conventions

### Rust
- Edition 2024, MSRV 1.85
- `cargo fmt` (rustfmt.toml) and `cargo clippy -- -D warnings` (clippy.toml) must pass
- `no_std` crates (`tpt-chrysalis-hal`) must not pull in `alloc` or `std` transitively
- Use `thiserror` for error types, `anyhow` for application-level errors
- Structured logging with `tracing` (JSON in production, pretty in dev)

### Wasm Frontend (Leptos)
- Components in `frontend/src/components.rs`
- Pages in `frontend/src/pages.rs`
- State management in `frontend/src/state.rs`
- API client in `frontend/src/api.rs`
- Types in `frontend/src/types.rs`

### Schemas
- All data contracts in `schemas/` as JSON Schema (draft-2020-12)
- Additive-only evolution: new optional fields only, never remove/rename
- Version in filename: `node-intent.v1.json`, `node-intent.v2.json`
- Validate with `check-jsonschema` pre-commit hook

### ADRs
- One ADR per architectural decision in `docs/adr/`
- Format: `NNNN-title.md` with Status, Context, Decision, Consequences
- Never delete or modify accepted ADRs; supersede with new ADR

## Project Structure

```
tpt-chrysalis/
├── crates/
│   ├── tpt-chrysalis-core/        # PDSG model, capability tokens, audit ledger
│   ├── tpt-chrysalis-hal/         # no_std HAL traits (actuator/sensor)
│   ├── tpt-chrysalis-sim/         # Simulated hardware backend
│   ├── tpt-chrysalis-safety/      # Deterministic safety gate engine
│   ├── tpt-chrysalis-orchestrator/ # Resource locking, protocol compiler
│   ├── tpt-chrysalis-compliance/  # ALCOA+ report generator
│   ├── tpt-chrysalis-api/         # REST + WebSocket service
│   └── (Phase 2/3 placeholders)
├── frontend/                      # Leptos + Trunk Wasm frontend
├── deploy/
│   ├── docker-compose.yml         # Local dev stack
│   ├── docker-compose.override.yml # Dev conveniences
│   ├── Dockerfile.api             # Multi-stage API build
│   ├── Dockerfile.frontend        # Multi-stage frontend build
│   ├── nginx.frontend.conf        # Nginx config for Wasm
│   ├── kind-config.yaml           # Kind cluster config
│   └── helm/tpt-chrysalis/        # Helm chart
├── schemas/                       # JSON Schema contracts
├── docs/adr/                      # Architecture Decision Records
├── justfile                       # Standardized dev commands
├── .github/workflows/ci-rust.yml  # CI pipeline
└── .pre-commit-config.yaml        # Pre-commit hooks
```

## Sibling Dependencies (Local)

All sibling repos are cloned under `d:/Programming/1PRODUCTION/Open Source/`:

| Repo | Purpose | Integration |
|------|---------|-------------|
| `tpt-archon` | Capability kernel, audit ledger, relational storage | Direct workspace dependency (git) |
| `tpt-flight-control` | Verified math (`tpt-math`), abstractions (`tpt-abstractions`) | Crate deps for HAL verification |
| `tpt-crucible` | AI model compiler (`tpt-catalyst`) | Phase 3 inference compilation only |
| `tpt-anima` | Active learning runtime (SSE/INP/ZKPA/CK) | Phase 3 `anima-sse`/`anima-inp` |
| `tpt-eve` | Symbolic/causal/neural reasoning | Phase 3 advisory only (not safety) |

## Safety-Critical Rules

1. **No LLM in safety path** — `tpt-chrysalis-safety` is pure deterministic constraint engine
2. **Capability tokens required** — Every physical command gated by `tpt-archon-kernel`
3. **Immutable audit** — Every sensor/actuator event logged via `tpt-archon-core` WAL
4. **Fail-closed** — Safety gate defaults to deny; malformed input = rejection
5. **Simulation-first** — No real hardware in Phase 1; `tpt-chrysalis-sim` implements HAL traits

## Testing Requirements

- Unit tests for all crates (target >80% coverage)
- Integration tests: intent → safety → capability → HAL → audit → PDSG → compliance
- Security tests: forged tokens, safety bypass attempts, audit tampering
- `no_std` target check: `cargo build --target thumbv7em-none-eabihf --no-default-features` for HAL
- Wasm build check: `trunk build --release` in CI

## Deployment

### Local Dev (Docker Compose)
```bash
just dev
# Frontend: http://localhost:8080
# API: http://localhost:3000
# MinIO Console: http://localhost:9001
# tpt-archon gRPC: localhost:50051/50052/50053
```

### Kubernetes (Kind)
```bash
just helm-install
# Ingress at http://chrysalis.local (add to /etc/hosts)
```

### Production (Helm)
```bash
helm upgrade --install tpt-chrysalis deploy/helm/tpt-chrysalis \
  -n tpt-chrysalis --create-namespace \
  -f values-secrets.yaml
```

## Common Tasks

### Add a new schema
1. Create `schemas/category/name.v1.json`
2. Update `schemas/POLICY.md` if new category
3. Run `just lint` to validate

### Add a new ADR
1. Create `docs/adr/NNNN-title.md` (next number)
2. Follow template in `docs/adr/0001-archon-reuse.md`
3. Link from related ADRs

### Update dependencies
```bash
just update-deps
```

### Generate secrets for local dev
```bash
just gen-secrets
# Creates deploy/secrets/capability_root_key
```

## CI Pipeline (`.github/workflows/ci-rust.yml`)

Jobs (all must pass):
1. **rust** — fmt, clippy, build, test, build API binary
2. **wasm-frontend** — trunk build --release
3. **hal-no-std** — check HAL compiles for `thumbv7em-none-eabihf`, verify no `alloc`/`std` leakage
4. **helm** — lint chart, template render
5. **security** — cargo deny check
6. **kind-integration** (scheduled/manual) — deploy to kind, run integration tests
7. **summary** — aggregate status

## Pre-commit Hooks

Install: `pre-commit install`

Runs on commit:
- `cargo fmt --check`
- `cargo clippy --workspace -- -D warnings`
- `cargo test --workspace` (quick)
- `cargo deny check`
- JSON schema validation
- YAML/Markdown/Shell linting
- Trailing whitespace, EOF, merge conflict detection

## Troubleshooting

### Cargo cache issues
```bash
cargo clean
rm -rf ~/.cargo/registry/cache
just build
```

### Frontend Wasm issues
```bash
cd frontend
trunk clean
trunk build --release
```

### Docker Compose port conflicts
```bash
just dev-stop
docker system prune -f
just dev
```

### Kind cluster issues
```bash
just kind-down
just kind-up
```

## References

- `spec.txt` — Full 4-phase specification
- `TODO.md` — Detailed task tracking
- `docs/adr/` — Architectural decisions
- `schemas/` — Data contracts
- `tpt-archon` sibling — Capability kernel/ledger API