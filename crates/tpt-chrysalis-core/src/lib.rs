//! `tpt-chrysalis-core`: the foundation every domain crate depends on.
//!
//! Three responsibilities (per spec.txt §4 Layer 2, delegated to `tpt-archon`
//! — see ADR 0001):
//!
//! - [`ledger`] — the immutable, hash-chained audit ledger built on
//!   `tpt-archon-core`'s append-only WAL. Every sensor reading, actuator
//!   movement, and environmental condition is recorded here before/with every
//!   command; no raw command executes outside this logging choke-point.
//! - [`security`] — chrysalis-specific physical-action capability tokens that
//!   mint and verify against `tpt-archon-bridge`'s unforgeable
//!   [`Capability`]s, adding action-scoping and short expiry.
//! - [`pdsg`] — the Physical-Digital State Graph node/edge model
//!   (spec.txt §3) backed by `tpt-archon-relational`'s SQL engine for
//!   storage/query.

extern crate alloc;

pub mod ledger;
pub mod pdsg;
pub mod security;

pub use ledger::{
    AuditEntry, AuditLedger, AuditOutcome, LedgerError, LedgerIntegrityReport, LedgerOutcome,
    PhysicalAction, SaveLoad,
};
pub use pdsg::{
    ActuatorNode, Edge, EdgeId, EdgeKind, IntentNode, IntentStatus, Node, NodeId, PdsgStore,
    SensorNode, SensorSample, StoreError, Subgraph,
};
pub use security::{ActionId, ActionToken, TokenAuthority, TokenError};

/// The hardware scope every token in this deployment is issued for (the
/// Phase 1 simulated liquid handler). Kept in one place so scope drift between
/// issuance and verification is impossible.
pub const DEFAULT_SCOPE: &str = "equipment/sim-liquid-handler-0";
