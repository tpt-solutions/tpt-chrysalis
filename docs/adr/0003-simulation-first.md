# ADR 0003: Simulation-First Hardware Strategy for Phase 1

**Status**: Accepted
**Date**: 2026-08-05

## Context

The specification (§8 Phase 1) asks for a "formally-verified `no_std` Rust HAL for a single simulated piece of lab equipment (a liquid handler or microfluidic pump)" governed by capability tokens and audited via the immutable ledger.

No physical lab equipment (Hamilton, Tecan, microfluidic pumps, FPGAs) is available in the development environment. The spec acknowledges this by describing Phase 1's deliverable as proving the "safety/security/determinism story end-to-end before any real hardware or AI is involved."

## Decision

**Build and validate the entire Phase 1 stack (HAL, capability gating, safety gate, audit ledger, orchestrator, PDSG) against a software-simulated actuator/sensor backend (`tpt-chrysalis-sim`).** Real hardware drivers are added later against whatever specific equipment becomes available.

Specifically:
- `tpt-chrysalis-hal` defines the `no_std` trait surface (e.g., `LiquidHandler::dispense`, `SensorDevice::read`)
- `tpt-chrysalis-sim` implements those traits against an in-process model with:
  - Pump volume/time physics (simulated fluid dynamics)
  - Sensor streams (TEER, pH, temperature) with configurable noise
  - Fault injection hooks (stuck valve, sensor dropout, out-of-range reading)
- The orchestrator, safety gate, capability enforcement, and audit ledger drive the **simulated** HAL exactly as they would real hardware
- Deterministic timing is measured on the simulated path (valve-open intent → actuation latency) as a baseline for future FPGA/real-hardware comparison
- When real drivers are added, they implement the same `tpt-chrysalis-hal` traits — **no changes** to orchestrator, safety, audit, or PDSG code

## Consequences

### Positive
- **Phase 1 unblocked** — no hardware procurement wait; the deterministic wedge delivers on schedule
- **Testability** — fault injection exercises safety gate and audit path under failure, not just happy path
- **Hardware abstraction purity** — the HAL trait surface is validated as truly hardware-agnostic before any real driver exists
- **CI/CD friendly** — the full stack runs in GitHub Actions with no special hardware runners

### Negative / Risks
- **Simulation fidelity gap** — the simulated physics may not capture real-world edge cases (cavitation, clogging, thermal drift)
- **Timing baseline mismatch** — simulated actuation latency on a host CPU ≠ FPGA/microcontroller timing
- **False confidence** — passing tests on the simulator don't guarantee real hardware behaves identically

### Mitigations
- Document the simulation's fidelity limits explicitly in `tpt-chrysalis-sim`'s README
- Measure and record the deterministic-timing baseline on the simulator as a **reference point**, not a guarantee
- When real hardware arrives, run the **same integration test suite** against the real driver to catch divergences
- Keep fault-injection hooks in the simulator as a permanent regression tool even after real hardware exists

## Alternatives Considered

1. **Wait for real hardware** — rejected: blocks Phase 1 indefinitely; the spec explicitly describes Phase 1 as simulation-validated
2. **Build a minimal mock instead of a physics-based simulator** — rejected: a mock can't exercise the safety gate with realistic sensor noise/faults; the spec asks for "configurable noise" and "fault injection"
3. **Use a third-party lab automation simulator** — rejected: introduces external dependency; the HAL trait surface is simple enough that an in-process model is lower friction and fully controllable

## References
- `spec.txt` §8 Phase 1: "Deliverable: a formally-verified `no_std` Rust HAL for a single **simulated** piece of lab equipment"
- `spec.txt` §4 Layer 1: HAL trait surface
- `tpt-chrysalis-hal` crate: `crates/tpt-chrysalis-hal/src/lib.rs`
- `tpt-chrysalis-sim` crate: `crates/tpt-chrysalis-sim/src/lib.rs`
- ADR 0001 (archon reuse — the capability/audit layer is the same for sim and real)
- ADR 0005 (MinIO for imaging — simulation extends to vision in Phase 2)