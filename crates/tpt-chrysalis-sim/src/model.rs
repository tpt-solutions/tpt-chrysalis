//! The simulated liquid-handler physics model.

use alloc::collections::BTreeMap;
use alloc::rc::Rc;
use alloc::vec::Vec;
use core::cell::RefCell;

use tpt_chrysalis_hal::{
    ActuationOutcome, ActuationRejection, ActuatorCommand, Clock, HalLimits, HalDevice,
    ReagentId, ValveId, HAL_LIMITS,
};

use crate::fault::{FaultInjector, SimFault};
use crate::xorshift::XorShift;

/// Tuning for the simulated pump physics.
#[derive(Debug, Clone, Copy)]
pub struct SimConfig {
    /// Dispense rate in microlitres per millisecond.
    pub rate_ul_per_ms: f64,
    /// Volumetric error, expressed as a fraction of the commanded volume.
    pub volume_error_fraction: f64,
    /// Seed for reproducible error behavior.
    pub seed: u64,
}

impl Default for SimConfig {
    fn default() -> Self {
        Self {
            rate_ul_per_ms: 10.0,
            volume_error_fraction: 0.001,
            seed: 1,
        }
    }
}

/// The visible state of the simulated rig, shared between the actuator model
/// and the sensor model so readings track actuation.
#[derive(Debug, Clone)]
pub struct SimState {
    /// Delivered volume per reagent (audit-relevant aggregate).
    pub dispensed_ul: BTreeMap<ReagentId, u64>,
    /// Current valve open/closed state.
    pub valves: BTreeMap<ValveId, bool>,
    /// Current chamber setpoint, milli-degrees Celsius.
    pub setpoint_milli_c: i64,
    /// Current manifold pressure, millibar (driven by actuation).
    pub pressure_mbar: u32,
    /// A monotonically increasing tick used as a free-running clock source.
    pub tick: u64,
}

impl SimState {
    pub fn new() -> Self {
        Self {
            dispensed_ul: BTreeMap::new(),
            valves: BTreeMap::new(),
            setpoint_milli_c: 37_000,
            pressure_mbar: 1_000,
            tick: 0,
        }
    }

    pub fn total_dispensed_ul(&self) -> u64 {
        self.dispensed_ul.values().sum()
    }
}

impl Default for SimState {
    fn default() -> Self {
        Self::new()
    }
}

/// A simulated liquid handler implementing [`HalDevice`].
///
/// The volume/time physics are linear (rate × elapsed ticks). Error is
/// injected via the seeded PRNG; absolute limits come from [`HAL_LIMITS`].
#[derive(Debug, Clone)]
pub struct SimLiquidHandler<C: Clock> {
    config: SimConfig,
    state: Rc<RefCell<SimState>>,
    faults: Vec<SimFault>,
    // Reserved for real-hardware timestamp parity; the physics model itself
    // advances `SimState::tick`.
    _clock: C,
    rng: XorShift,
}

impl<C: Clock> SimLiquidHandler<C> {
    pub fn new(config: SimConfig, clock: C) -> Self {
        Self {
            config,
            state: Rc::new(RefCell::new(SimState::new())),
            faults: Vec::new(),
            _clock: clock,
            rng: XorShift::new(config.seed),
        }
    }

    /// The shared state handle; hand a clone to the sensor bank so readings
    /// track actuation.
    pub fn shared_state(&self) -> Rc<RefCell<SimState>> {
        self.state.clone()
    }

    fn has_fault(&self, f: &SimFault) -> bool {
        self.faults.contains(f)
    }

    fn volume_after_error(&mut self, volume_ul: u32) -> u32 {
        if self.config.volume_error_fraction == 0.0 {
            return volume_ul;
        }
        let frac = (self.config.volume_error_fraction * self.rng.next_f64()).min(1.0);
        let delta = ((volume_ul as f64) * frac).round() as u64;
        // Errors only ever reduce delivered volume (fail-safe bias).
        volume_ul.saturating_sub(delta.min(volume_ul as u64) as u32)
    }
}

impl<C: Clock> FaultInjector for SimLiquidHandler<C> {
    fn inject(&mut self, fault: SimFault) {
        self.faults.push(fault);
    }

    fn clear(&mut self, fault: &SimFault) {
        self.faults.retain(|f| f != fault);
    }

    fn active(&self) -> Vec<SimFault> {
        self.faults.clone()
    }
}

impl<C: Clock> HalDevice for SimLiquidHandler<C> {
    type Error = core::convert::Infallible;

    fn execute(&mut self, cmd: ActuatorCommand) -> Result<ActuationOutcome, Self::Error> {
        let outcome = match cmd {
            ActuatorCommand::Dispense(d) => {
                if self.has_fault(&SimFault::PumpBlocked(d.reagent)) {
                    return Ok(ActuationOutcome::Rejected(
                        ActuationRejection::UnknownReagent(d.reagent),
                    ));
                }
                if self.has_fault(&SimFault::WellFull(d.target)) {
                    return Ok(ActuationOutcome::Rejected(ActuationRejection::DeviceFault));
                }
                if d.volume_ul < HAL_LIMITS.min_volume_ul
                    || d.volume_ul > HAL_LIMITS.max_volume_ul
                {
                    return Ok(ActuationOutcome::Rejected(ActuationRejection::OutOfLimits));
                }
                let delivered = self.volume_after_error(d.volume_ul);
                let elapsed = (delivered as f64 / self.config.rate_ul_per_ms).round() as u64;
                let mut st = self.state.borrow_mut();
                st.tick = st.tick.saturating_add(elapsed);
                *st.dispensed_ul.entry(d.reagent).or_default() += delivered as u64;
                st.pressure_mbar =
                    1_000u32.saturating_add(delivered.saturating_mul(2).min(1_000));
                drop(st);
                ActuationOutcome::Ok
            }
            ActuatorCommand::SetValve(v) => {
                if self.has_fault(&SimFault::StuckValve(v.valve)) {
                    return Ok(ActuationOutcome::Rejected(ActuationRejection::ValveStuck(v.valve)));
                }
                self.state.borrow_mut().valves.insert(v.valve, v.open);
                self.state.borrow_mut().tick += 1;
                ActuationOutcome::Ok
            }
            ActuatorCommand::SetTemperature(t) => {
                if t.setpoint_milli_c < HAL_LIMITS.min_temp_milli_c
                    || t.setpoint_milli_c > HAL_LIMITS.max_temp_milli_c
                {
                    return Ok(ActuationOutcome::Rejected(ActuationRejection::OutOfLimits));
                }
                let mut st = self.state.borrow_mut();
                st.setpoint_milli_c = t.setpoint_milli_c;
                st.tick += 1;
                drop(st);
                ActuationOutcome::Ok
            }
        };
        self.state.borrow_mut().tick += 1;
        Ok(outcome)
    }

    fn limits(&self) -> HalLimits {
        HAL_LIMITS
    }
}

/// A clock that advances deterministically per simulated tick (for tests).
#[derive(Debug, Clone, Copy, Default)]
pub struct TickClock {
    pub ticks: u64,
}

impl TickClock {
    pub fn new() -> Self {
        Self { ticks: 0 }
    }
}

impl Clock for TickClock {
    fn now_ms(&self) -> u64 {
        self.ticks
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_chrysalis_hal::{DispenseCommand, WellId};

    #[test]
    fn dispense_updates_aggregates_within_limits() {
        let mut h = SimLiquidHandler::new(SimConfig::default(), TickClock::new());
        let r = h
            .execute(ActuatorCommand::Dispense(DispenseCommand {
                reagent: ReagentId(0),
                volume_ul: 100,
                target: WellId(1),
            }))
            .unwrap();
        assert_eq!(r, ActuationOutcome::Ok);
        let dispensed = h
            .shared_state()
            .borrow()
            .dispensed_ul
            .get(&ReagentId(0))
            .copied()
            .unwrap_or(0);
        assert!(dispensed <= 100);
        assert!(dispensed > 0);
    }

    #[test]
    fn out_of_limits_dispense_is_rejected() {
        let mut h = SimLiquidHandler::new(SimConfig::default(), TickClock::new());
        let r = h
            .execute(ActuatorCommand::Dispense(DispenseCommand {
                reagent: ReagentId(0),
                volume_ul: HAL_LIMITS.max_volume_ul + 1,
                target: WellId(1),
            }))
            .unwrap();
        assert_eq!(r, ActuationOutcome::Rejected(ActuationRejection::OutOfLimits));
    }

    #[test]
    fn stuck_valve_fault_blocks_actuation() {
        let mut h = SimLiquidHandler::new(SimConfig::default(), TickClock::new());
        let v = ValveId(3);
        h.inject(SimFault::StuckValve(v));
        let r = h
            .execute(ActuatorCommand::SetValve(tpt_chrysalis_hal::ValveCommand {
                valve: v,
                open: true,
            }))
            .unwrap();
        assert_eq!(r, ActuationOutcome::Rejected(ActuationRejection::ValveStuck(v)));
        // Clearing the fault unblocks it.
        h.clear(&SimFault::StuckValve(v));
        let r = h
            .execute(ActuatorCommand::SetValve(tpt_chrysalis_hal::ValveCommand {
                valve: v,
                open: true,
            }))
            .unwrap();
        assert_eq!(r, ActuationOutcome::Ok);
    }

    #[test]
    fn state_is_shared_between_devices() {
        let mut h = SimLiquidHandler::new(SimConfig::default(), TickClock::new());
        let shared = h.shared_state();
        assert!(shared.borrow().dispensed_ul.is_empty());
        let _ = h
            .execute(ActuatorCommand::Dispense(DispenseCommand {
                reagent: ReagentId(0),
                volume_ul: 50,
                target: WellId(1),
            }))
            .unwrap();
        assert!(!shared.borrow().dispensed_ul.is_empty());
        let _ = String::from("smoke");
    }
}
