//! The Physical-Digital State Graph (PDSG) — spec.txt §3.
//!
//! Node types: [`IntentNode`] (desired experimental parameters),
//! [`ActuatorNode`] (the hardware component), [`SensorNode`] (observation
//! stream). Edges are causality/flow links whose "verified, deterministic
//! pathways" requirement is enforced at write time: a causality edge
//! (intent → actuation) cannot exist without a passed safety-gate check.
//!
//! Storage/query is backed by `tpt-archon-relational`'s SQL engine
//! (ADR 0001): nodes and edges are rows in `pdsg_nodes` / `pdsg_edges`, with
//! the typed payload kept as canonical JSON so the model can evolve without
//! schema churn (additive-only versioning policy, ADR 0007).

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use tpt_archon_relational::database::Database;
use tpt_archon_relational::executor::Value;
use tpt_archon_relational::parser::parse_statement;

/// A stable node identifier.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct NodeId(pub String);

impl core::fmt::Display for NodeId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A stable edge identifier.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct EdgeId(pub String);

impl core::fmt::Display for EdgeId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Lifecycle of a digital intent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IntentStatus {
    Pending,
    Compiling,
    Executing,
    Completed,
    Aborted,
}

/// spec.txt §3 "Digital Intent Nodes".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IntentNode {
    pub id: NodeId,
    /// Free-text or structured description ("Expose Hepatocyte Cluster A to
    /// 10µM Compound X for 4 hours").
    pub intent: String,
    /// The reagent identifier, when the intent drives a dispense.
    pub reagent: Option<String>,
    /// The target well, when the intent drives a dispense.
    pub target_well: Option<String>,
    /// Structured numeric parameters (volume µl, temperature milli-°C, …).
    pub parameters: BTreeMap<String, f64>,
    pub status: IntentStatus,
}

/// spec.txt §3 "Physical Actuator Nodes".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActuatorNode {
    pub id: NodeId,
    /// Equipment identifier, e.g. "sim-liquid-handler-0".
    pub equipment: String,
    /// Human-readable current state, e.g. "idle" or "dispensed 100µl of A".
    pub state: String,
}

/// One observation (scaled integer units — no float drift in the ledger).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SensorSample {
    pub value_scaled: i64,
    pub at_ms: u64,
}

/// spec.txt §3 "Sensor/Observer Nodes".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SensorNode {
    pub id: NodeId,
    /// e.g. "temperature_milli_c".
    pub channel: String,
    pub latest: Option<SensorSample>,
}

/// The three node kinds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Node {
    Intent(IntentNode),
    Actuator(ActuatorNode),
    Sensor(SensorNode),
}

impl Node {
    pub fn id(&self) -> &NodeId {
        match self {
            Node::Intent(n) => &n.id,
            Node::Actuator(n) => &n.id,
            Node::Sensor(n) => &n.id,
        }
    }

    /// The storage `kind` tag.
    pub fn kind_tag(&self) -> &'static str {
        match self {
            Node::Intent(_) => "intent",
            Node::Actuator(_) => "actuator",
            Node::Sensor(_) => "sensor",
        }
    }
}

/// Edge kinds. `Causality` links intent → actuation; `Flow` links actuation →
/// observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EdgeKind {
    /// intent → actuator plan (requires a passed safety-gate check).
    Causality,
    /// actuator → sensor observation stream.
    Flow,
    /// Marker that the safety gate approved a pathway.
    SafetyVerified,
}

impl EdgeKind {
    pub fn as_tag(&self) -> &'static str {
        match self {
            EdgeKind::Causality => "causality",
            EdgeKind::Flow => "flow",
            EdgeKind::SafetyVerified => "safety_verified",
        }
    }
}

/// spec.txt §3 "Edges (Causality & Flow)".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Edge {
    pub id: EdgeId,
    pub from: NodeId,
    pub to: NodeId,
    pub kind: EdgeKind,
    /// True iff a safety-gate check passed for this pathway. Required for
    /// `Causality` edges (see [`PdsgStore::put_edge`]).
    pub verified_by_gate: bool,
}

/// Errors from PDSG storage.
#[derive(Debug, Error)]
pub enum StoreError {
    #[error("SQL error: {0}")]
    Sql(String),
    #[error("node {0} not found")]
    NodeNotFound(NodeId),
    #[error("node payload is not valid {1}: {0}")]
    InvalidPayload(String, &'static str),
    #[error("causality edge {0} requires a passed safety-gate check (verified_by_gate = true)")]
    UnverifiedEdge(EdgeId),
    #[error("edge references unknown node {0}")]
    UnknownEndpoint(NodeId),
    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),
}

/// A subgraph rooted at a node (nodes + edges reachable along the edge
/// direction).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Subgraph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

fn sql_str(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// The relational-backed PDSG store.
#[derive(Debug)]
pub struct PdsgStore {
    db: Database,
}

impl Default for PdsgStore {
    fn default() -> Self {
        Self::new().expect("PDSG schema initializes")
    }
}

impl PdsgStore {
    /// Creates an empty store with the `pdsg_nodes`/`pdsg_edges` schema.
    pub fn new() -> Result<Self, StoreError> {
        let mut db = Database::empty();
        let ddl = [
            "CREATE TABLE pdsg_nodes (node_id TEXT, kind TEXT, payload TEXT)",
            "CREATE TABLE pdsg_edges (edge_id TEXT, from_id TEXT, to_id TEXT, kind TEXT, verified INTEGER, payload TEXT)",
        ];
        for sql in ddl {
            let stmt = parse_statement(sql).map_err(|e| StoreError::Sql(e.0))?;
            db.execute(&stmt, &[])
                .map_err(|e| StoreError::Sql(e.to_string()))?;
        }
        Ok(Self { db })
    }

    fn exec(&mut self, sql: &str) -> Result<(), StoreError> {
        let stmt = parse_statement(sql).map_err(|e| StoreError::Sql(e.0))?;
        self.db
            .execute(&stmt, &[])
            .map(|_| ())
            .map_err(|e| StoreError::Sql(e.to_string()))
    }

    fn query_rows(&mut self, sql: &str) -> Result<Vec<Vec<Value>>, StoreError> {
        let stmt = parse_statement(sql).map_err(|e| StoreError::Sql(e.0))?;
        let rs = self
            .db
            .execute(&stmt, &[])
            .map_err(|e| StoreError::Sql(e.to_string()))?;
        Ok(rs.rows)
    }

    /// Upserts a node (insert or update by id).
    pub fn put_node(&mut self, node: &Node) -> Result<(), StoreError> {
        let id = sql_str(&node.id().0);
        let kind = node.kind_tag();
        let payload = serde_json::to_string(node)?;
        let exists = !self
            .query_rows(&format!("SELECT payload FROM pdsg_nodes WHERE node_id = {id}"))?
            .is_empty();
        if exists {
            self.exec(&format!(
                "UPDATE pdsg_nodes SET kind = {k}, payload = {p} WHERE node_id = {id}",
                k = sql_str(kind),
                p = sql_str(&payload)
            ))
        } else {
            self.exec(&format!(
                "INSERT INTO pdsg_nodes (node_id, kind, payload) VALUES ({id}, {k}, {p})",
                k = sql_str(kind),
                p = sql_str(&payload)
            ))
        }
    }

    pub fn get_node(&mut self, id: &NodeId) -> Result<Option<Node>, StoreError> {
        let id = sql_str(&id.0);
        let rows = self
            .query_rows(&format!("SELECT payload FROM pdsg_nodes WHERE node_id = {id}"))?;
        let Some(row) = rows.into_iter().next() else {
            return Ok(None);
        };
        let payload = match &row[0] {
            Value::Text(s) => s.clone(),
            other => return Err(StoreError::InvalidPayload(format!("{other:?}"), "text")),
        };
        serde_json::from_str(&payload)
            .map(Some)
            .map_err(|e| StoreError::InvalidPayload(e.to_string(), "node"))
    }

    pub fn list_nodes(&mut self) -> Result<Vec<Node>, StoreError> {
        let rows = self.query_rows("SELECT payload FROM pdsg_nodes")?;
        let mut out = Vec::new();
        for row in rows {
            let payload = match &row[0] {
                Value::Text(s) => s.clone(),
                other => return Err(StoreError::InvalidPayload(format!("{other:?}"), "text")),
            };
            out.push(
                serde_json::from_str(&payload)
                    .map_err(|e| StoreError::InvalidPayload(e.to_string(), "node"))?,
            );
        }
        Ok(out)
    }

    pub fn list_nodes_by_kind(&mut self, kind: &str) -> Result<Vec<Node>, StoreError> {
        let k = sql_str(kind);
        let rows = self.query_rows(&format!(
            "SELECT payload FROM pdsg_nodes WHERE kind = {k}"
        ))?;
        let mut out = Vec::new();
        for row in rows {
            let payload = match &row[0] {
                Value::Text(s) => s.clone(),
                other => return Err(StoreError::InvalidPayload(format!("{other:?}"), "text")),
            };
            out.push(
                serde_json::from_str(&payload)
                    .map_err(|e| StoreError::InvalidPayload(e.to_string(), "node"))?,
            );
        }
        Ok(out)
    }

    /// Inserts an edge. Enforces the "verified, deterministic pathways"
    /// requirement (spec.txt §3): a `Causality` edge may only be written with
    /// `verified_by_gate = true`, and both endpoints must exist.
    pub fn put_edge(&mut self, edge: &Edge) -> Result<(), StoreError> {
        if edge.kind == EdgeKind::Causality && !edge.verified_by_gate {
            return Err(StoreError::UnverifiedEdge(edge.id.clone()));
        }
        if self.get_node(&edge.from)?.is_none() {
            return Err(StoreError::UnknownEndpoint(edge.from.clone()));
        }
        if self.get_node(&edge.to)?.is_none() {
            return Err(StoreError::UnknownEndpoint(edge.to.clone()));
        }
        let id = sql_str(&edge.id.0);
        let payload = serde_json::to_string(edge)?;
        self.exec(&format!(
            "INSERT INTO pdsg_edges (edge_id, from_id, to_id, kind, verified, payload) VALUES ({id}, {f}, {t}, {k}, {v}, {p})",
            f = sql_str(&edge.from.0),
            t = sql_str(&edge.to.0),
            k = sql_str(edge.kind.as_tag()),
            v = if edge.verified_by_gate { 1 } else { 0 },
            p = sql_str(&payload),
        ))
    }

    pub fn list_edges(&mut self) -> Result<Vec<Edge>, StoreError> {
        let rows = self.query_rows("SELECT payload FROM pdsg_edges")?;
        let mut out = Vec::new();
        for row in rows {
            let payload = match &row[0] {
                Value::Text(s) => s.clone(),
                other => return Err(StoreError::InvalidPayload(format!("{other:?}"), "text")),
            };
            out.push(
                serde_json::from_str(&payload)
                    .map_err(|e| StoreError::InvalidPayload(e.to_string(), "edge"))?,
            );
        }
        Ok(out)
    }

    /// The subgraph reachable from `root` following edges outbound from
    /// included nodes.
    pub fn subgraph(&mut self, root: &NodeId) -> Result<Subgraph, StoreError> {
        let mut nodes: Vec<Node> = Vec::new();
        let mut edges: Vec<Edge> = Vec::new();
        let mut frontier = vec![root.clone()];
        let mut seen = alloc::collections::BTreeSet::new();
        while let Some(id) = frontier.pop() {
            if !seen.insert(id.clone()) {
                continue;
            }
            if let Some(node) = self.get_node(&id)? {
                nodes.push(node);
            }
            for edge in self.list_edges()? {
                if edge.from == id && !edges.contains(&edge) {
                    frontier.push(edge.to.clone());
                    edges.push(edge);
                }
            }
        }
        Ok(Subgraph { nodes, edges })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn intent(id: &str) -> Node {
        Node::Intent(IntentNode {
            id: NodeId(id.into()),
            intent: "Expose cells to 10uM compound".into(),
            reagent: Some("A".into()),
            target_well: Some("w1".into()),
            parameters: BTreeMap::from([
                ("reagent_a".into(), 1.0),
                ("volume_ul".into(), 100.0),
            ]),
            status: IntentStatus::Pending,
        })
    }

    #[test]
    fn round_trips_node() {
        let mut store = PdsgStore::new().unwrap();
        store.put_node(&intent("i1")).unwrap();
        assert_eq!(store.get_node(&NodeId("i1".into())).unwrap(), Some(intent("i1")));
        assert_eq!(store.list_nodes().unwrap().len(), 1);
    }

    #[test]
    fn update_overwrites_by_id() {
        let mut store = PdsgStore::new().unwrap();
        store.put_node(&intent("i1")).unwrap();
        let mut updated = intent("i1");
        if let Node::Intent(n) = &mut updated {
            n.status = IntentStatus::Completed;
        }
        store.put_node(&updated).unwrap();
        assert_eq!(store.list_nodes().unwrap().len(), 1);
        let got = store.get_node(&NodeId("i1".into())).unwrap().unwrap();
        assert!(matches!(
            got,
            Node::Intent(IntentNode {
                status: IntentStatus::Completed,
                ..
            })
        ));
    }

    #[test]
    fn causality_edge_requires_gate_verification() {
        let mut store = PdsgStore::new().unwrap();
        store.put_node(&intent("i1")).unwrap();
        store
            .put_node(&Node::Actuator(ActuatorNode {
                id: NodeId("a1".into()),
                equipment: "sim-liquid-handler-0".into(),
                state: "idle".into(),
            }))
            .unwrap();
        let unverified = Edge {
            id: EdgeId("e1".into()),
            from: NodeId("i1".into()),
            to: NodeId("a1".into()),
            kind: EdgeKind::Causality,
            verified_by_gate: false,
        };
        assert!(matches!(
            store.put_edge(&unverified),
            Err(StoreError::UnverifiedEdge(_))
        ));
        let verified = Edge {
            id: EdgeId("e1".into()),
            verified_by_gate: true,
            ..unverified
        };
        store.put_edge(&verified).unwrap();
        assert_eq!(store.list_edges().unwrap().len(), 1);
    }

    #[test]
    fn flow_edge_rejects_unknown_endpoint() {
        let mut store = PdsgStore::new().unwrap();
        let edge = Edge {
            id: EdgeId("e1".into()),
            from: NodeId("ghost".into()),
            to: NodeId("s1".into()),
            kind: EdgeKind::Flow,
            verified_by_gate: false,
        };
        assert!(matches!(
            store.put_edge(&edge),
            Err(StoreError::UnknownEndpoint(_))
        ));
    }

    #[test]
    fn subgraph_traverses_causality_chain() {
        let mut store = PdsgStore::new().unwrap();
        store.put_node(&intent("i1")).unwrap();
        store
            .put_node(&Node::Actuator(ActuatorNode {
                id: NodeId("a1".into()),
                equipment: "sim-liquid-handler-0".into(),
                state: "idle".into(),
            }))
            .unwrap();
        store
            .put_node(&Node::Sensor(SensorNode {
                id: NodeId("s1".into()),
                channel: "temperature_milli_c".into(),
                latest: Some(SensorSample {
                    value_scaled: 37_000,
                    at_ms: 0,
                }),
            }))
            .unwrap();
        store
            .put_edge(&Edge {
                id: EdgeId("e1".into()),
                from: NodeId("i1".into()),
                to: NodeId("a1".into()),
                kind: EdgeKind::Causality,
                verified_by_gate: true,
            })
            .unwrap();
        store
            .put_edge(&Edge {
                id: EdgeId("e2".into()),
                from: NodeId("a1".into()),
                to: NodeId("s1".into()),
                kind: EdgeKind::Flow,
                verified_by_gate: false,
            })
            .unwrap();
        let sub = store.subgraph(&NodeId("i1".into())).unwrap();
        assert_eq!(sub.nodes.len(), 3);
        assert_eq!(sub.edges.len(), 2);
    }
}
