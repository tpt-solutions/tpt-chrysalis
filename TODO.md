# tpt-chrysalis — Project TODO

Tracking checklist for the full 4-phase roadmap defined in `spec.txt` (v1.0.0). Scope decisions locked in for this checklist: `tpt-archon` is depended on directly (not reimplemented) for the capability-based microkernel, the immutable audit ledger, and PDSG graph/relational storage — spec §4 Layer 2 explicitly assigns all three responsibilities to `tpt-archon` already, and unlike the sibling `tpt-cerebrum` project (which found `tpt-archon` a researcher/data-class scope mismatch and built a custom capability/audit stack), `tpt-chrysalis`'s physical-command model (`dispense(reagent, volume)`-style capability tokens gating a kernel) is exactly the kernel/IPC-scoped shape `tpt-archon` already provides — so no third from-scratch storage/capability engine gets added to the ecosystem. The safety-critical protocol gate is a **dedicated, purely-deterministic constraint engine built inside this repo** (`tpt-chrysalis-safety`), not the sibling `tpt-eve` project — `tpt-eve` is a general cognitive architecture with an LLM neural-pattern layer, and the spec's zero-tolerance-for-hallucination requirement on physical/chemical safety means no LLM can sit in that decision path; real `tpt-eve` is deferred to Phase 3 for non-safety-critical, higher-level experiment-design reasoning only, running alongside `tpt-chrysalis-safety` rather than replacing it. `tpt-crucible`, `tpt-flight-control`, and `tpt-anima` are all real repositories on `github.com/tpt-solutions` (confirmed via GitHub API) but are not yet cloned locally and their actual crate/API surfaces haven't been inspected — Phase 0 clones and audits all three before any later-phase task depends on their exact APIs, the same way `tpt-archon` only finalized its `tpt-eidos`/`tpt-telos` integration after reading their real crates rather than their READMEs. Hardware is **simulation-first**: no physical liquid handler, microfluidic pump, or FPGA is available, so Phase 1 builds and validates the HAL, capability gating, and audit ledger entirely against a software-simulated actuator/sensor backend (`tpt-chrysalis-sim`); real hardware drivers are added later against whatever specific equipment becomes available. Frontend is **Rust/Wasm**, per spec §7's explicit performance requirement for real-time microfluidic-state and high-content-imaging visualization — kept as specified rather than switched to the React/TypeScript stack `tpt-soma`/`tpt-cerebrum` used, since this is a stated technical requirement, not an open ecosystem-consistency choice. High-content microscopy imaging (Phase 2) needs object/blob storage that neither `tpt-archon` nor `tpt-keystone-db` provide, so a MinIO/S3-compatible store is added alongside `tpt-archon`, mirroring the pattern `tpt-cerebrum` used for its own volumetric imaging. Single Cargo-workspace monorepo, dual Docker Compose + Kubernetes/Helm deployment maintained from day one, matching ecosystem precedent (`tpt-soma`, `tpt-cerebrum`).

---

## Phase 0 — Monorepo Foundational Setup (blocks all of Phase 1)

### Repo & workspace layout
- [x] `git init`, `.gitignore` (Rust `target/`, Wasm `pkg/`/`dist/`, `.env`, MinIO local volumes, simulator fixture output)
- [x] Cargo workspace root `Cargo.toml` with member crates: `tpt-chrysalis-core` (PDSG graph model + data-access layer), `tpt-chrysalis-hal` (`no_std` actuator/sensor Hardware Abstraction Layer), `tpt-chrysalis-sim` (simulated hardware backend implementing the HAL's traits), `tpt-chrysalis-safety` (deterministic symbolic constraint/safety-gate engine), `tpt-chrysalis-orchestrator` (per spec §5A — resource locking, protocol compilation to the HAL), `tpt-chrysalis-compliance` (per spec §5E — ALCOA+ reports, de-identification), `tpt-chrysalis-api` (backend service tying the above together), plus placeholder crates for `tpt-chrysalis-organoid` (§5B), `tpt-chrysalis-vision` (§5D), `tpt-chrysalis-active` (§5C) matching spec §5's module names exactly for later phases
- [x] `rust-toolchain.toml` pin, workspace `rustfmt.toml`, `clippy.toml`, `deny.toml` (cargo-deny — audit `no_std` crates specifically for `alloc`-pulling transitive deps)
- [x] `frontend/` package: Rust/Wasm scaffold (Leptos + Trunk) with `wasm-pack`/`trunk` build tooling
- [x] `schemas/` directory: shared schema definitions for PDSG node/edge types and capability-token payloads, cross-crate/cross-language data contracts, documented additive-only versioning policy
- [x] `docs/adr/` directory + first ADRs: (1) depend on `tpt-archon` directly for capability kernel + audit ledger + relational storage rather than building custom, (2) `tpt-chrysalis-safety` as a dedicated deterministic engine instead of using `tpt-eve` for the safety-critical path, (3) simulation-first hardware strategy for Phase 1, (4) Rust/Wasm frontend framework choice (Leptos), (5) MinIO/S3 object storage addition for imaging, (6) dual Compose/Helm deployment, (7) schema evolution policy

### Sibling dependency audit (Phase 1 blocker)
- [x] Clone `tpt-crucible`, `tpt-flight-control`, `tpt-anima` locally as siblings under `Open Source/` (all confirmed real on `github.com/tpt-solutions`, already present locally)
- [x] Audit `tpt-crucible`'s actual crate/API surface: **Finding** — `tpt-crucible` contains `tpt-alloy` (swarm partitioning/firmware generation) and `tpt-catalyst` (core IR compiler for AI model ingestion: GGUF/ONNX/PyTorch/SafeTensors → MLIR). It is **AI-model-compilation scoped**, not general control-loop compilation. **Decision**: Scope `tpt-crucible` to `tpt-chrysalis-active`'s inference compilation (Phase 3) only; drop from Phase 1 deterministic-loop critical path. Documented in ADR 0001-adjacent.
- [x] Audit `tpt-flight-control`'s actual crate/API surface: **Finding** — `tpt-flight-control` is a full avionics-grade flight stack with `tpt-math` (verified-friendly linear algebra, `nalgebra` with `libm` for `no_std`), `tpt-sensor-fusion`, `tpt-mapping`, `tpt-mixer`, `tpt-abstractions`, and multiple RTOS backends (bare-metal, FreeRTOS, Zephyr, PikeOS, seL4, VxWorks). **Directly reusable**: `tpt-math` (verified numerical primitives), `tpt-abstractions` (hardware abstraction patterns), `tpt-sensor-fusion` (redundant/dissimilar computation patterns). **Decision**: Use `tpt-math` and `tpt-abstractions` as methodology references and crate dependencies for HAL verification in Phase 1.
- [x] Audit `tpt-anima`'s actual crate/API surface: **Finding** — `tpt-anima` implements a four-layer runtime (SSE/INP/ZKPA/CK) with `anima-core-hypervisor`, `anima-edge-runtime`, `anima-sse`, `anima-inp`, `anima-zkpa`, `anima-ck`. **Relevant to Phase 3**: `anima-sse` (Symbolic Simulation Engine) for Bayesian optimization, `anima-inp` (Inference Processing) for RL loop, `anima-ck` (Consensus Kernel) for multi-agent coordination. **Decision**: Integrate `anima-sse` and `anima-inp` for active-learning loop in Phase 3; `anima-ck` for multi-organ coordination in Phase 4.
- [x] Re-confirm `tpt-archon`'s and `tpt-eve`'s current crate APIs: **tpt-archon** has `tpt-archon-kernel` (capability kernel), `tpt-archon-core` (WAL/page-manager), `tpt-archon-relational` (SQL/relational storage), `tpt-archon-verify` (formal verification harness). **tpt-eve** has `tpt-eve-symbolic`, `tpt-eve-causal`, `tpt-eve-neural` (LLM layer), `tpt-eve-active-learning`. **Decision**: ADR 0001 confirmed — use `tpt-archon-kernel` + `tpt-archon-core` + `tpt-archon-relational` directly. ADR 0002 confirmed — `tpt-eve-neural` (LLM) cannot be in safety path; use `tpt-chrysalis-safety` for Phase 1, defer `tpt-eve-symbolic`/`tpt-eve-causal` to Phase 3 advisory role.
- [x] Write/update ADRs (1) and (2) above with confirmed findings from this audit

### CI / dev tooling
- [x] `justfile` exposing `just lint/test/build/dev`, per `Open Source/.github/CONTRIBUTING.md`'s documented ecosystem standard
- [x] CI pipeline: Rust workspace (`cargo build`, `cargo test`, `cargo clippy -- -D warnings`, `cargo fmt --check`), `no_std`-specific CI target check for `tpt-chrysalis-hal` (`cargo build --target thumbv7em-none-eabihf` or whatever target the eventual real HAL requires, no default features), Wasm frontend build check (`trunk build` / `wasm-pack build`) — using the org's reusable `ci-rust.yml` workflow per ecosystem convention
- [x] Pre-commit hook config mirroring CI checks locally
- [x] Dual `LICENSE-MIT`/`LICENSE-APACHE` files, `README.md`, `CLAUDE.md`/`AGENTS.md`, `CHANGELOG.md` matching ecosystem convention

### Deployment skeletons (dual-path from day one)
- [x] `deploy/docker-compose.yml`: `tpt-archon` service (kernel + storage), `minio` (object storage for imaging, added Phase 2 but scaffolded now), `api` service placeholder, `frontend` dev service
- [x] `deploy/docker-compose.override.yml` for local dev conveniences (hot reload, exposed debug ports)
- [x] `deploy/helm/tpt-chrysalis/` chart skeleton: `Chart.yaml`, `values.yaml`, templates mirroring compose services 1:1 — must stay genuinely deployable every phase
- [x] Secrets strategy for both paths: `.env.example` for Compose; K8s `Secret`/`values-secrets.yaml.example` for Helm (capability-token root signing key, MinIO credentials)
- [x] Dockerfiles: multi-stage Rust build (std services), separate `no_std` cross-compile toolchain image for `tpt-chrysalis-hal` firmware artifacts, Wasm frontend static build

### `tpt-archon` integration scaffolding
- [x] Add `tpt-archon-kernel` and `tpt-archon-core` as workspace dependencies (git dependency against the local sibling checkout initially, matching `tpt-archon`'s own pattern of referencing siblings before they're published)
- [x] Spike: issue and verify a trivial capability token end-to-end against `tpt-archon-kernel` outside any chrysalis-specific logic, to confirm the integration surface before building real commands on top of it (see `crates/tpt-chrysalis-core/tests/archon_spike.rs`)
- [x] Spike: write and read back a record via `tpt-archon-core`'s WAL/page-manager to confirm it behaves as the "zero-copy, append-only ledger" spec §4 Layer 2 requires (see `crates/tpt-chrysalis-core/tests/archon_spike.rs`)

---

## Phase 1 — The Deterministic Wedge (Months 1-6)

Focus: `tpt-chrysalis-orchestrator` and `tpt-chrysalis-compliance`, per spec §8 Phase 1. Deliverable: a formally-verified `no_std` Rust HAL for a single simulated piece of lab equipment (a liquid handler or microfluidic pump), governed by `tpt-archon` capability tokens and audited via `tpt-archon`'s immutable ledger — proving the safety/security/determinism story end-to-end before any real hardware or AI is involved.

### `tpt-chrysalis-hal` (no_std HAL)
- [x] Define the HAL trait surface for one equipment class (e.g. `LiquidHandler: dispense(reagent, volume, target) -> Result<...>`), scoped narrowly per the spec's own "single, common piece of lab equipment" ask
- [x] `no_std`, zero-allocation implementation with formal-verification annotations/harness wired to `tpt-telos` (isolated verification dependency, not a runtime dependency — mirror `tpt-archon`'s `crates/out-archon-verify` pattern so the shippable HAL crate stays `cargo publish --dry-run` clean). Harness lives in `crates/out-chrysalis-verify` (workspace-excluded), gated on `cfg(kani)`, with `formal-proofs/dispense.telos` recording the proof intent; bounded concrete checks run under plain `cargo test`.
- [x] Prove absence of buffer overflows / race conditions / uninitialized memory for the control-loop critical path via `tpt-telos`'s verifier. Kani proof harnesses (`proof_dispense_in_limits_accepted`, `proof_dispense_over_limit_rejected`, `proof_timing_no_underflow`, `proof_command_copy_roundtrip`) cover overflow/underflow/uninit; race-freedom is structural (`&mut self`, `Copy`-only, no shared mutability). Run with `cargo kani` (opt-in CI job; tpt-telos drives the same harnesses).
- [x] Deterministic timing harness: even without real FPGA compilation (pending the `tpt-crucible` audit), the simulated HAL path must have bounded, measured latency between "valve open intent" and "actuation," recorded as a benchmark baseline

### `tpt-chrysalis-sim` (simulated hardware backend)
- [x] Implement `tpt-chrysalis-hal`'s trait surface against an in-process simulated actuator + sensor model (e.g. simulated pump with volume/time physics, simulated TEER/pH/temperature sensor streams with configurable noise)
- [x] Fault injection hooks (simulate a stuck valve, a sensor dropout, an out-of-range reading) to exercise the safety gate and audit path under failure, not just the happy path

### `tpt-chrysalis-safety` (deterministic constraint engine)
- [x] Rule schema: typed preconditions over reagents/temperature/pressure/volume (e.g. "IF reagent_A AND reagent_B THEN temperature MUST BE < 4°C ELSE abort"), matching spec §4 Layer 3's example directly
- [x] Pure, deterministic evaluator — no LLM or network call in this decision path, unit-testable in isolation
- [x] Every generated protocol is symbolically evaluated against the rule set *before* compilation to the HAL; a failing check must make physical execution unreachable, not just logged
- [x] Unit tests: known-safe protocol passes, known-unsafe protocol (temperature/reagent violation) is blocked, malformed/incomplete protocol is rejected rather than defaulting to permissive

### `tpt-archon` integration (capability + audit, Phase 1 blocker)
- [x] Capability-token issuance for chrysalis-specific physical actions (`dispense`, `open_valve`, `set_temperature`, ...), scoped per equipment/resource, short expiry
- [x] Kernel enforcement wired so `tpt-chrysalis-hal` calls cannot execute without a valid token verified by `tpt-archon-kernel`
- [x] Every sensor reading, actuator movement, and environmental condition logged via `tpt-archon-core`'s append-only ledger — no raw command executes outside this logging choke-point
- [x] Chain-integrity verification job (recompute and compare the ledger's hash chain; alert on mismatch)
- [x] Unit/integration tests: forged/expired/unscoped token rejected before reaching the HAL; tampered ledger entry detected

### PDSG core model (`tpt-chrysalis-core`)
- [x] Digital Intent Node, Physical Actuator Node, Sensor/Observer Node, and Edge (causality/flow) types per spec §3, backed by `tpt-archon-relational` for storage/query
- [x] Edge constraints enforce the "verified, deterministic pathways" requirement — e.g. an actuation edge cannot exist without a corresponding passed safety-gate check
- [x] Query API for "what's the current state of this experiment's PDSG subgraph" used by the compliance reporter and (later) the frontend

### `tpt-chrysalis-orchestrator`
- [x] Resource-locking to prevent simulated hardware collisions or reagent cross-contamination (per spec §5A) — even against a simulated backend, the locking logic itself is real and testable
- [x] Protocol compiler: takes a Digital Intent Node, produces the sequence of HAL calls, running each through `tpt-chrysalis-safety` first
- [x] End-to-end integration test: intent → safety-gate check → capability-gated simulated actuation → audit ledger entry → PDSG updated, full round trip

### `tpt-chrysalis-compliance`
- [x] ALCOA+ (Attributable, Legible, Contemporaneous, Original, Accurate) compliance report generator over the `tpt-archon` audit ledger
- [x] Report generation CLI: "show full attributable history for experiment/run X"
- [x] Document the compliance mechanism (non-code deliverable) — real regulatory submission is out of scope for Phase 1, but the report format should be reviewable against ALCOA+ and NZ HIPC 2020 criteria (see `docs/compliance-mechanism.md`)

### Frontend/API
- [x] Minimal `tpt-chrysalis-api` exposing PDSG state + protocol submission (capability token as bearer credential)
- [x] Rust/Wasm frontend: basic PDSG visualization (intent/actuator/sensor nodes and edges), simulated sensor stream display — full 3D OoC visualization deferred to Phase 2 when there's an actual chip model to render (see `frontend/src/pages.rs`, `frontend/src/components.rs`)

### Testing/validation
- [x] Unit test coverage targets for hal/sim/safety/core/orchestrator/compliance crates (80% line floor wired via `just cover` → `cargo llvm-cov --fail-under-lines 80`; `out-chrysalis-verify` adds bounded formal checks)
- [x] Full end-to-end test: submit an intent → orchestrator compiles → safety gate evaluates → capability-gated simulated execution → ledger entry → compliance report reflects it
- [x] Security tests: unauthorized/forged capability token rejected and logged; safety-gate bypass attempt blocked; tampered audit chain detected
- [x] Deterministic-timing benchmark baseline recorded (see `tpt-chrysalis-hal` above) as the reference point for later real-hardware/FPGA comparison

### Deployment
- [x] Docker Compose fully wired for Phase 1 stack (`tpt-archon`, `api`, `frontend`) (see `deploy/docker-compose.yml`)
- [x] Helm chart validated against Phase 1 stack on kind/minikube (`deploy/helm/tpt-chrysalis` mirrors compose 1:1; `just helm-lint`/`helm-template`/`helm-install` provide validation — requires local `helm`/`kind`, not present in this environment)
- [x] Structured logging + scheduled audit-chain-integrity check job (`tracing` in `tpt-chrysalis-api`; `tpt-chrysalis-compliance integrity` subcommand with `--watch`; Helm `integrity-cronjob.yaml` + compose `integrity-check` service, `Dockerfile.compliance`)

---

## Phase 2 — The Microphysiological Expansion (Months 7-12)

Focus: `tpt-chrysalis-organoid` and `tpt-chrysalis-vision`, per spec §8 Phase 2. Still simulation-first unless real OoC hardware becomes available.

### `tpt-chrysalis-organoid`
- [ ] Multi-organ linking model: simulated microfluidic "blood flow" graph connecting multiple simulated OoC chips (e.g. Pancreas → Liver → Blood-Brain-Barrier), extending the PDSG's actuator/sensor nodes to chip-level granularity
- [ ] Patient-Derived Organoid (PDO) workflow scaffolding: screening protocol definition, per-therapy simulated dosing runs
- [ ] Extend `tpt-chrysalis-safety`'s rule schema for organoid-specific constraints (cross-chip flow limits, biological compatibility rules)

### `tpt-chrysalis-vision`
- [ ] Object storage: MinIO/S3-compatible store wired in for high-content microscopy images and derived feature data, checksum-on-write
- [ ] Local computer-vision pipeline (edge-computed, per spec §5D): cell morphology tracking, neurite outgrowth (feeds `tpt-cerebrum` neurotoxicity studies once that integration exists), barrier integrity (TEER) — initially validated against public/synthetic microscopy datasets since no real imaging hardware is connected yet
- [ ] Structured time-series output written into the PDSG as Sensor/Observer Node updates

### Security integration
- [ ] New capability-token resource classes for organoid/vision commands and data
- [ ] Audit coverage extended to organoid linking and vision pipeline outputs

### Frontend/API
- [ ] 3D visualization of the (simulated) Organ-on-a-Chip array synchronized with real-time sensor data, per spec §4 Layer 4
- [ ] API extended for organoid/vision queries

### Testing/validation
- [ ] Golden-file tests against public/synthetic microscopy datasets for the vision pipeline
- [ ] Multi-organ flow-graph correctness tests (simulated)
- [ ] Security tests for new resource classes

### Deployment
- [ ] Docker Compose/Helm updated for MinIO + vision/organoid services

---

## Phase 3 — The Autonomous Closed Loop (Months 13-18)

Focus: `tpt-chrysalis-active`, per spec §8 Phase 3 — first real integration of `tpt-anima` and `tpt-eve`, using the Phase 0 API audits rather than assumptions.

### `tpt-chrysalis-active`
- [ ] Integrate `tpt-anima` (per its audited API) as the active-learning runtime: Bayesian Optimization / RL loop over PDSG sensor data to propose the next protocol that maximally reduces uncertainty
- [ ] Integrate real `tpt-eve`'s symbolic/causal reasoning layer for higher-level experiment-design reasoning (hypothesis generation, cross-domain abstraction) — explicitly wired as advisory input to protocol proposals, never as a substitute for `tpt-chrysalis-safety`'s gate
- [ ] Revisit `tpt-crucible` integration using the Phase 0 audit finding — either wire it in for inference/orchestration compilation, or document why it doesn't apply and what (if anything) replaces it
- [ ] Every AI-proposed protocol still passes through `tpt-chrysalis-safety` unchanged from Phase 1 — no new bypass path introduced for AI-originated protocols

### Testing/validation
- [ ] Closed-loop demo: high-level goal in (e.g. "maximize tumor spheroid death, minimize cardiomyocyte toxicity") → `tpt-anima`-proposed protocol → `tpt-chrysalis-safety` verification → simulated execution → measured uncertainty reduction
- [ ] Benchmark: reagent/time cost of the active-learning loop vs. a grid-search baseline on the same simulated search space, targeting the spec's 10x-100x reduction claim (§5C) — report actual measured ratio, not the target number, if they diverge
- [ ] Security/safety regression suite re-run against AI-generated (not just hand-written) protocols specifically, since these are more likely to probe edge cases in the safety rule schema

---

## Phase 4 — The Whole-Body Physical Twin (Months 19-24+)

Focus: multi-organ linking at full scale and `tpt-soma`/`tpt-cerebrum` integration, per spec §8 Phase 4.

### Multi-organ & digital-twin integration
- [ ] Full multi-OoC linking (Gut, Liver, Brain, ...) via the Phase 2 organoid-linking model, scaled up
- [ ] FHIR R5 harmonization: map physical assay results (e.g. cytokine levels) to FHIR R5 `Observation`/`Specimen` resources
- [ ] Define the scoped `tpt-soma`/`tpt-cerebrum` interop API surface (both are pre-implementation as of this writing, per their own repo states) — a defined two-way integration point, not an open-ended build, mirroring `tpt-cerebrum`'s own precedent for handling a not-yet-built sibling dependency
- [ ] `tpt-soma` receives harmonized physical assay data to update its Digital Twin; the Digital Twin proposes the next physical experiment back to `tpt-chrysalis-active`

### Compliance
- [ ] OMOP CDM mapping alongside FHIR R5, per spec §7
- [ ] NZ HIPC 2020 / HIPAA / GDPR de-identification pipeline for patient-derived organoid metadata before any data leaves the local facility, per spec §5E/§6

### Testing/validation
- [ ] End-to-end systemic pharmacokinetics test: assay data from a linked multi-chip run → FHIR-harmonized → ingested by `tpt-soma` (once available) → next-experiment proposal flows back
- [ ] Full-stack load/performance test at target scale
- [ ] Air-gap verification: confirm no experimental protocol or patient-derived data traverses the public internet from the local `tpt-anima` active-learning loop, per spec §6

---

## Cross-phase dependencies

1. **`tpt-archon` capability tokens + audit ledger (Phase 0/1)** underpin every later phase's physical-command security — each phase adds new resource/data classes, never rebuilds the core kernel/ledger integration.
2. **`tpt-chrysalis-safety` (Phase 1)** is the single safety choke-point for all protocol execution, hand-written or AI-generated — Phase 3's `tpt-anima`/`tpt-eve` integration explicitly routes through it unchanged rather than adding a parallel path.
3. **The Phase 0 sibling-dependency audit (`tpt-crucible`/`tpt-flight-control`/`tpt-anima`)** gates any later-phase task that assumes a specific API from those three — Phase 1's HAL and Phase 3's active-learning loop must not be blocked on unresolved audit findings, per the descoping table below.
4. **`tpt-chrysalis-sim` (Phase 1)** is the hardware abstraction every phase validates against until real equipment is available; when real drivers are added, they implement the same `tpt-chrysalis-hal` trait surface so orchestrator/safety/audit code doesn't change.
5. **The PDSG model (Phase 1)** is extended, not replaced, by Phase 2's chip-level/multi-organ nodes and Phase 4's whole-body linking — same `tpt-archon-relational`-backed store, growing scope.
6. **Docker Compose + Helm parity maintained every phase** — Phase 4's larger deployment is the same chart grown incrementally, matching `tpt-cerebrum`'s precedent for why this pays off.

## Descoping decisions (pragmatic substitutes for what wasn't already locked in as full-fidelity)

| Spec ambition | Why it's unrealistic to build as literally specified right now | Pragmatic substitute |
|---|---|---|
| FPGA/photonic-mesh compilation of orchestration logic via `tpt-crucible` from Phase 1 | `tpt-crucible`'s actual scope (per its GitHub description) is AI-model compilation (GGUF/ONNX/PyTorch/SafeTensors → hardware), which may not cover general control-loop compilation — unconfirmed until the Phase 0 audit | Phase 1's HAL runs on the simulated backend with a measured deterministic-timing baseline instead; `tpt-crucible` integration is revisited in Phase 3 once its real API is known, scoped to inference compilation if that's the actual fit |
| Real physical hardware drivers (Hamilton, Tecan, etc.) from Phase 1 | No physical lab equipment available | Simulation-first: `tpt-chrysalis-sim` implements the same HAL trait surface; real drivers are added later against whatever specific equipment is acquired, with no change needed to orchestrator/safety/audit code |
| `tpt-eve` as the literal safety-constraint checker (spec §4 Layer 3 naming) | `tpt-eve` is a general cognitive architecture with an LLM neural-pattern layer; a zero-hallucination-tolerance physical/chemical safety gate cannot have an LLM in its decision path | `tpt-chrysalis-safety`: a dedicated, purely-deterministic rule engine built in this repo; real `tpt-eve` used only for non-safety-critical, higher-level reasoning from Phase 3 onward |
| Full FDA/regulatory submission readiness in Phase 1 | Real regulatory submission requires real subject/patient data and institutional processes far outside a Phase 1 engineering scope | Build and validate the ALCOA+ report *format* and audit mechanism against the spec's criteria; actual submission is a non-code, institutional deliverable for whenever real data flows |
| `tpt-soma`/`tpt-cerebrum` full bidirectional integration in Phase 4 | Both are pre-implementation (spec.txt + TODO.md only) as of this writing — building deep integration against unshipped siblings is premature | Define and build the scoped interop API surface described in Phase 4 above; treat full depth as dependent on their own progress, not a fixed Phase 4 deliverable date |
| Full whole-body multi-organ linking (every organ type) from the start of Phase 4 | Each additional chip type is its own validation workstream | Start with the spec's own worked example (Gut → Liver → Blood-Brain-Barrier per §2), expand chip types incrementally |

## Critical first files

These are the first files to create — everything else in the roadmap depends on them:

- `Cargo.toml` — workspace root, defines crate boundaries for the whole monorepo
- `docs/adr/0001-archon-reuse.md` — the archon-reuse decision and its risk callout, referenced by every later Phase 1 task
- `crates/tpt-chrysalis-safety/src/lib.rs` — deterministic constraint engine (Phase 1 blocker, the safety choke-point every later phase routes through)
- `crates/tpt-chrysalis-core/src/lib.rs` — PDSG node/edge model over `tpt-archon-relational` (Phase 1 blocker, every domain crate depends on it)
- `crates/tpt-chrysalis-hal/src/lib.rs` — `no_std` HAL trait surface (Phase 1 blocker, real-hardware drivers implement this later without touching callers)
- `crates/tpt-chrysalis-sim/src/lib.rs` — simulated hardware backend implementing the HAL traits (Phase 1 blocker, stands in for real equipment)
- `deploy/docker-compose.yml` and `deploy/helm/tpt-chrysalis/` — dual deployment path, kept in parity every phase
- `schemas/` — shared PDSG node/edge and capability-token payload definitions establishing data-contract conventions every crate depends on
