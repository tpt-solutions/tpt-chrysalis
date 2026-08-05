//! The simulated objective the Phase 3 loop optimizes against.
//!
//! No real hardware is available in Phase 3 (simulation-first), so the loop
//! drives a deterministic 1-D well. The same [`Objective`] trait will be
//! backed by real readouts once hardware exists; the learner and gate are
//! untouched.

use alloc::string::String;

use crate::interop::ProtocolProposal;

/// Feasible range for the optimized variable.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds {
    pub min: f64,
    pub max: f64,
}

/// Anything the loop can evaluate a proposed protocol against.
pub trait Objective {
    /// The variable name this objective reads from a proposal's steps.
    fn var(&self) -> &str;
    fn bounds(&self) -> Bounds;
    /// The objective value for a proposal (NaN if the proposal lacks the var).
    fn evaluate(&self, proposal: &ProtocolProposal) -> f64;
}

/// A smooth 1-D well peaking at `optimum` (for `maximize` goals). Deterministic
/// and bounded in [0,1].
#[derive(Debug, Clone)]
pub struct SimulatedObjective {
    pub var_name: String,
    pub bounds: Bounds,
    pub optimum: f64,
}

impl Default for SimulatedObjective {
    fn default() -> Self {
        Self {
            var_name: "volume_ul".into(),
            bounds: Bounds { min: 0.0, max: 200.0 },
            optimum: 100.0,
        }
    }
}

impl Objective for SimulatedObjective {
    fn var(&self) -> &str {
        &self.var_name
    }
    fn bounds(&self) -> Bounds {
        self.bounds
    }
    fn evaluate(&self, proposal: &ProtocolProposal) -> f64 {
        let v = proposal
            .steps
            .iter()
            .find(|s| s.var == self.var_name)
            .map(|s| s.value);
        let v = match v {
            Some(v) => v,
            None => return f64::NAN,
        };
        let span = (self.bounds.max - self.bounds.min).max(1e-9);
        let x = (v - self.optimum) / span;
        (1.0 - x * x).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interop::ProposedStep;

    #[test]
    fn peaks_at_optimum() {
        let o = SimulatedObjective::default();
        let at = |v: f64| o.evaluate(&ProtocolProposal {
            id: "p".into(),
            steps: vec![ProposedStep { var: "volume_ul".into(), value: v }],
            rationale: String::new(),
        });
        assert!((at(100.0) - 1.0).abs() < 1e-9);
        assert!(at(0.0) < at(100.0));
        assert!(at(50.0) < 1.0);
    }
}
