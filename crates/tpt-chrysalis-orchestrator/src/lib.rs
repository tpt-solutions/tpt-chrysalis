//! `tpt-chrysalis-orchestrator`: the universal translator between biological
//! intent and physical machine action (spec.txt §5A).
//!
//! Owns the Phase 1 flow end-to-end:
//!
//! ```text
//! intent ──► protocol compiler ──► safety gate ──► capability-gated
//! simulated actuation ──► audit ledger entry ──► PDSG updated
//! ```
//!
//! Every step of that flow is real and testable even though the backend is a
//! simulator (ADR 0003): resource locking, the safety gate, capability
//! enforcement through `tpt-archon-kernel`'s `MessageRouter`, the hash-chained
//! audit ledger, and the relational-backed PDSG.
extern crate alloc;

pub mod compiler;
pub mod gateway;
pub mod lock;
pub mod orchestrator;
pub mod registry;

pub use compiler::{CompileError, ProtocolCompiler};
pub use gateway::{GatewayError, HardwareGateway};
pub use lock::{LockError, LockManager, ResourceId};
pub use orchestrator::{AcceptedStep, ExecutionReport, OrchestrationError, Orchestrator};
pub use registry::{ReagentRegistry, TargetRegistry, ValveRegistry, WellRegistry};
