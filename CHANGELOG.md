# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Phase 0 monorepo foundational setup complete
- Cargo workspace with 10 crates (7 implemented, 3 Phase 2/3 placeholders)
- 7 Architecture Decision Records (ADRs) documenting key architectural choices
- 12 JSON Schema contracts for PDSG nodes/edges, capability tokens, and API payloads
- Leptos + Trunk Wasm frontend scaffold with Dashboard, PDSG Explorer, Compliance Report, Settings pages
- Dual deployment: Docker Compose (dev) + Helm chart (Kubernetes) with full parity
- CI pipeline (`.github/workflows/ci-rust.yml`) with 7 jobs: rust, wasm-frontend, hal-no-std, helm, security, kind-integration, summary
- Pre-commit hooks mirroring CI checks locally
- `justfile` with 20+ standardized dev commands per ecosystem convention
- Sibling dependency audit completed for `tpt-archon`, `tpt-flight-control`, `tpt-crucible`, `tpt-anima`, `tpt-eve`
- Ecosystem convention files: `README.md`, `CLAUDE.md`, `AGENTS.md`, `LICENSE-MIT`, `LICENSE-APACHE`

### Changed
- N/A (initial release)

### Deprecated
- N/A

### Removed
- N/A

### Fixed
- N/A

### Security
- Capability-token-based security model established via `tpt-archon` integration
- Deterministic safety gate (`tpt-chrysalis-safety`) with zero LLM in decision path
- Immutable audit ledger via `tpt-archon-core` WAL
- Network policies (deny-by-default) in Helm chart

## [0.1.0] - 2024-08-05

### Added
- Initial repository structure
- Cargo workspace configuration
- Basic project documentation

---

## Release Process

1. Update version in `Cargo.toml` (workspace.package.version)
2. Update this CHANGELOG.md with release notes
3. Create git tag: `git tag v0.x.x`
4. Push tag: `git push origin v0.x.x`
5. GitHub Actions will build and publish artifacts

## Versioning Policy

- **Major** (x.0.0): Breaking changes to PDSG schema, capability token format, or HAL trait surface
- **Minor** (0.x.0): New features, new schema versions (additive), new API endpoints
- **Patch** (0.0.x): Bug fixes, documentation updates, CI improvements

Schema versions are independent of crate versions and follow additive-only evolution (see `schemas/POLICY.md`).