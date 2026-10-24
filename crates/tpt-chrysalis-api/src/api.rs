//! The API service core: an [`ApiState`] that owns a Phase 1 rig
//! (simulated backend) and exposes token-issuance, PDSG snapshot, protocol
//! submission, and compliance-report operations. All handlers are pure
//! functions of their inputs so they are testable without a socket; the HTTP
//! binding lives in [`crate::http`].

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use serde::{Deserialize, Serialize};
use tpt_chrysalis_compliance::build_report;
use tpt_chrysalis_compliance::AlcoaReport;
use tpt_chrysalis_core::{ActionId, Edge, Node, PhysicalAction, SensorSample, StoreError};
use tpt_chrysalis_orchestrator::compiler::build_intent;
use tpt_chrysalis_orchestrator::{AcceptedStep, ExecutionReport, OrchestrationError, Orchestrator};
use tpt_chrysalis_safety::environment::{EnvValue, Var};
use tpt_chrysalis_safety::gate::SafetyGate;
use tpt_chrysalis_safety::rule::{Condition, FailureAction, Operator, Rule};
use tpt_chrysalis_safety::Environment;
use tpt_chrysalis_sim::model::{SimConfig, SimLiquidHandler, TickClock};
use tpt_chrysalis_sim::sensor::{SimSensorBank, SimSensorConfig};
use tpt_chrysalis_organoid::{DoseOutcome, FlowPlan, OrganLinkGraph, PdoScreen};
use tpt_chrysalis_vision::{SyntheticMicroscopyImage, VisionFeatures};
use crate::domain::{OrganoidDomain, VisionDomain};

use crate::tokens::{ApiAuthError, ApiToken, ApiTokenManager};

/// The concrete Phase 1 rig: simulated liquid handler + sensor bank.
type SimOrchestrator =
    Orchestrator<SimLiquidHandler<TickClock>, SimSensorBank<TickClock>, TickClock>;

/// The safety gate the API ships with: both reagents present ⇒ the reaction
/// must stay below 4°C, else the protocol is aborted.
pub fn default_gate() -> SafetyGate {
    SafetyGate::new()
        .add_rule(Rule {
            id: "rxn-temp-limit".into(),
            preconditions: vec![
                Condition::new(Var::ReagentA, Operator::Ge, EnvValue::num(0.5)),
                Condition::new(Var::ReagentB, Operator::Ge, EnvValue::num(0.5)),
            ],
            requirement: Condition::new(
                Var::TemperatureMilliC,
                Operator::Lt,
                EnvValue::num(4_000.0),
            ),
            on_fail: FailureAction::Abort,
        })
        .clone()
}

/// Device command kinds the API recognises. Recognised is not allowed. See
/// [`APPROVED_DEVICE_COMMANDS`].
pub const DEVICE_COMMAND_KINDS: &[&str] = &["heal_pathway", "sprout_conduit", "reinforce_tendon"];

/// Device command kinds that may execute. This list is empty.
///
/// The safety gate allows any step that no rule matches (`gate.rs`, the
/// `Allow` at the end of `evaluate`). So a command kind must not run until it
/// has its own gate rule and its own HAL mapping. Add a kind here only together
/// with both, after safety review.
pub const APPROVED_DEVICE_COMMANDS: &[&str] = &[];

/// `POST /api/v1/device-commands` body. Parameters use the device's own units.
#[derive(Debug, Clone, Deserialize)]
pub struct DeviceCommandRequest {
    pub id: String,
    pub device: String,
    pub kind: String,
    #[serde(default)]
    pub parameters: BTreeMap<String, f64>,
    #[serde(default)]
    pub environment: BTreeMap<String, f64>,
}

/// Why an API operation failed.
#[derive(Debug)]
pub enum ApiError {
    /// State construction failed (e.g. the PDSG store could not be created).
    Init(String),
    /// The request body or intent was malformed / not compilable.
    BadRequest(String),
    /// The safety gate blocked the protocol.
    SafetyBlocked(String),
    /// A resource lock could not be acquired.
    Conflict(String),
    /// Anything downstream that should not happen.
    Internal(String),
}

impl ApiError {
    pub fn status_code(&self) -> u16 {
        match self {
            ApiError::BadRequest(_) => 400,
            ApiError::SafetyBlocked(_) => 422,
            ApiError::Conflict(_) => 409,
            ApiError::Init(_) | ApiError::Internal(_) => 500,
        }
    }

    pub fn message(&self) -> String {
        match self {
            ApiError::Init(m)
            | ApiError::BadRequest(m)
            | ApiError::SafetyBlocked(m)
            | ApiError::Conflict(m)
            | ApiError::Internal(m) => m.clone(),
        }
    }
}

impl From<OrchestrationError> for ApiError {
    fn from(e: OrchestrationError) -> Self {
        match e {
            OrchestrationError::Store(s) => ApiError::Internal(format!("store: {s}")),
            OrchestrationError::Compile(c) => ApiError::BadRequest(format!("compile: {c}")),
            OrchestrationError::SafetyBlocked(d) => ApiError::SafetyBlocked(format!("{d:?}")),
            OrchestrationError::Lock(l) => ApiError::Conflict(format!("lock: {l}")),
            OrchestrationError::Gateway(g) => ApiError::Internal(format!("gateway: {g}")),
            OrchestrationError::ActuationRejected(r) => {
                ApiError::Internal(format!("actuation rejected: {r}"))
            }
        }
    }
}

impl From<StoreError> for ApiError {
    fn from(e: StoreError) -> Self {
        ApiError::Internal(format!("store: {e}"))
    }
}

/// `POST /api/v1/tokens` body.
#[derive(Debug, Clone, Deserialize)]
pub struct IssueTokenRequest {
    pub actor: String,
    pub action: ActionId,
    #[serde(default = "default_ttl_ms")]
    pub ttl_ms: u64,
}

fn default_ttl_ms() -> u64 {
    60_000
}

/// `POST /api/v1/intents` body. `environment` carries the gate variables
/// (e.g. `reagent_a`/`reagent_b` presence, `pressure_mbar`); a missing
/// variable fails the gate closed.
#[derive(Debug, Clone, Deserialize)]
pub struct SubmitIntentRequest {
    pub id: String,
    pub reagent: String,
    pub target_well: String,
    pub volume_ul: u32,
    #[serde(default)]
    pub temperature_milli_c: Option<i64>,
    #[serde(default)]
    pub environment: BTreeMap<String, f64>,
}

/// One accepted actuation, rendered for the wire.
#[derive(Debug, Clone, Serialize)]
pub struct ApiAcceptedStep {
    pub seq: u64,
    pub action: PhysicalAction,
    pub latency_us: u64,
}

impl From<&AcceptedStep> for ApiAcceptedStep {
    fn from(s: &AcceptedStep) -> Self {
        Self {
            seq: s.seq,
            action: s.action.clone(),
            latency_us: s.latency_us,
        }
    }
}

/// The result of a submitted intent, rendered for the wire.
#[derive(Debug, Clone, Serialize)]
pub struct ApiExecution {
    pub protocol_id: String,
    pub step_count: usize,
    pub accepted_steps: Vec<ApiAcceptedStep>,
    pub warnings: Vec<String>,
    pub ledger_tail_hash: String,
    pub sensor_samples: Vec<SensorSample>,
}

impl From<ExecutionReport> for ApiExecution {
    fn from(r: ExecutionReport) -> Self {
        Self {
            protocol_id: r.protocol_id,
            step_count: r.step_count,
            accepted_steps: r.accepted_steps.iter().map(ApiAcceptedStep::from).collect(),
            warnings: r.warnings,
            ledger_tail_hash: hex(&r.ledger_tail_hash),
            sensor_samples: r.sensor_samples,
        }
    }
}

/// A point-in-time view of the PDSG.
#[derive(Debug, Clone, Serialize)]
pub struct PdsgSnapshot {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

/// `POST /api/v1/organoid/verify` body: a proposed cross-chip flow plan plus
/// the reagents currently active on the path.
#[derive(Debug, Clone, Deserialize)]
pub struct OrganoidVerifyRequest {
    pub plan: FlowPlan,
    #[serde(default)]
    pub active_reagents: Vec<String>,
}

/// `POST /api/v1/organoid/verify` response.
#[derive(Debug, Clone, Serialize)]
pub struct OrganoidVerifyResponse {
    pub allowed: bool,
    pub errors: Vec<String>,
}

/// `POST /api/v1/organoid/screen` body: a Patient-Derived Organoid screen.
#[derive(Debug, Clone, Deserialize)]
pub struct PdoScreenRequest {
    pub screen: PdoScreen,
}

/// `POST /api/v1/organoid/screen` response.
#[derive(Debug, Clone, Serialize)]
pub struct PdoScreenResponse {
    pub outcomes: Vec<DoseOutcome>,
}

/// `POST /api/v1/vision/capture` body: parameters to synthesize a microscopy
/// frame and the object-store key to write it under.
#[derive(Debug, Clone, Deserialize)]
pub struct VisionCaptureRequest {
    pub seed: u64,
    pub width: u32,
    pub height: u32,
    pub density: f64,
    pub background: u8,
    pub key: String,
}

/// `POST /api/v1/vision/capture` response.
#[derive(Debug, Clone, Serialize)]
pub struct VisionCaptureResponse {
    pub features: VisionFeatures,
    pub sha256_hex: String,
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use core::fmt::Write as _;
        let _ = core::write!(out, "{byte:02x}");
    }
    out
}

fn var_from_name(name: &str) -> Option<Var> {
    Some(match name {
        "temperature_milli_c" => Var::TemperatureMilliC,
        "pressure_mbar" => Var::PressureMbar,
        "ph_milli" => Var::PhMilli,
        "volume_ul" => Var::VolumeUl,
        "reagent_a" => Var::ReagentA,
        "reagent_b" => Var::ReagentB,
        "reagent_c" => Var::ReagentC,
        _ => return None,
    })
}

fn build_environment(req: &SubmitIntentRequest) -> Environment {
    let mut env = Environment::new();
    for (name, value) in &req.environment {
        if let Some(var) = var_from_name(name) {
            env.set(var, EnvValue::num(*value));
        }
    }
    // The actuated setpoint is also the environmental temperature the gate
    // evaluates; the explicit field wins over any map entry.
    if let Some(t) = req.temperature_milli_c {
        env.set(Var::TemperatureMilliC, EnvValue::num(t as f64));
    }
    env
}

/// Owns one rig plus its bearer-credential registry. Not cloneable — it is a
/// service boundary, not a value.
#[derive(Debug)]
pub struct ApiState {
    orchestrator: SimOrchestrator,
    tokens: ApiTokenManager,
    /// Phase 2 multi-organ linking domain (scoped to `ORGANOID_SCOPE`).
    organoid: OrganoidDomain,
    /// Phase 2 high-content imaging domain (scoped to `VISION_SCOPE`).
    vision: VisionDomain,
}

impl ApiState {
    /// Builds a fresh rig with the [`default_gate`] safety policy.
    pub fn new(scope: &str) -> Result<Self, ApiError> {
        Self::with_gate(scope, default_gate())
    }

    /// Builds a fresh rig with a caller-supplied safety policy.
    pub fn with_gate(scope: &str, gate: SafetyGate) -> Result<Self, ApiError> {
        let clock = TickClock::new();
        let device = SimLiquidHandler::new(SimConfig::default(), clock);
        let sim_state = device.shared_state();
        let sensors = SimSensorBank::new(
            SimSensorConfig { noise: 0.0, seed: 2 },
            sim_state,
            clock,
        );
        let orchestrator = Orchestrator::new(scope, device, sensors, clock, gate, 5_000)
            .map_err(|e| ApiError::Init(e.to_string()))?;
        Ok(Self {
            orchestrator,
            tokens: ApiTokenManager::new(scope),
            organoid: OrganoidDomain::new(OrganLinkGraph::new()),
            vision: VisionDomain::new(),
        })
    }

    pub fn scope(&self) -> &str {
        self.tokens.scope()
    }

    // -- bearer credentials -------------------------------------------------

    pub fn issue_token(
        &mut self,
        actor: &str,
        action: ActionId,
        ttl_ms: u64,
        now_ms: u64,
    ) -> ApiToken {
        self.tokens.mint(actor, action, now_ms, ttl_ms)
    }

    pub fn authenticate(
        &self,
        bearer: &str,
        action: ActionId,
        now_ms: u64,
    ) -> Result<&str, ApiAuthError> {
        self.tokens
            .authenticate(bearer, action, now_ms)
            .map(|t| t.actor.as_str())
    }

    pub fn revoke(&mut self, bearer: &str) -> bool {
        self.tokens.revoke(bearer)
    }

    pub fn live_tokens(&self) -> usize {
        self.tokens.live_count()
    }

    // -- state queries ------------------------------------------------------

    pub fn pdsg_snapshot(&mut self) -> Result<PdsgSnapshot, ApiError> {
        let store = self.orchestrator.store();
        let nodes = store.list_nodes().map_err(ApiError::from)?;
        let edges = store.list_edges().map_err(ApiError::from)?;
        Ok(PdsgSnapshot { nodes, edges })
    }

    /// The ALCOA+ compliance report over the orchestrator's ledger.
    pub fn compliance_report(&self, actor_filter: Option<&str>, now_ms: u64) -> AlcoaReport {
        build_report(self.orchestrator.ledger(), now_ms, actor_filter)
    }

    // -- command submission ---------------------------------------------------

    pub fn submit_intent(
        &mut self,
        req: &SubmitIntentRequest,
        actor: &str,
        now_ms: u64,
    ) -> Result<ApiExecution, ApiError> {
        let intent = build_intent(
            &req.id,
            &req.reagent,
            &req.target_well,
            req.volume_ul,
            req.temperature_milli_c,
        );
        let env = build_environment(req);
        match self.orchestrator.submit_intent(&intent, &env, actor, now_ms) {
            Ok(report) => {
                tracing::info!(
                    intent = req.id.as_str(),
                    actor,
                    steps = report.step_count,
                    accepted = report.accepted_steps.len(),
                    "protocol executed through safety gate + audit ledger"
                );
                Ok(ApiExecution::from(report))
            }
            Err(e) => {
                tracing::warn!(
                    intent = req.id.as_str(),
                    actor,
                    error = %e,
                    "protocol rejected at safety gate"
                );
                Err(e.into())
            }
        }
    }

    /// Checks a device command, and refuses it unless its kind is approved.
    ///
    /// A refused command of a recognised kind is written to the audit ledger
    /// before the error is returned. Nothing is passed to the orchestrator. A
    /// kind that is approved but not yet mapped to the HAL is also refused.
    /// Because `APPROVED_DEVICE_COMMANDS` is empty, this never succeeds yet, so
    /// the result is `Infallible`. Unknown kinds are not recorded, because their
    /// strings come straight from the request.
    pub fn submit_device_command(
        &mut self,
        req: &DeviceCommandRequest,
        actor: &str,
        now_ms: u64,
    ) -> Result<core::convert::Infallible, ApiError> {
        if req.id.is_empty() || req.device.is_empty() {
            return Err(ApiError::BadRequest("id and device are required".into()));
        }
        let kind = req.kind.as_str();
        if !DEVICE_COMMAND_KINDS.contains(&kind) {
            return Err(ApiError::BadRequest(format!("unknown device command kind `{kind}`")));
        }
        if !APPROVED_DEVICE_COMMANDS.contains(&kind) {
            let reason = "no approved safety rule";
            let scope = self.scope().to_string();
            self.orchestrator.ledger_mut().append(
                now_ms,
                actor,
                PhysicalAction::DeviceCommand {
                    kind: kind.to_string(),
                    device: req.device.clone(),
                },
                tpt_chrysalis_core::ledger::AuditOutcome::Rejected {
                    reason: reason.to_string(),
                },
                &scope,
                None,
            );
            tracing::warn!(command = req.id.as_str(), kind, actor, "device command refused");
            return Err(ApiError::SafetyBlocked(format!(
                "no approved safety rule for `{kind}`; command not executed"
            )));
        }
        Err(ApiError::BadRequest(format!(
            "`{kind}` is approved but not yet mapped to the HAL; command not executed"
        )))
    }

    /// Number of entries in the audit ledger.
    pub fn ledger_len(&self) -> usize {
        self.orchestrator.ledger().len()
    }

    /// The PDSG subgraph rooted at a given node (e.g. an intent id).
    pub fn subgraph(&mut self, root: &str) -> Result<tpt_chrysalis_core::Subgraph, ApiError> {
        let store = self.orchestrator.store();
        store
            .subgraph(&tpt_chrysalis_core::NodeId(root.to_string()))
            .map_err(ApiError::from)
    }

    // -- Phase 2: organoid (multi-organ linking) ---------------------------

    /// Verifies a proposed cross-chip flow plan against the organoid graph's
    /// structural limits and biological-contamination rule. Pure; the caller
    /// must already hold a `LinkChip` capability token.
    pub fn verify_cross_chip(
        &self,
        req: &OrganoidVerifyRequest,
    ) -> OrganoidVerifyResponse {
        let verdict = self
            .organoid
            .verify_cross_chip(&req.plan, &req.active_reagents);
        OrganoidVerifyResponse {
            allowed: verdict.allowed,
            errors: verdict.errors(),
        }
    }

    /// Runs a Patient-Derived Organoid screening sweep. The caller must hold a
    /// `ScreenPdo` capability token.
    pub fn screen_pdo(&self, req: &PdoScreenRequest) -> PdoScreenResponse {
        PdoScreenResponse {
            outcomes: self.organoid.screen(&req.screen),
        }
    }

    // -- Phase 2: vision (high-content imaging) ----------------------------

    /// Synthesizes a microscopy frame from the request parameters, extracts its
    /// CV features, and writes the raw image to object storage with
    /// checksum-on-write. The caller must hold a `CaptureImage` token.
    pub fn capture_vision(
        &self,
        req: &VisionCaptureRequest,
    ) -> Result<VisionCaptureResponse, ApiError> {
        let img =
            SyntheticMicroscopyImage::synthetic(req.seed, req.width, req.height, req.density, req.background);
        let (features, stored) = self
            .vision
            .capture_and_store(&img, &req.key)
            .map_err(|e| ApiError::Internal(format!("vision store: {e}")))?;
        Ok(VisionCaptureResponse {
            features,
            sha256_hex: stored.sha256_hex,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_chrysalis_core::DEFAULT_SCOPE;

    fn state() -> ApiState {
        ApiState::new(DEFAULT_SCOPE).expect("state builds")
    }

    fn now() -> u64 {
        1_000
    }

    fn submit_req() -> SubmitIntentRequest {
        SubmitIntentRequest {
            id: "i1".into(),
            reagent: "A".into(),
            target_well: "w1".into(),
            volume_ul: 100,
            temperature_milli_c: Some(2_000),
            environment: BTreeMap::from([
                ("reagent_a".into(), 1.0),
                ("reagent_b".into(), 1.0),
            ]),
        }
    }

    #[test]
    fn issue_and_submit_round_trip() {
        let mut s = state();
        let tok = s.issue_token("alice", ActionId::SubmitProtocol, 5_000, now());
        assert_eq!(s.authenticate(&tok.token, ActionId::SubmitProtocol, now()), Ok("alice"));
        let exec = s.submit_intent(&submit_req(), "alice", now()).expect("submits");
        assert_eq!(exec.step_count, 2);
        assert_eq!(exec.accepted_steps.len(), 2);
        assert_eq!(exec.ledger_tail_hash.len(), 64);
        assert!(s.pdsg_snapshot().expect("snapshot").nodes.len() >= 3);
    }

    #[test]
    fn safety_blocked_intent_maps_to_422() {
        let mut s = state();
        let mut req = submit_req();
        req.temperature_milli_c = Some(37_000); // 37°C aborts the gate
        let err = s.submit_intent(&req, "bob", now()).expect_err("blocked");
        assert!(matches!(err, ApiError::SafetyBlocked(_)));
        assert_eq!(err.status_code(), 422);
    }

    #[test]
    fn compliance_report_over_api_rig() {
        let mut s = state();
        let tok = s.issue_token("alice", ActionId::SubmitProtocol, 5_000, now());
        let _ = s.authenticate(&tok.token, ActionId::SubmitProtocol, now());
        s.submit_intent(&submit_req(), "alice", now()).expect("submits");
        let report = s.compliance_report(None, now());
        assert!(report.core_compliant);
        assert_eq!(report.reported_entry_count, 6);
    }

    #[test]
    fn bearer_wrong_action_fails() {
        let mut s = state();
        let tok = s.issue_token("alice", ActionId::ReadState, 5_000, now());
        assert_eq!(
            s.authenticate(&tok.token, ActionId::SubmitProtocol, now()),
            Err(ApiAuthError::WrongAction)
        );
    }
}
