//! The orchestrator: the end-to-end Phase 1 flow
//! `intent → compile → gate → locked, token-gated execution → audit → PDSG`.

use alloc::string::String;
use alloc::vec::Vec;

use tpt_chrysalis_core::ledger::{AuditLedger, AuditOutcome, LedgerOutcome, PhysicalAction};
use tpt_chrysalis_core::pdsg::{
    ActuatorNode, Edge, EdgeId, EdgeKind, IntentNode, IntentStatus, Node, NodeId, PdsgStore,
    SensorNode, SensorSample, StoreError,
};
use tpt_chrysalis_core::security::{ActionId, TokenAuthority};
use tpt_chrysalis_hal::{
    ActuationRejection, ActuatorCommand, Clock, DispenseCommand, HalDevice, SensorChannel,
    SensorDevice, TemperatureCommand, ValveCommand,
};
use tpt_chrysalis_safety::environment::Environment;
use tpt_chrysalis_safety::gate::{GateDecision, SafetyGate};
use tpt_chrysalis_safety::protocol::{Protocol, StepAction};

use crate::compiler::{CompileError, ProtocolCompiler};
use crate::gateway::{GatewayError, HardwareGateway};
use crate::lock::{LockError, LockManager, ResourceId};
use crate::registry::{ReagentRegistry, ValveRegistry, WellRegistry};

/// One accepted actuation, recorded in the report.
#[derive(Debug, Clone, PartialEq)]
pub struct AcceptedStep {
    /// Ledger sequence number of this action's audit entry.
    pub seq: u64,
    pub action: PhysicalAction,
    /// Measured intent→actuation window in microseconds.
    pub latency_us: u64,
}

/// The result of a submitted intent, carrying the audit/PDSG fingerprints.
#[derive(Debug, Clone, PartialEq)]
pub struct ExecutionReport {
    pub protocol_id: String,
    pub step_count: usize,
    pub accepted_steps: Vec<AcceptedStep>,
    /// Non-blocking safety-gate alerts that were recorded and continued past.
    pub warnings: Vec<String>,
    /// The audit ledger chain hash after this protocol.
    pub ledger_tail_hash: [u8; 32],
    /// Post-protocol sensor pass samples.
    pub sensor_samples: Vec<SensorSample>,
}

/// Why an intent did not execute.
#[derive(Debug)]
pub enum OrchestrationError {
    Store(StoreError),
    Compile(CompileError),
    /// The safety gate blocked compilation to the HAL.
    SafetyBlocked(GateDecision),
    Lock(LockError),
    Gateway(GatewayError),
    /// The device refused an actuation (fault-injected or real).
    ActuationRejected(ActuationRejection),
}

impl From<StoreError> for OrchestrationError {
    fn from(e: StoreError) -> Self {
        OrchestrationError::Store(e)
    }
}

impl core::fmt::Display for OrchestrationError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            OrchestrationError::Store(e) => write!(f, "store: {e}"),
            OrchestrationError::Compile(e) => write!(f, "compile: {e}"),
            OrchestrationError::SafetyBlocked(GateDecision::Abort { rule_id, reason }) => {
                write!(f, "safety gate blocked ({rule_id}): {reason}")
            }
            OrchestrationError::SafetyBlocked(other) => write!(f, "safety gate: {other:?}"),
            OrchestrationError::Lock(e) => write!(f, "lock: {e}"),
            OrchestrationError::Gateway(e) => write!(f, "gateway: {e}"),
            OrchestrationError::ActuationRejected(r) => write!(f, "actuation rejected: {r}"),
        }
    }
}

/// Owns one rig end-to-end: token authority, capability-gated gateway, audit
/// ledger, PDSG store, safety gate, and the resource lock manager.
#[derive(Debug)]
pub struct Orchestrator<D, S, C> {
    authority: TokenAuthority,
    gateway: HardwareGateway<D, S, C>,
    ledger: AuditLedger,
    store: PdsgStore,
    safety: SafetyGate,
    locks: LockManager,
    compiler: ProtocolCompiler,
    equipment: String,
    equipment_node_id: NodeId,
    token_ttl_ms: u64,
    sensor_channels: [SensorChannel; 4],
}

impl<D, S, C> Orchestrator<D, S, C>
where
    D: HalDevice,
    S: SensorDevice,
    C: Clock,
{
    /// Wires the gateway to the shared capability issuer (so revocations are
    /// live) and registers this rig's actuator node in the PDSG.
    pub fn new(
        equipment: &str,
        device: D,
        sensors: S,
        clock: C,
        safety: SafetyGate,
        token_ttl_ms: u64,
    ) -> Result<Self, OrchestrationError> {
        let authority = TokenAuthority::new(equipment);
        let issuer = authority.issuer();
        let gateway = HardwareGateway::new(device, sensors, clock, issuer, equipment);
        let mut store = PdsgStore::new()?;
        let equipment_node_id = NodeId(equipment.to_string());
        store.put_node(&Node::Actuator(ActuatorNode {
            id: equipment_node_id.clone(),
            equipment: equipment.to_string(),
            state: "idle".into(),
        }))?;
        Ok(Self {
            authority,
            gateway,
            ledger: AuditLedger::new(),
            store,
            safety,
            locks: LockManager::new(),
            compiler: ProtocolCompiler,
            equipment: equipment.to_string(),
            equipment_node_id,
            token_ttl_ms,
            sensor_channels: [
                SensorChannel::PhMilli,
                SensorChannel::TemperatureMilliC,
                SensorChannel::PressureMbar,
                SensorChannel::TeerMilliOhm,
            ],
        })
    }

    /// Submits a digital intent for execution. Returns the execution report
    /// (audit + PDSG fingerprints) or an [`OrchestrationError`].
    pub fn submit_intent(
        &mut self,
        intent: &IntentNode,
        env: &Environment,
        actor: &str,
        now_ms: u64,
    ) -> Result<ExecutionReport, OrchestrationError> {
        // 1. Register the intent in the PDSG as compiling.
        let mut compiling = intent.clone();
        compiling.status = IntentStatus::Compiling;
        self.store.put_node(&Node::Intent(compiling))?;

        // 2. Compile the intent into a safety-evaluable protocol.
        let protocol = self.compiler.compile(intent).map_err(OrchestrationError::Compile)?;

        // 3. Safety gate: physical execution is unreachable on Abort.
        let mut warnings = Vec::new();
        match self.safety.evaluate(&protocol, env) {
            GateDecision::Allow => {}
            GateDecision::Alert { rule_id, reason } => {
                warnings.push(alloc::format!("alert ({rule_id}): {reason}"));
                self.ledger.append(
                    now_ms,
                    actor,
                    PhysicalAction::SetValve {
                        valve: "protocol".into(),
                        open: false,
                    },
                    AuditOutcome::Rejected {
                        reason: alloc::format!("safety alert ({rule_id}): {reason}"),
                    },
                    &self.equipment,
                    None,
                );
            }
            GateDecision::Abort { ref rule_id, ref reason } => {
                self.ledger.append(
                    now_ms,
                    actor,
                    PhysicalAction::SetValve {
                        valve: "protocol".into(),
                        open: false,
                    },
                    AuditOutcome::Rejected {
                        reason: alloc::format!("safety gate blocked ({rule_id}): {reason}"),
                    },
                    &self.equipment,
                    None,
                );
                self.set_intent_status(&intent.id, IntentStatus::Aborted)?;
                return Err(OrchestrationError::SafetyBlocked(GateDecision::Abort {
                    rule_id: rule_id.clone(),
                    reason: reason.clone(),
                }));
            }
        }

        // 4. Causality edge: the safety gate passed, so the pathway may be
        // recorded as verified, deterministic (spec.txt §3).
        self.store.put_edge(&Edge {
            id: EdgeId(alloc::format!("causality-{}", protocol.id)),
            from: intent.id.clone(),
            to: self.equipment_node_id.clone(),
            kind: EdgeKind::Causality,
            verified_by_gate: true,
        })?;

        // 5. Acquire every resource atomically (all-or-nothing rollback).
        let resources = self.resources_for(&protocol);
        self.locks
            .acquire_all(&resources)
            .map_err(OrchestrationError::Lock)?;

        // 6. Execute each step, capability-gated through the gateway.
        let mut accepted_steps = Vec::new();
        for step in &protocol.steps {
            let Some((action, cmd)) = self.compile_step(&step.action) else {
                self.ledger.append(
                    now_ms,
                    actor,
                    self.ledger_action(&step.action),
                    AuditOutcome::Rejected {
                        reason: "step has no capability mapping".into(),
                    },
                    &self.equipment,
                    None,
                );
                self.abort(&intent.id, &resources, now_ms, actor)?;
                return Err(OrchestrationError::Compile(CompileError::UnsupportedStep(
                    step.action.kind().into(),
                )));
            };
            let action_id = ActionId::from_action(&action)
                .ok_or(CompileError::UnsupportedStep(step.action.kind().into()))
                .map_err(OrchestrationError::Compile)?;
            let token = self
                .authority
                .issue(action_id, actor, now_ms, now_ms + self.token_ttl_ms);
            match self.gateway.actuate(&token, cmd, &action, now_ms) {
                Ok((tpt_chrysalis_hal::ActuationOutcome::Ok, timing)) => {
                    let outcome = self.ledger.append(
                        now_ms,
                        actor,
                        action.clone(),
                        AuditOutcome::Accepted,
                        &self.equipment,
                        Some(token.expires_at_ms),
                    );
                    let LedgerOutcome::Appended(seq) = outcome;
                    accepted_steps.push(AcceptedStep {
                        seq,
                        action,
                        latency_us: timing.latency_us(),
                    });
                }
                Ok((tpt_chrysalis_hal::ActuationOutcome::Rejected(rejection), _)) => {
                    self.ledger.append(
                        now_ms,
                        actor,
                        action.clone(),
                        AuditOutcome::Rejected {
                            reason: rejection.to_string(),
                        },
                        &self.equipment,
                        Some(token.expires_at_ms),
                    );
                    self.abort(&intent.id, &resources, now_ms, actor)?;
                    return Err(OrchestrationError::ActuationRejected(rejection));
                }
                Err(e) => {
                    self.ledger.append(
                        now_ms,
                        actor,
                        action.clone(),
                        AuditOutcome::Rejected {
                            reason: e.to_string(),
                        },
                        &self.equipment,
                        Some(token.expires_at_ms),
                    );
                    self.abort(&intent.id, &resources, now_ms, actor)?;
                    return Err(OrchestrationError::Gateway(e));
                }
            }
        }

        // 7. Sensor pass: record observations into the PDSG (Flow edges).
        let mut sensor_samples = Vec::new();
        for channel in self.sensor_channels {
            let action = PhysicalAction::ReadSensor {
                channel: channel.to_string(),
            };
            let token = self
                .authority
                .issue(ActionId::ReadSensor, actor, now_ms, now_ms + self.token_ttl_ms);
            match self.gateway.read_sensor(&token, channel, now_ms) {
                Ok(reading) => {
                    self.ledger.append(
                        now_ms,
                        actor,
                        action.clone(),
                        AuditOutcome::Accepted,
                        &self.equipment,
                        Some(token.expires_at_ms),
                    );
                    let sample = SensorSample {
                        value_scaled: reading.value_scaled,
                        at_ms: reading.at_ms,
                    };
                    let sensor_id = NodeId(alloc::format!("sensor-{channel}"));
                    self.store.put_node(&Node::Sensor(SensorNode {
                        id: sensor_id.clone(),
                        channel: channel.to_string(),
                        latest: Some(sample),
                    }))?;
                    self.store.put_edge(&Edge {
                        id: EdgeId(alloc::format!("flow-{}-{channel}", intent.id.0)),
                        from: self.equipment_node_id.clone(),
                        to: sensor_id,
                        kind: EdgeKind::Flow,
                        verified_by_gate: false,
                    })?;
                    sensor_samples.push(sample);
                }
                Err(e) => {
                    self.ledger.append(
                        now_ms,
                        actor,
                        action.clone(),
                        AuditOutcome::Rejected {
                            reason: e.to_string(),
                        },
                        &self.equipment,
                        Some(token.expires_at_ms),
                    );
                }
            }
        }

        // 8. Complete the intent and release every resource.
        self.set_intent_status(&intent.id, IntentStatus::Completed)?;
        self.locks.release_all(&resources);

        Ok(ExecutionReport {
            protocol_id: protocol.id.clone(),
            step_count: protocol.steps.len(),
            accepted_steps,
            warnings,
            ledger_tail_hash: self.ledger.last_hash(),
            sensor_samples,
        })
    }

    /// The audit ledger (for integrity re-walks / export).
    pub fn ledger(&self) -> &AuditLedger {
        &self.ledger
    }

    /// Mutable access to the audit ledger, for records written outside a
    /// protocol run, such as refused device commands.
    pub fn ledger_mut(&mut self) -> &mut AuditLedger {
        &mut self.ledger
    }

    /// The PDSG store (for subgraph queries / export).
    pub fn store(&mut self) -> &mut PdsgStore {
        &mut self.store
    }

    /// Whether any resource is currently held (should be empty outside an
    /// in-flight submission).
    pub fn held_resource_count(&self) -> usize {
        self.locks.held_count()
    }

    fn resources_for(&self, protocol: &Protocol) -> Vec<ResourceId> {
        let mut out = vec![ResourceId::equipment(&self.equipment)];
        for step in &protocol.steps {
            if let StepAction::Dispense {
                reagent,
                target_well,
                ..
            } = &step.action
            {
                out.push(ResourceId::reagent(reagent));
                out.push(ResourceId::well(target_well));
            }
        }
        out
    }

    /// Maps a step action to its ledger record + HAL command. `None` when the
    /// protocol's names are unknown to the registries.
    fn compile_step(&self, action: &StepAction) -> Option<(PhysicalAction, ActuatorCommand)> {
        match action {
            StepAction::Dispense {
                reagent,
                volume_ul,
                target_well,
            } => Some((
                PhysicalAction::Dispense {
                    reagent: reagent.clone(),
                    volume_ul: *volume_ul,
                    target: target_well.clone(),
                },
                ActuatorCommand::Dispense(DispenseCommand {
                    reagent: ReagentRegistry::id(reagent)?,
                    volume_ul: *volume_ul,
                    target: WellRegistry::id(target_well)?,
                }),
            )),
            StepAction::SetValve { valve, open } => Some((
                PhysicalAction::SetValve {
                    valve: valve.clone(),
                    open: *open,
                },
                ActuatorCommand::SetValve(ValveCommand {
                    valve: ValveRegistry::id(valve)?,
                    open: *open,
                }),
            )),
            StepAction::SetTemperature { setpoint_milli_c } => Some((
                PhysicalAction::SetTemperature {
                    setpoint_milli_c: *setpoint_milli_c,
                },
                ActuatorCommand::SetTemperature(TemperatureCommand {
                    setpoint_milli_c: *setpoint_milli_c,
                }),
            )),
        }
    }

    fn ledger_action(&self, action: &StepAction) -> PhysicalAction {
        match action {
            StepAction::Dispense {
                reagent,
                volume_ul,
                target_well,
            } => PhysicalAction::Dispense {
                reagent: reagent.clone(),
                volume_ul: *volume_ul,
                target: target_well.clone(),
            },
            StepAction::SetValve { valve, open } => PhysicalAction::SetValve {
                valve: valve.clone(),
                open: *open,
            },
            StepAction::SetTemperature { setpoint_milli_c } => {
                PhysicalAction::SetTemperature {
                    setpoint_milli_c: *setpoint_milli_c,
                }
            }
        }
    }

    fn set_intent_status(
        &mut self,
        id: &NodeId,
        status: IntentStatus,
    ) -> Result<(), OrchestrationError> {
        if let Some(Node::Intent(mut node)) = self.store.get_node(id)? {
            node.status = status;
            self.store.put_node(&Node::Intent(node))?;
        }
        Ok(())
    }

    /// Shared failure path: mark the intent aborted and release the locks.
    fn abort(
        &mut self,
        id: &NodeId,
        resources: &[ResourceId],
        _now_ms: u64,
        _actor: &str,
    ) -> Result<(), OrchestrationError> {
        self.set_intent_status(id, IntentStatus::Aborted)?;
        self.locks.release_all(resources);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_chrysalis_core::DEFAULT_SCOPE;
    use tpt_chrysalis_hal::ReagentId;
    use tpt_chrysalis_safety::environment::{EnvValue, Var};
    use tpt_chrysalis_safety::rule::{Condition, FailureAction, Operator, Rule};
    use tpt_chrysalis_sim::fault::{FaultInjector, SimFault};
    use tpt_chrysalis_sim::model::{SimConfig, SimLiquidHandler, TickClock};
    use tpt_chrysalis_sim::sensor::{SimSensorBank, SimSensorConfig};

    use crate::compiler::build_intent;

    fn rig() -> (
        SimLiquidHandler<TickClock>,
        SimSensorBank<TickClock>,
        TickClock,
    ) {
        let clock = TickClock::new();
        let device = SimLiquidHandler::new(SimConfig::default(), clock);
        let state = device.shared_state();
        let sensors = SimSensorBank::new(
            SimSensorConfig {
                noise: 0.0,
                seed: 2,
            },
            state,
            clock,
        );
        (device, sensors, clock)
    }

    fn temp_rule() -> Rule {
        Rule {
            id: "rxn-temp-limit".into(),
            preconditions: vec![
                Condition::new(Var::ReagentA, Operator::Ge, EnvValue::num(0.5)),
                Condition::new(Var::ReagentB, Operator::Ge, EnvValue::num(0.5)),
            ],
            requirement: Condition::new(Var::TemperatureMilliC, Operator::Lt, EnvValue::num(4_000.0)),
            on_fail: FailureAction::Abort,
        }
    }

    /// Both reagents present, so the temperature rule fires.
    fn env() -> Environment {
        Environment::new()
            .set(Var::ReagentA, EnvValue::present())
            .set(Var::ReagentB, EnvValue::present())
            .set(Var::VolumeUl, EnvValue::num(100.0))
            .clone()
    }

    fn orchestrator(
        device: SimLiquidHandler<TickClock>,
        sensors: SimSensorBank<TickClock>,
        clock: TickClock,
    ) -> Orchestrator<SimLiquidHandler<TickClock>, SimSensorBank<TickClock>, TickClock> {
        let gate = SafetyGate::new().add_rule(temp_rule()).clone();
        Orchestrator::new(DEFAULT_SCOPE, device, sensors, clock, gate, 5_000).unwrap()
    }

    #[test]
    fn end_to_end_dispense_round_trip() {
        let (device, sensors, clock) = rig();
        let mut o = orchestrator(device, sensors, clock);
        let intent = build_intent("i1", "A", "w1", 100, Some(2_000));
        let report = o.submit_intent(&intent, &env(), "alice", 1_000).unwrap();
        // Temperature step + dispense step.
        assert_eq!(report.step_count, 2);
        assert_eq!(report.accepted_steps.len(), 2);
        assert!(report.warnings.is_empty());
        assert!(o.ledger().verify_integrity().is_intact());
        assert_eq!(o.held_resource_count(), 0);

        // PDSG: intent completed, causality edge verified, sensors attached.
        let intent_node = o.store().get_node(&intent.id).unwrap().unwrap();
        assert!(matches!(
            intent_node,
            Node::Intent(IntentNode { status: IntentStatus::Completed, .. })
        ));
        let edges = o.store().list_edges().unwrap();
        assert!(edges.iter().any(|e| e.kind == EdgeKind::Causality && e.verified_by_gate));
        assert!(edges.iter().any(|e| e.kind == EdgeKind::Flow));
        assert!(!report.sensor_samples.is_empty());
    }

    #[test]
    fn safety_blocked_intent_is_aborted_and_audited() {
        let (device, sensors, clock) = rig();
        let mut o = orchestrator(device, sensors, clock);
        // 37 °C violates the < 4 °C reaction rule.
        let intent = build_intent("i2", "A", "w1", 100, Some(37_000));
        match o.submit_intent(&intent, &env(), "alice", 1_000) {
            Err(OrchestrationError::SafetyBlocked(GateDecision::Abort {
                rule_id,
                ..
            })) => assert_eq!(rule_id, "rxn-temp-limit"),
            other => panic!("expected SafetyBlocked, got {other:?}"),
        }
        let intent_node = o.store().get_node(&intent.id).unwrap().unwrap();
        assert!(matches!(
            intent_node,
            Node::Intent(IntentNode { status: IntentStatus::Aborted, .. })
        ));
        // The gate rejection was audited.
        let entries = o.ledger().entries();
        assert!(entries
            .iter()
            .any(|e| matches!(e.outcome, AuditOutcome::Rejected { .. })));
        assert_eq!(o.held_resource_count(), 0);
    }

    #[test]
    fn actuation_rejection_aborts_and_audits() {
        let (device, sensors, clock) = rig();
        let mut device = device;
        device.inject(SimFault::PumpBlocked(ReagentId(0)));
        let mut o = orchestrator(device, sensors, clock);
        let intent = build_intent("i3", "A", "w1", 100, Some(2_000));
        match o.submit_intent(&intent, &env(), "bob", 1_000) {
            Err(OrchestrationError::ActuationRejected(rejection)) => {
                assert!(matches!(
                    rejection,
                    ActuationRejection::UnknownReagent(ReagentId(0))
                ));
            }
            other => panic!("expected ActuationRejected, got {other:?}"),
        }
        let entries = o.ledger().entries();
        assert!(entries
            .iter()
            .any(|e| matches!(e.outcome, AuditOutcome::Rejected { .. })));
        assert_eq!(o.held_resource_count(), 0);
    }

    #[test]
    fn locks_block_second_concurrent_use_of_same_reagent() {
        let (device, sensors, clock) = rig();
        let mut o = orchestrator(device, sensors, clock);
        // Acquire the reagent lock by hand to simulate an in-flight protocol.
        let intent = build_intent("i4", "A", "w1", 100, Some(2_000));
        let protocol = ProtocolCompiler.compile(&intent).unwrap();
        let resources = o.resources_for(&protocol);
        o.locks.acquire_all(&resources).unwrap();
        // A second intent touching reagent A must be refused before any
        // actuation.
        let second = build_intent("i5", "A", "w2", 50, Some(2_000));
        match o.submit_intent(&second, &env(), "carol", 1_000) {
            Err(OrchestrationError::Lock(LockError::AlreadyHeld)) => {}
            other => panic!("expected Lock(AlreadyHeld), got {other:?}"),
        }
        o.locks.release_all(&resources);
        assert_eq!(o.held_resource_count(), 0);
    }

    #[test]
    fn sensor_dropout_is_audited_not_fatal() {
        let (device, sensors, clock) = rig();
        let mut sensors = sensors;
        sensors.inject(SimFault::SensorDropout(SensorChannel::PhMilli));
        let mut o = orchestrator(device, sensors, clock);
        let intent = build_intent("i6", "A", "w1", 100, Some(2_000));
        let report = o.submit_intent(&intent, &env(), "dave", 1_000).unwrap();
        // 3 of 4 channels recorded (the dropped one is audited as rejected).
        assert_eq!(report.sensor_samples.len(), 3);
        let entries = o.ledger().entries();
        assert!(entries.iter().any(|e| {
            matches!(e.outcome, AuditOutcome::Rejected { .. })
                && matches!(e.action, PhysicalAction::ReadSensor { .. })
        }));
    }
}
