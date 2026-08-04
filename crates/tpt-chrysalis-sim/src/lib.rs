//! `tpt-chrysalis-sim`: simulated hardware backend.
//!
//! Implements the `tpt-chrysalis-hal` trait surface against an in-process
//! model of a liquid handler (pump volume/time physics) plus a sensor bank
//! with configurable noise and fault-injection hooks (stuck valve, sensor
//! dropout, out-of-range reading). Every real driver added later implements
//! the same traits, so orchestrator/safety/audit code never changes - see
//! ADR 0003 (simulation-first).
extern crate alloc;

pub mod fault;
pub mod model;
pub mod sensor;
pub mod xorshift;

pub use fault::{FaultInjector, SimFault};
pub use model::{SimConfig, SimLiquidHandler, SimState};
pub use sensor::{SimSensorBank, SimSensorConfig};
