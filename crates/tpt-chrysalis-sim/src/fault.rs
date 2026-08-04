//! Fault injection for the simulator.

use tpt_chrysalis_hal::{ReagentId, SensorChannel, ValveId, WellId};

/// An injected fault, mirroring real failure modes the safety gate and audit
/// path must survive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimFault {
    /// The valve is stuck (cannot change state).
    StuckValve(ValveId),
    /// The sensor returns no data (dropout).
    SensorDropout(SensorChannel),
    /// The sensor returns an out-of-range reading.
    SensorOutOfRange(SensorChannel),
    /// The pump rejects everything for the given reagent.
    PumpBlocked(ReagentId),
    /// The given well no longer accepts volumes.
    WellFull(WellId),
}

/// Enables injecting and clearing faults into a simulator.
pub trait FaultInjector {
    fn inject(&mut self, fault: SimFault);
    fn clear(&mut self, fault: &SimFault);
    fn active(&self) -> alloc::vec::Vec<SimFault>;
}
