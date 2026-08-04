//! Actuator surface: the commands, outcomes, and trait for the deterministic
//! control-loop critical path.
//!
//! All command/outcome types are `Copy` value types: the orchestrator compiles
//! a protocol into a flat list of these and nothing in the loop allocates.

use core::fmt;

/// A reagent identifier (index into the lab's reagent registry).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct ReagentId(pub u8);

/// A target well (1-based; a 96-well plate is well ids 1..=96).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct WellId(pub u16);

/// A valve on the fluidic manifold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct ValveId(pub u8);

/// Physical limits the HAL (and every real driver) must enforce in hardware.
/// The `out-chrysalis-verify` harness proves these bounds are never exceeded
/// by the compiled command stream (see `formal-proofs/dispense.telos`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HalLimits {
    /// Minimum dispense volume in microlitres (inclusive).
    pub min_volume_ul: u32,
    /// Maximum dispense volume in microlitres (inclusive).
    pub max_volume_ul: u32,
    /// Maximum liquid-handler pressure in millibar.
    pub max_pressure_mbar: u32,
    /// Valid temperature range in milli-degrees Celsius.
    pub min_temp_milli_c: i64,
    pub max_temp_milli_c: i64,
}

/// The default limits for the simulated liquid handler. Every real driver must
/// define its own `HalLimits`; callers gate on the device's reported limits.
pub const HAL_LIMITS: HalLimits = HalLimits {
    min_volume_ul: 1,
    max_volume_ul: 1000,
    max_pressure_mbar: 2_000,
    min_temp_milli_c: -20_000,
    max_temp_milli_c: 90_000,
};

/// A dispense command: move `volume_ul` of `reagent` into `target`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DispenseCommand {
    pub reagent: ReagentId,
    pub volume_ul: u32,
    pub target: WellId,
}

/// A valve command: open or close `valve`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValveCommand {
    pub valve: ValveId,
    pub open: bool,
}

/// A temperature command: drive the environmental chamber to `setpoint_milli_c`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TemperatureCommand {
    pub setpoint_milli_c: i64,
}

/// Every physical action the deterministic loop can ask of the device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActuatorCommand {
    Dispense(DispenseCommand),
    SetValve(ValveCommand),
    SetTemperature(TemperatureCommand),
}

/// Why a device refused to execute a command. Refusals are surfaced (and
/// audited), never swallowed — a stuck valve must be observable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActuationRejection {
    /// The command violates the device's physical limits.
    OutOfLimits,
    /// The referenced valve is stuck (fault-injected or real).
    ValveStuck(ValveId),
    /// The referenced well is out of range.
    UnknownWell(WellId),
    /// The referenced reagent is not loaded.
    UnknownReagent(ReagentId),
    /// The device is in an inconsistent state (e.g. overpressure).
    DeviceFault,
}

impl fmt::Display for ActuationRejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ActuationRejection::OutOfLimits => write!(f, "command exceeds physical limits"),
            ActuationRejection::ValveStuck(v) => write!(f, "valve {v:?} is stuck"),
            ActuationRejection::UnknownWell(w) => write!(f, "unknown well {w:?}"),
            ActuationRejection::UnknownReagent(r) => write!(f, "unknown reagent {r:?}"),
            ActuationRejection::DeviceFault => write!(f, "device fault"),
        }
    }
}

/// The result of a single actuation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActuationOutcome {
    /// The command executed and the device state advanced.
    Ok,
    /// The device refused; see [`ActuationRejection`].
    Rejected(ActuationRejection),
}

/// The deterministic actuation surface. Implementations must be pure with
/// respect to the input command plus their own state — no network, no
/// scheduling, no allocation.
pub trait HalDevice {
    /// Device-specific terminal error (I/O fault etc.). Must remain `Copy`.
    type Error: fmt::Debug + Copy;

    /// Executes a command, returning a refusal-or-success [`ActuationOutcome`].
    fn execute(&mut self, cmd: ActuatorCommand) -> Result<ActuationOutcome, Self::Error>;

    /// The physical limits this device enforces; used by the orchestrator's
    /// compile-time checks and the safety gate.
    fn limits(&self) -> HalLimits;
}

#[cfg(test)]
mod tests {
    use super::*;

    // Compile-time physical sanity: HAL_LIMITS is a const, so these are
    // verified before any runtime test runs.
    const _: () = {
        assert!(HAL_LIMITS.min_volume_ul <= HAL_LIMITS.max_volume_ul);
        assert!(HAL_LIMITS.min_temp_milli_c < HAL_LIMITS.max_temp_milli_c);
        assert!(HAL_LIMITS.max_pressure_mbar > 0);
    };

    #[test]
    fn commands_are_copy_value_types() {
        let cmd = ActuatorCommand::Dispense(DispenseCommand {
            reagent: ReagentId(0),
            volume_ul: 100,
            target: WellId(1),
        });
        // Copy semantics: using `cmd` twice must not move it.
        let _a = cmd;
        assert!(matches!(cmd, ActuatorCommand::Dispense(_)));
    }
}
