//! The Phase 3 autonomous closed loop: `tpt-anima` (active-learning runtime)
//! and `tpt-eve` (symbolic/causal advisor) integrated as a *defined interop
//! surface*, never as a substitute for the safety gate.
//!
//! Per spec.txt §8 Phase 3 and ADR 0002, every protocol proposed by the
//! learning loop — hand-tuned or AI-generated — is evaluated by the Phase 1
//! [`tpt_chrysalis_safety`] gate **unchanged**. The learning components
//! (`ProposalSource`, `Advisor`) are traits so the real `tpt-anima`/`tpt-eve`
//! crates (audited in Phase 0) can be dropped in behind them once available,
//! without any change to the safety path.
//!
//! Modules:
//! - [`interop`] — the shared protocol-proposal contract and the
//!   [`interop::ProposalSource`] trait the learner implements.
//! - [`advisor`] — the [`advisor::Advisor`] trait (`tpt-eve`'s advisory role).
//! - [`objective`] — a simulated objective the loop optimizes against (no real
//!   hardware in Phase 3).
//! - [`sources`] — a model-based active learner ([`sources::QuadraticFitSource`])
//!   and a grid-search baseline ([`sources::GridSearchSource`]).
//! - [`loop_run`] — [`loop_run::ClosedLoop`], which wires a source + advisor +
//!   the safety gate + the objective into one deterministic, auditable run.

extern crate alloc;

pub mod advisor;
pub mod interop;
pub mod loop_run;
pub mod objective;
pub mod sources;

pub use advisor::{Advisor, EveAdvisor};
pub use interop::{Goal, ProposedStep, ProposalSource, ProtocolProposal, Trial};
pub use loop_run::{ClosedLoop, LoopReport};
pub use objective::{Bounds, Objective, SimulatedObjective};
pub use sources::{GridSearchSource, QuadraticFitSource};
