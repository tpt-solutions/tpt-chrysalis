//! Sensor surface: the observation channels and the trait that backs the PDSG
//! Sensor/Observer nodes.

use core::fmt;

/// The observation channels the Phase 1 simulated rig exposes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum SensorChannel {
    /// pH probe, scaled x1000.
    PhMilli,
    /// Environmental temperature, milli-degrees Celsius.
    TemperatureMilliC,
    /// Manifold pressure, millibar.
    PressureMbar,
    /// TEER impedance (organoid Phase 2), milli-ohms.
    TeerMilliOhm,
}

impl fmt::Display for SensorChannel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SensorChannel::PhMilli => write!(f, "ph_milli"),
            SensorChannel::TemperatureMilliC => write!(f, "temperature_milli_c"),
            SensorChannel::PressureMbar => write!(f, "pressure_mbar"),
            SensorChannel::TeerMilliOhm => write!(f, "teer_milli_ohm"),
        }
    }
}

/// A single observation, scaled to integer units so the audit ledger and the
/// safety gate stay exact (no float drift in the decision path).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SensorReading {
    pub channel: SensorChannel,
    pub value_scaled: i64,
    pub at_ms: u64,
}

/// The deterministic observation surface. Like [`HalDevice`], implementations
/// must be pure with respect to input + internal state.
pub trait SensorDevice {
    /// Device-specific terminal error.
    type Error: fmt::Debug + Copy;

    fn read(&mut self, channel: SensorChannel) -> Result<SensorReading, Self::Error>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channels_are_distinct_serializable_tags() {
        assert_ne!(SensorChannel::PhMilli as u8, SensorChannel::TemperatureMilliC as u8);
        assert_eq!(SensorChannel::TeerMilliOhm as u8, 3);
    }
}
