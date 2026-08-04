//! `tpt-chrysalis-safety`: the deterministic symbolic safety gate.
//!
//! The single safety choke-point every protocol execution routes through —
//! hand-written or (in Phase 3) AI-proposed. It evaluates a proposed
//! [`Protocol`] against a set of hard [`Rule`]s *before* the orchestrator
//! compiles anything to the HAL. A failing check makes physical execution
//! unreachable; it is never merely logged-and-continued.
//!
//! # Determinism guarantees
//!
//! - Pure: no LLM, no network, no randomness, no time dependence anywhere in
//!   the decision path (this is the zero-hallucination-tolerance design of
//!   ADR 0002 — `tpt-eve`/`tpt-anima` are deliberately excluded here).
//! - Fail-closed: a rule referencing a variable absent from the evaluated
//!   [`Environment`] aborts. An incomplete/malformed protocol cannot default
//!   to permissive.
//! - `f64` arithmetic only on finite literals; no floats enter from sensors at
//!   gate time (sensor values are scaled integers in the audit path).
#![cfg_attr(not(feature = "std"), no_std)]
extern crate alloc;

pub mod environment;
pub mod gate;
pub mod protocol;
pub mod rule;

pub use environment::{Environment, EnvValue, Var};
pub use gate::{GateDecision, SafetyGate};
pub use protocol::{Protocol, ProtocolStep, StepAction};
pub use rule::{Condition, FailureAction, Operator, Rule, RuleCheck, RuleSet};
