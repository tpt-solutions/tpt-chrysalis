//! The shared protocol-proposal contract for the Phase 3 active-learning loop.
//!
//! A [`ProtocolProposal`] is the cross-component message: the learner
//! (`tpt-anima`) produces one, the advisor (`tpt-eve`) may annotate its
//! rationale, and the safety gate evaluates it *before* any simulated
//! execution. Keeping this as a plain data contract is what makes the real
//! sibling crates drop-in replaceable (per the Phase 0 API audit).

use alloc::string::String;
use alloc::vec::Vec;

use serde::{Deserialize, Serialize};
use tpt_chrysalis_safety::environment::{EnvValue, Var};

/// A high-level optimization goal for the loop.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Goal {
    pub id: String,
    /// True to maximize the objective, false to minimize.
    pub maximize: bool,
    pub description: String,
}

/// One proposed set-point: a gate variable and a value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProposedStep {
    pub var: String,
    pub value: f64,
}

/// A learner-proposed protocol (one or more set-points). `rationale` carries
/// any advisory context from `tpt-eve`; it never affects the gate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProtocolProposal {
    pub id: String,
    pub steps: Vec<ProposedStep>,
    pub rationale: String,
}

/// A completed trial: the proposal and the objective value it achieved (after
/// passing the gate and "executing" against the simulated objective).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Trial {
    pub proposal: ProtocolProposal,
    pub observed: f64,
}

/// The active-learning runtime (`tpt-anima`). Implemented locally by
/// [`crate::sources`] and, once available, by the real `tpt-anima` crate
/// behind the same trait.
pub trait ProposalSource {
    /// Propose the next protocol given the trial history.
    fn propose(&mut self, goal: &Goal, history: &[Trial]) -> ProtocolProposal;
}

/// Maps a proposal's string step names into a gate [`tpt_chrysalis_safety::Environment`].
pub fn proposal_environment(proposal: &ProtocolProposal) -> tpt_chrysalis_safety::Environment {
    use tpt_chrysalis_safety::Environment;
    let mut env = Environment::new();
    for step in &proposal.steps {
        if let Some(var) = var_from_name(&step.var) {
            env.set(var, EnvValue::num(step.value));
        }
    }
    env
}

/// Resolves a gate variable name to its [`Var`] (the closed set the gate knows).
pub fn var_from_name(name: &str) -> Option<Var> {
    Some(match name {
        "temperature_milli_c" => Var::TemperatureMilliC,
        "pressure_mbar" => Var::PressureMbar,
        "ph_milli" => Var::PhMilli,
        "volume_ul" => Var::VolumeUl,
        "reagent_a" => Var::ReagentA,
        "reagent_b" => Var::ReagentB,
        "reagent_c" => Var::ReagentC,
        "flow_rate_ul_min" => Var::FlowRateUlMin,
        "cross_chip_incompatible" => Var::CrossChipIncompatible,
        "barrier_integrity" => Var::BarrierIntegrity,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn environment_maps_known_vars_only() {
        let p = ProtocolProposal {
            id: "p1".into(),
            steps: vec![
                ProposedStep { var: "volume_ul".into(), value: 50.0 },
                ProposedStep { var: "unknown_var".into(), value: 1.0 },
            ],
            rationale: String::new(),
        };
        let env = proposal_environment(&p);
        assert_eq!(env.get(Var::VolumeUl), Some(EnvValue::num(50.0)));
        assert_eq!(env.get(Var::ReagentA), None);
    }
}
