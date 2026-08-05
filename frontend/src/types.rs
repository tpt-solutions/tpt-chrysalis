//! Shared types matching the JSON schemas in `schemas/`

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentNode {
    pub node_id: String,
    pub kind: String, // "Intent"
    pub intent_type: IntentType,
    pub parameters: HashMap<String, serde_json::Value>,
    pub status: IntentStatus,
    pub created_at_ms: u64,
    pub created_by: String,
    pub parent_intent_id: Option<String>,
    pub metadata: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum IntentType {
    DoseReagent,
    SetTemperature,
    Incubate,
    Wash,
    ReadSensor,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum IntentStatus {
    Pending,
    Compiling,
    SafetyCheck,
    Executing,
    Completed,
    Failed,
    Aborted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActuatorNode {
    pub node_id: String,
    pub kind: String, // "Actuator"
    pub actuator_type: ActuatorType,
    pub resource_id: String,
    pub capabilities: Vec<ActuatorCapability>,
    pub status: ActuatorStatus,
    pub location: Location3D,
    pub limits: HashMap<String, serde_json::Value>,
    pub current_state: HashMap<String, serde_json::Value>,
    pub metadata: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum ActuatorType {
    Valve,
    Pump,
    Heater,
    Cooler,
    Stirrer,
    Dispenser,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum ActuatorCapability {
    Open,
    Close,
    SetFlowRate,
    SetTemperature,
    Dispense,
    Stir,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum ActuatorStatus {
    Idle,
    Busy,
    Error,
    Maintenance,
    Offline,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Location3D {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensorNode {
    pub node_id: String,
    pub kind: String, // "Sensor"
    pub sensor_type: SensorType,
    pub channel: String,
    pub units: String,
    pub sample_rate_hz: f64,
    pub status: SensorStatus,
    pub location: Location3D,
    pub calibration: HashMap<String, serde_json::Value>,
    pub noise_model: HashMap<String, serde_json::Value>,
    pub current_reading: HashMap<String, serde_json::Value>,
    pub metadata: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum SensorType {
    Temperature,
    PH,
    TEER,
    Pressure,
    FlowRate,
    Optical,
    Imaging,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum SensorStatus {
    Active,
    Degraded,
    Offline,
    Calibrating,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub edge_id: String,
    pub kind: String, // "Edge"
    pub source_node_id: String,
    pub target_node_id: String,
    pub edge_kind: EdgeKind,
    pub created_at_ms: u64,
    pub safety_gate_passed: bool,
    pub safety_check_id: Option<String>,
    pub capability_token_id: Option<String>,
    pub parameters: HashMap<String, serde_json::Value>,
    pub metadata: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum EdgeKind {
    IntentToActuator,
    ActuatorToSensor,
    SensorToIntent,
    ActuatorToActuator,
    IntentToIntent,
    DataFlow,
    ControlFlow,
    Causality,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PdsgSnapshot {
    pub snapshot_id: String,
    pub timestamp_ms: u64,
    pub nodes: Vec<PdsgNode>,
    pub edges: Vec<Edge>,
    pub subgraph_root: String,
    pub metadata: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum PdsgNode {
    Intent(IntentNode),
    Actuator(ActuatorNode),
    Sensor(SensorNode),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityToken {
    pub token_id: String,
    pub action_id: String,
    pub scope: String,
    pub subject: String,
    pub issued_at_ms: u64,
    pub expires_at_ms: u64,
    pub signature: String,
    pub nonce: Option<String>,
    pub constraints: HashMap<String, serde_json::Value>,
    pub metadata: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlcoaReport {
    pub report_id: String,
    pub generated_at_ms: u64,
    pub actor_filter: Option<String>,
    pub ledger_entry_count: u64,
    pub reported_entry_count: u64,
    pub last_hash_hex: String,
    pub integrity: LedgerIntegrity,
    pub criteria: Vec<AlcoaCriterion>,
    pub history: Vec<ReportedEntry>,
    pub core_compliant: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerIntegrity {
    pub is_intact: bool,
    pub entry_count: u64,
    pub broken_seqs: Vec<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlcoaCriterion {
    pub code: String,
    pub name: String,
    pub description: String,
    pub satisfied: bool,
    pub evidence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportedEntry {
    pub seq: u64,
    pub timestamp_ms: u64,
    pub actor: String,
    pub action: String,
    pub outcome: String,
    pub scope: String,
    pub token_expires_at_ms: Option<u64>,
    pub prev_hash_hex: String,
    pub hash_hex: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmitIntentRequest {
    pub intent: IntentNode,
    pub capability_token: CapabilityToken,
    pub priority: u8,
    pub metadata: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensorReading {
    pub sensor_id: String,
    pub channel: String,
    pub value: f64,
    pub units: String,
    pub timestamp_ms: u64,
    pub quality: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
    pub details: Option<HashMap<String, serde_json::Value>>,
}

impl From<gloo_net::Error> for ApiError {
    fn from(e: gloo_net::Error) -> Self {
        ApiError {
            code: "NETWORK_ERROR".into(),
            message: e.to_string(),
            details: None,
        }
    }
}
