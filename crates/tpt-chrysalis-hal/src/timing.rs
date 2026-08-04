//! Deterministic timing primitives for the control loop.
//!
//! The spec's determinism claim (spec.txt §2, §4 Layer 1) is anchored on a
//! *measured* budget between "valve open intent" and "actuation". Even with no
//! real hardware or FPGA (see ADR 0003), the simulated path records that
//! latency so a baseline exists for later real-hardware comparison.

/// A monotonically increasing instant in milliseconds since an arbitrary epoch.
/// Implementations must never go backwards.
pub trait Clock {
    fn now_ms(&self) -> u64;
}

/// A host-side `Clock` for sim/test/benchmark use.
#[derive(Debug, Clone, Copy, Default)]
pub struct SimInstant;

impl Clock for SimInstant {
    fn now_ms(&self) -> u64 {
        #[cfg(feature = "std")]
        {
            use std::time::{SystemTime, UNIX_EPOCH};
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64
        }
        #[cfg(not(feature = "std"))]
        {
            0
        }
    }
}

/// The measured window for one actuation: from the orchestrator's intent
/// timestamp through the device's acknowledgement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ActuationTiming {
    /// Wall-clock time when the orchestrator issued the command.
    pub intent_ms: u64,
    /// Device-reported completion time.
    pub actuation_ms: u64,
}

impl ActuationTiming {
    /// The measured latency in microseconds (0 if the clock did not advance).
    pub fn latency_us(&self) -> u64 {
        self.actuation_ms.saturating_sub(self.intent_ms) * 1000
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latency_is_forward_and_micros() {
        let t = ActuationTiming {
            intent_ms: 1_000,
            actuation_ms: 1_010,
        };
        assert_eq!(t.latency_us(), 10_000);
    }
}
