# tpt-chrysalis

> Deterministic lab automation platform for Organ-on-a-Chip and microphysiological systems.

[![CI](https://github.com/tpt-solutions/tpt-chrysalis/actions/workflows/ci-rust.yml/badge.svg)](https://github.com/tpt-solutions/tpt-chrysalis/actions/workflows/ci-rust.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/License-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE-MIT)
[![Rust Version](https://img.shields.io/badge/rust-1.85+-orange.svg)](https://www.rust-lang.org)

## Overview

tpt-chrysalis is a safety-critical lab automation platform built for deterministic, auditable control of microfluidic and Organ-on-a-Chip (OoC) systems. It implements the Physical-Digital State Graph (PDSG) model with capability-based security, immutable audit ledgering, and a formally-verified `no_std` Hardware Abstraction Layer (HAL).

### Key Features

- **Deterministic Safety Gate** (`tpt-chrysalis-safety`): Pure symbolic constraint engine — zero LLM, zero network, fail-closed
- **Capability-Based Security** (`tpt-archon`): Unforgeable tokens gate every physical command via kernel enforcement
- **Immutable Audit Ledger** (`tpt-archon`): Hash-chained WAL recording every sensor reading and actuator movement
- **PDSG Model** (`tpt-chrysalis-core`): Digital Intent → Physical Actuator → Sensor Observer graph backed by relational storage
- **Simulation-First HAL** (`tpt-chrysalis-hal` + `tpt-chrysalis-sim`): `no_std` traits validated against in-process physics simulator
- **Rust/Wasm Frontend** (Leptos): Real-time PDSG visualization, sensor streams, ALCOA+ compliance reports
- **Dual Deployment**: Docker Compose (dev) + Kubernetes/Helm (prod) with strict parity

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        tpt-chrysalis-api                        │
│  REST + WebSocket  │  PDSG State  │  Protocol Submit  │  Audit │
└─────────────────────────────────────────────────────────────────┘
                                  │
                    ┌─────────────┼─────────────┐
                    ▼             ▼             ▼
            ┌─────────────┐ ┌───────────┐ ┌─────────────┐
            │ tpt-chrysalis-    │ tpt-archon  │  MinIO/S3   │
            │ orchestrator  │ │  kernel   │ │  (imaging)  │
            │  + safety     │ │  + ledger │ └─────────────┘
            └─────────────┘ └───────────┘
                    │             │
                    ▼             ▼
            ┌───────────────────────────────┐
            │     tpt-chrysalis-hal         │
            │  (no_std trait surface)       │
            └───────────────────────────────┘
                    │
                    ▼
            ┌───────────────────────────────┐
            │   tpt-chrysalis-sim           │
            │  (simulated liquid handler)   │
            └───────────────────────────────┘
```

## Quick Start

### Prerequisites

- Rust 1.85+ (`rustup install 1.85`)
- Trunk (`cargo install trunk --locked`)
- Docker + Docker Compose
- Kind + Helm (for Kubernetes testing)
- Just (`cargo install just`)

### Local Development

```bash
# Clone and enter
git clone https://github.com/tpt-solutions/tpt-chrysalis
cd tpt-chrysalis

# Generate local secrets
just gen-secrets

# Start dev stack (tpt-archon, MinIO, API, Frontend)
just dev

# Or manually:
docker compose -f deploy/docker-compose.yml -f deploy/docker-compose.override.yml up -d

# Access:
# - Frontend: http://localhost:8080
# - API: http://localhost:3000
# - MinIO Console: http://localhost:9001
# - tpt-archon gRPC: localhost:50051/50052/50053
```

### Run Tests

```bash
# All workspace tests
just test

# Lint (fmt + clippy)
just lint

# Full CI locally
just ci
```

### Frontend Development

```bash
# Hot-reload dev server
just wasm-dev

# Or manually:
cd frontend && trunk serve --port 8080 --open
```

### Kubernetes (Kind)

```bash
# Create cluster and deploy
just helm-install

# Or manually:
kind create cluster --name tpt-chrysalis --config deploy/kind-config.yaml
helm upgrade --install tpt-chrysalis deploy/helm/tpt-chrysalis \
  --namespace tpt-chrysalis --create-namespace \
  -f deploy/helm/tpt-chrysalis/values-secrets.yaml.example
```

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
├── schemas/                       # JSON Schema contracts (PDSG, capability, API)
├── docs/adr/                      # Architecture Decision Records
├── justfile                       # Standardized dev commands
├── .github/workflows/ci-rust.yml  # CI pipeline
└── .pre-commit-config.yaml        # Pre-commit hooks
```

## ADRs (Architecture Decision Records)

| ID | Title | Status |
|----|-------|--------|
| 0001 | Depend on `tpt-archon` for kernel/ledger/storage | Accepted |
| 0002 | Dedicated `tpt-chrysalis-safety` instead of `tpt-eve` | Accepted |
| 0003 | Simulation-first hardware strategy | Accepted |
| 0004 | Leptos for Rust/Wasm frontend | Accepted |
| 0005 | MinIO/S3 for imaging object storage | Accepted |
| 0006 | Dual Docker Compose + Helm deployment | Accepted |
| 0007 | Additive-only schema evolution policy | Accepted |

## Spec Compliance

This implementation follows `spec.txt` v1.0.0:

- **Phase 0** ✓ Monorepo setup, ADRs, schemas, deployment skeletons, CI
- **Phase 1** (in progress) Deterministic wedge: HAL, safety, orchestrator, compliance, API, frontend
- **Phase 2** Organoid linking + Vision (planned)
- **Phase 3** Active learning loop with `tpt-anima`/`tpt-eve` (planned)
- **Phase 4** Whole-body digital twin + `tpt-soma`/`tpt-cerebrum` (planned)

## License

Dual-licensed under MIT OR Apache-2.0.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) (ecosystem standard at `Open Source/.github/CONTRIBUTING.md`).

## Ecosystem

Part of the TPT Solutions platform:
- `tpt-archon` — Capability kernel, audit ledger, relational storage
- `tpt-cerebrum` — Cognitive architecture (sibling)
- `tpt-soma` — Digital twin (sibling)
- `tpt-anima` — Active learning runtime (sibling)
- `tpt-eve` — Symbolic/causal reasoning (sibling)
- `tpt-crucible` — Compilation toolchain (sibling)
- `tpt-flight-control` — Verified control primitives (sibling)