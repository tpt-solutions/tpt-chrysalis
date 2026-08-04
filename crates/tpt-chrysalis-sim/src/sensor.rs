//! The simulated sensor bank: noisy, fault-injectable readings of the shared
//! rig state.

use alloc::rc::Rc;
use alloc::vec::Vec;
use core::cell::RefCell;

use tpt_chrysalis_hal::{Clock, SensorChannel, SensorDevice, SensorReading};

use crate::fault::{FaultInjector, SimFault};
use crate::model::SimState;
use crate::xorshift::XorShift;

/// Tuning for the sensor model.
#[derive(Debug, Clone, Copy)]
pub struct SimSensorConfig {
    /// RMS noise per channel (scaled units). `0` disables noise.
    pub noise: f64,
    pub seed: u64,
}

impl Default for SimSensorConfig {
    fn default() -> Self {
        Self {
            noise: 5.0,
            seed: 2,
        }
    }
}

/// Implements [`SensorDevice`] against the shared [`SimState`], so a
/// temperature actuation is observed as a temperature reading change.
#[derive(Debug, Clone)]
pub struct SimSensorBank<C: Clock> {
    config: SimSensorConfig,
    state: Rc<RefCell<SimState>>,
    faults: Vec<SimFault>,
    clock: C,
    rng: XorShift,
}

impl<C: Clock> SimSensorBank<C> {
    pub fn new(config: SimSensorConfig, state: Rc<RefCell<SimState>>, clock: C) -> Self {
        Self {
            config,
            state,
            faults: Vec::new(),
            clock,
            rng: XorShift::new(config.seed),
        }
    }

    fn has_fault(&self, f: &SimFault) -> bool {
        self.faults.contains(f)
    }

    fn noisy(&mut self, base: f64) -> i64 {
        if self.config.noise == 0.0 {
            return base.round() as i64;
        }
        let jitter = (self.rng.next_f64() * 2.0 - 1.0) * self.config.noise;
        (base + jitter).round() as i64
    }

    fn base_value(&self, channel: SensorChannel) -> f64 {
        let st = self.state.borrow();
        match channel {
            SensorChannel::PhMilli => 7_400.0,
            SensorChannel::TemperatureMilliC => st.setpoint_milli_c as f64,
            SensorChannel::PressureMbar => st.pressure_mbar as f64,
            SensorChannel::TeerMilliOhm => 1_000.0,
        }
    }

    /// A value guaranteed to be outside every plausible physical band (used by
    /// the out-of-range fault to exercise the safety gate).
    fn out_of_range_value(channel: SensorChannel) -> i64 {
        match channel {
            SensorChannel::PhMilli => 99_000,
            SensorChannel::TemperatureMilliC => 999_000,
            SensorChannel::PressureMbar => 9_999_000,
            SensorChannel::TeerMilliOhm => 99_000_000,
        }
    }
}

impl<C: Clock> FaultInjector for SimSensorBank<C> {
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

/// Terminal sensor error: the channel dropped out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SensorError {
    Dropout(SensorChannel),
}

impl core::fmt::Display for SensorError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl<C: Clock> SensorDevice for SimSensorBank<C> {
    type Error = SensorError;

    fn read(&mut self, channel: SensorChannel) -> Result<SensorReading, Self::Error> {
        if self.has_fault(&SimFault::SensorDropout(channel)) {
            return Err(SensorError::Dropout(channel));
        }
        let value_scaled = if self.has_fault(&SimFault::SensorOutOfRange(channel)) {
            Self::out_of_range_value(channel)
        } else {
            self.noisy(self.base_value(channel))
        };
        Ok(SensorReading {
            channel,
            value_scaled,
            at_ms: self.clock.now_ms(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rig() -> (SimSensorBank<super::super::model::TickClock>, Rc<RefCell<SimState>>) {
        use crate::model::{SimConfig, SimLiquidHandler, TickClock};
        let handler = SimLiquidHandler::new(SimConfig::default(), TickClock::new());
        let state = handler.shared_state();
        let bank = SimSensorBank::new(
            SimSensorConfig { noise: 0.0, seed: 2 },
            state.clone(),
            TickClock::new(),
        );
        (bank, state)
    }

    #[test]
    fn temperature_tracks_setpoint() {
        let (mut bank, state) = rig();
        state.borrow_mut().setpoint_milli_c = 10_000;
        let r = bank.read(SensorChannel::TemperatureMilliC).unwrap();
        assert_eq!(r.value_scaled, 10_000);
    }

    #[test]
    fn dropout_is_an_error() {
        let (mut bank, _) = rig();
        bank.inject(SimFault::SensorDropout(SensorChannel::PhMilli));
        assert!(matches!(
            bank.read(SensorChannel::PhMilli),
            Err(SensorError::Dropout(SensorChannel::PhMilli))
        ));
    }

    #[test]
    fn out_of_range_reading_is_extreme() {
        let (mut bank, _) = rig();
        bank.inject(SimFault::SensorOutOfRange(SensorChannel::TemperatureMilliC));
        let r = bank.read(SensorChannel::TemperatureMilliC).unwrap();
        assert_eq!(r.value_scaled, 999_000);
    }
}
