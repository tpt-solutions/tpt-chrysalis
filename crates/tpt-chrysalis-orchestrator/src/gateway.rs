//! The capability-gated hardware gateway: the enforcement point between the
//! orchestrator and the HAL.
//!
//! Mirrors `tpt-archon-kernel`'s `MessageRouter` pattern: every actuation is
//! framed as a kernel [`Message`] addressed to the action's channel and only
//! delivered when the presented archon [`Capability`] authorizes a write to
//! that channel (live revocation against the shared issuer included). On top
//! of the kernel check the gateway enforces the chrysalis token's short
//! expiry and action-scope. Only after all three gates does the command reach
//! the [`HalDevice`] — a compromised orchestrator process cannot actuate
//! without the token.

use alloc::string::{String, ToString};

use tpt_archon_bridge::capability::SharedIssuer;
use tpt_archon_kernel::ipc::{IpcError, Message, MessageRouter};
use tpt_chrysalis_core::ledger::PhysicalAction;
use tpt_chrysalis_core::security::{ActionId, ActionToken};
use tpt_chrysalis_hal::{
    ActuationOutcome, ActuationTiming, ActuatorCommand, Clock, HalDevice, SensorChannel,
    SensorDevice, SensorReading,
};

/// Why the gateway refused a command before (or while) reaching the HAL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GatewayError {
    /// The archon capability was not authorized for this action/channel
    /// (forged, revoked, or wrong resource).
    Unauthorized,
    /// `now > token.expires_at`.
    Expired,
    /// The token authorizes a different action.
    WrongAction,
    /// The action could not be mapped to a capability channel.
    UnknownAction,
    /// The kernel `MessageRouter` refused delivery.
    KernelDenied,
    /// A registered channel was missing.
    NoSuchChannel,
    /// The HAL device returned a terminal error.
    DeviceFailed(String),
    /// The sensor device returned a terminal error.
    SensorFailed(String),
}

impl core::fmt::Display for GatewayError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}

/// Wraps a [`HalDevice`] + [`SensorDevice`] behind the kernel's capability
/// router.
#[derive(Debug)]
pub struct HardwareGateway<D, S, C> {
    device: D,
    sensors: S,
    router: MessageRouter,
    scope: String,
    clock: C,
}

impl<D, S, C> HardwareGateway<D, S, C>
where
    D: HalDevice,
    S: SensorDevice,
    C: Clock,
{
    /// Registers every chrysalis action channel with the kernel router and
    /// wires it to the shared issuer so revocations are live.
    pub fn new(device: D, sensors: S, clock: C, issuer: SharedIssuer, scope: &str) -> Self {
        let mut router = MessageRouter::new(issuer);
        for action in [
            ActionId::Dispense,
            ActionId::OpenValve,
            ActionId::SetTemperature,
            ActionId::ReadSensor,
        ] {
            router.register_channel(action.channel_id());
        }
        Self {
            device,
            sensors,
            router,
            scope: scope.to_string(),
            clock,
        }
    }

    fn enforce(&self, token: &ActionToken, action: ActionId, now_ms: u64) -> Result<(), GatewayError> {
        if token.scope != self.scope {
            return Err(GatewayError::Unauthorized);
        }
        if token.action != action {
            return Err(GatewayError::WrongAction);
        }
        if token.is_expired(now_ms) {
            return Err(GatewayError::Expired);
        }
        // The kernel router performs the capability check (issuer + resource +
        // right) inside `send`; reaching this point means the token verified.
        Ok(())
    }

    fn route(&mut self, token: &ActionToken, action: ActionId, payload: Vec<u8>) -> Result<(), GatewayError> {
        self.router
            .send(
                &token.capability,
                Message {
                    channel: action.channel_id(),
                    payload,
                },
            )
            .map_err(|e| match e {
                IpcError::Denied => GatewayError::KernelDenied,
                IpcError::NoSuchChannel => GatewayError::NoSuchChannel,
            })
    }

    /// Executes one actuation iff the token clears the kernel router and the
    /// chrysalis expiry/scope checks. Returns the HAL outcome plus the
    /// measured intent→actuation window.
    pub fn actuate(
        &mut self,
        token: &ActionToken,
        cmd: ActuatorCommand,
        action: &PhysicalAction,
        now_ms: u64,
    ) -> Result<(ActuationOutcome, ActuationTiming), GatewayError> {
        let action_id = ActionId::from_action(action).ok_or(GatewayError::UnknownAction)?;
        self.enforce(token, action_id, now_ms)?;
        let intent_ms = self.clock.now_ms();
        self.route(token, action_id, alloc::format!("{cmd:?}").into_bytes())?;
        let outcome = self
            .device
            .execute(cmd)
            .map_err(|e| GatewayError::DeviceFailed(alloc::format!("{e:?}")))?;
        let actuation_ms = self.clock.now_ms();
        Ok((
            outcome,
            ActuationTiming {
                intent_ms,
                actuation_ms,
            },
        ))
    }

    /// Reads one sensor channel, token-gated like actuation. Observation is
    /// the read-side of the kernel: the [`MessageRouter::receive`] path
    /// enforces the read capability (and live revocation) against the shared
    /// issuer, mirroring how actuation is gated by `send`.
    pub fn read_sensor(
        &mut self,
        token: &ActionToken,
        channel: SensorChannel,
        now_ms: u64,
    ) -> Result<SensorReading, GatewayError> {
        self.enforce(token, ActionId::ReadSensor, now_ms)?;
        // Read-capability enforcement through the kernel router (revocation
        // visible); the inbox is empty because reads carry no message body.
        self.router
            .receive(&token.capability, ActionId::ReadSensor.channel_id())
            .map_err(|e| match e {
                IpcError::Denied => GatewayError::KernelDenied,
                IpcError::NoSuchChannel => GatewayError::NoSuchChannel,
            })?;
        self.sensors
            .read(channel)
            .map_err(|e| GatewayError::SensorFailed(alloc::format!("{e:?}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_chrysalis_core::security::{ActionId, TokenAuthority};
    use tpt_chrysalis_hal::{ActuatorCommand, DispenseCommand, ReagentId, WellId};
    use tpt_chrysalis_sim::model::{SimConfig, SimLiquidHandler, TickClock};
    use tpt_chrysalis_sim::sensor::{SimSensorBank, SimSensorConfig};

    fn gateway_rig(scope: &str) -> (HardwareGateway<SimLiquidHandler<TickClock>, SimSensorBank<TickClock>, TickClock>, TokenAuthority) {
        let authority = TokenAuthority::new(scope);
        let issuer = authority.issuer();
        let device = SimLiquidHandler::new(SimConfig::default(), TickClock::new());
        let state = device.shared_state();
        let sensors = SimSensorBank::new(
            SimSensorConfig {
                noise: 0.0,
                seed: 2,
            },
            state,
            TickClock::new(),
        );
        let gateway = HardwareGateway::new(device, sensors, TickClock::new(), issuer, scope);
        (gateway, authority)
    }

    fn dispense_cmd() -> ActuatorCommand {
        ActuatorCommand::Dispense(DispenseCommand {
            reagent: ReagentId(0),
            volume_ul: 100,
            target: WellId(1),
        })
    }

    #[test]
    fn gateway_round_trip_with_valid_token() {
        let scope = "equipment/gw-test";
        let (mut gateway, mut authority) = gateway_rig(scope);
        let token = authority.issue(ActionId::Dispense, "alice", 1_000, 2_000);
        let action = PhysicalAction::Dispense {
            reagent: "A".into(),
            volume_ul: 100,
            target: "w1".into(),
        };
        let (outcome, timing) = gateway.actuate(&token, dispense_cmd(), &action, 1_500).unwrap();
        assert_eq!(outcome, tpt_chrysalis_hal::ActuationOutcome::Ok);
        // The TickClock is a free-running counter that never advances during
        // a single execute, so the measured window is exactly zero here.
        assert_eq!(timing.latency_us(), 0);
    }

    #[test]
    fn gateway_rejects_expired_token_before_hal() {
        let scope = "equipment/gw-test";
        let (mut gateway, mut authority) = gateway_rig(scope);
        let token = authority.issue(ActionId::Dispense, "alice", 1_000, 1_500);
        let action = PhysicalAction::Dispense {
            reagent: "A".into(),
            volume_ul: 100,
            target: "w1".into(),
        };
        assert_eq!(
            gateway.actuate(&token, dispense_cmd(), &action, 1_501),
            Err(GatewayError::Expired)
        );
    }
}
