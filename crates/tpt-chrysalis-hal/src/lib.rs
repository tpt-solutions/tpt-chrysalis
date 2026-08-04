//! `tpt-chrysalis-hal`: the `no_std` Hardware Abstraction Layer.
//!
//! This is Phase 1's deterministic wedge. It defines the trait surface for a
//! single, common piece of lab equipment (a simulated liquid handler) that the
//! `tpt-chrysalis` orchestrator, safety gate, capability kernel, and audit
//! ledger all drive. Real hardware drivers implement these same traits later
//! without any change to orchestrator/safety/audit code (see `spec.txt` §4
//! Layer 1 and ADR 0003).
//!
//! # Safety posture
//!
//! The control-loop critical path here is `#![no_std]`, forbids `unsafe` code,
//! and uses only `Copy` value types — no buffers, no allocation, no dynamic
//! dispatch on the hot path. The absence-of-buffer-overflow /
//! race-condition / uninitialized-memory properties are proven on this trait
//! surface by the `tpt-telos`-wired harness in `crates/out-chrysalis-verify`
//! (with `.telos` artifacts under `formal-proofs/`), mirroring `tpt-archon`'s
//! `out-archon-verify` pattern so this crate stays publish-clean.
#![cfg_attr(not(feature = "std"), no_std)]
#![forbid(unsafe_code)]

pub mod actuator;
pub mod sensor;
pub mod timing;

pub use actuator::{
    ActuationOutcome, ActuationRejection, ActuatorCommand, DispenseCommand, HalDevice,
    HalLimits, ReagentId, TemperatureCommand, ValveCommand, ValveId, WellId, HAL_LIMITS,
};
pub use sensor::{SensorChannel, SensorDevice, SensorReading};
pub use timing::{ActuationTiming, Clock, SimInstant};
