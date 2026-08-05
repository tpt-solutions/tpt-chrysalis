//! `tpt-chrysalis-organoid`: the Phase 2 multi-organ expansion (spec.txt §2/§8).
//!
//! Two responsibilities, both deterministic and auditable:
//!
//! - [`flow_graph`] — the simulated microfluidic "blood flow" graph linking
//!   several Organ-on-a-Chip devices (Pancreas → Liver → Blood-Brain-Barrier),
//!   extending the Phase 1 PDSG up to chip-level granularity. Enforces
//!   acyclic flow and per-link rated flow limits.
//! - [`pdo`] — Patient-Derived Organoid screening: expands therapy dose sweeps
//!   into per-therapy simulated dosing runs and evaluates them against a
//!   swappable [`pdo::PdoSimBackend`].
//!
//! [`safety`] wires the organoid-specific constraints into the Phase 1
//! [`tpt_chrysalis_safety`] gate so the single deterministic choke-point is
//! preserved for hand-written *and* (Phase 3) AI-proposed protocols.

extern crate alloc;

pub mod flow_graph;
pub mod pdo;
pub mod safety;

pub use flow_graph::{
    Chip, FlowPlan, GraphError, Link, OrganKind, OrganLinkGraph,
};
pub use pdo::{
    DoseOutcome, DoseRun, PdoScreen, PdoSimBackend, SyntheticPdoBackend, Therapy,
};
pub use safety::{
    organoid_compatibility_rules, organoid_gate, verify_cross_chip, CrossChipVerdict,
};
