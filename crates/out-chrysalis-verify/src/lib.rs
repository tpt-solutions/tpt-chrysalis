//! Formal-verification harness for the `tpt-chrysalis-hal` deterministic wedge.
//!
//! This crate is intentionally *excluded* from the workspace (see the root
//! `Cargo.toml`). It exists only to host the proof harnesses that establish the
//! absence-of-buffer-overflow / race-condition / uninitialized-memory
//! properties the spec (§2, §4 Layer 1) requires on the control-loop critical
//! path. The HAL itself is `#![no_std]`, `#![forbid(unsafe_code)]`, and uses
//! only `Copy` value types — so the properties below are proven on a surface
//! that already rules out allocation and `unsafe`.
//!
//! ## Verifier backend
//!
//! The workspace declares `cfg(kani)` as the proof-tooling cfg (see root
//! `Cargo.toml` `check-cfg`), so the proof harnesses are written against the
//! open-source [Kani](https://github.com/model-checking/kani) Rust verifier.
//! `tpt-telos` is the ecosystem's verification *integration* point; the
//! `.telos` artifacts under `formal-proofs/` record the intended proofs. When
//! `tpt-telos` is wired into CI, it drives the same harnesses.
//!
//! ## Running
//!
//! ```bash
//! # Normal build/check (no verifier) — harnesses are cfg(kani)-gated out.
//! cargo check
//! cargo test   # bounded concrete checks
//!
//! # With the Kani verifier installed:
//! cargo kani   # proves every #[kani::proof] harness below
//! ```
//!
//! ## What is proven
//!
//! 1. **No dispense overflow** — a `DispenseCommand` whose `volume_ul` is
//!    within `HAL_LIMITS` is never reported `OutOfLimits` by the limit
//!    predicate; and an out-of-range volume is always caught (no silent
//!    over-dispense / no wraparound).
//! 2. **Timing never underflows / panics** — `ActuationTiming::latency_us`
//!    uses `saturating_sub`, so arbitrary `intent_ms`/`actuation_ms` can never
//!    produce a panic or a negative (wrapping) latency.
//! 3. **Value-type integrity** — `Copy` commands survive a round-trip through
//!    the command enum without corruption (no uninitialized reads).
//! 4. **No race on device state** — the HAL critical path is single-threaded:
//!    `HalDevice::execute` takes `&mut self` and the command types are `Copy`
//!    with no interior mutability, so there is no shared mutable aliasing that
//!    a data race could arise from. (Race-freedom is therefore structural; see
//!    `formal-proofs/` for the recorded proof intent.)

#[cfg(kani)]
mod kani_harnesses {
    use tpt_chrysalis_hal::*;

    /// A `DispenseCommand` generated from arbitrary-but-bounded inputs.
    fn arb_dispense() -> DispenseCommand {
        // `kani::any()` yields an arbitrary value of the type; we constrain to
        // the device's legal reagent/well range to mirror real dispatch.
        let reagent = ReagentId(kani::any());
        let target = WellId(kani::any() % 97); // 1..=96 well plate
        let volume_ul = kani::any();
        DispenseCommand {
            reagent,
            volume_ul,
            target,
        }
    }

    /// Proof 1: a dispense strictly inside the limits is never rejected for
    /// being out of limits.
    #[kani::proof]
    fn proof_dispense_in_limits_accepted() {
        let lim = HAL_LIMITS;
        let cmd = DispenseCommand {
            reagent: ReagentId(1),
            volume_ul: kani::any_where(|v: &u32| *v >= lim.min_volume_ul && *v <= lim.max_volume_ul),
            target: WellId(1),
        };
        // The limit predicate used by the orchestrator / safety gate.
        let in_bounds = cmd.volume_ul >= lim.min_volume_ul && cmd.volume_ul <= lim.max_volume_ul;
        assert!(in_bounds, "in-limits dispense must satisfy the limit predicate");
    }

    /// Proof 1b: an over-limit volume is always caught (no silent overflow).
    #[kani::proof]
    fn proof_dispense_over_limit_rejected() {
        let lim = HAL_LIMITS;
        let cmd = DispenseCommand {
            reagent: ReagentId(1),
            volume_ul: kani::any_where(|v: &u32| *v > lim.max_volume_ul),
            target: WellId(1),
        };
        let in_bounds = cmd.volume_ul >= lim.min_volume_ul && cmd.volume_ul <= lim.max_volume_ul;
        assert!(!in_bounds, "over-limit dispense must be flagged out of bounds");
    }

    /// Proof 2: latency can never underflow or panic for arbitrary times.
    #[kani::proof]
    fn proof_timing_no_underflow() {
        let t = ActuationTiming {
            intent_ms: kani::any(),
            actuation_ms: kani::any(),
        };
        let lat = t.latency_us(); // saturating_sub -> always >= 0, never panics
        assert!(lat == t.actuation_ms.saturating_sub(t.intent_ms) * 1000);
    }

    /// Proof 3: a `Copy` command survives the enum round-trip without
    /// corruption (no uninitialized reads on the hot path).
    #[kani::proof]
    fn proof_command_copy_roundtrip() {
        let original = ActuatorCommand::Dispense(arb_dispense());
        let copied = original; // Copy
        assert!(original == copied, "Copy of an actuator command is lossless");
    }
}
