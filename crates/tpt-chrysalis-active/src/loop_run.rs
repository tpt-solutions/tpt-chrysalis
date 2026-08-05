//! The closed loop: wires a learner + advisor + the safety gate + a simulated
//! objective into one deterministic, auditable run (spec.txt §8 Phase 3).
//!
//! The critical invariant is enforced here, not in the AI components: **every
//! proposal is evaluated by the Phase 1 [`tpt_chrysalis_safety`] gate before
//! any simulated execution**. An [`GateDecision::Abort`](tpt_chrysalis_safety::gate::GateDecision::Abort)
//! means the protocol is *not executed* — the loop simply proposes again. No
//! new bypass path is introduced for AI-generated protocols.

use alloc::string::String;
use alloc::vec::Vec;

use tpt_chrysalis_safety::environment::Var;
use tpt_chrysalis_safety::gate::{GateDecision, SafetyGate};
use tpt_chrysalis_safety::protocol::Protocol;
use tpt_chrysalis_safety::rule::{Condition, FailureAction, Operator, Rule};
use tpt_chrysalis_safety::EnvValue;

use crate::advisor::{Advisor, EveAdvisor};
use crate::interop::{proposal_environment, Goal, ProposalSource, Trial};
use crate::objective::Objective;
use crate::sources::{grid_search_evals_to_target, QuadraticFitSource};

/// Outcome of one closed-loop run.
#[derive(Debug, Clone, PartialEq)]
pub struct LoopReport {
    pub best_value: f64,
    pub best_proposal_id: String,
    /// Simulated executions that actually happened (gate-allowed).
    pub evaluations: u32,
    /// Proposals the gate allowed.
    pub accepted: u32,
    /// Proposals the gate blocked (never executed).
    pub blocked_by_safety: u32,
}

/// The autonomous loop. The objective is borrowed so the same objective can be
/// reused for the benchmark baseline.
#[derive(Debug)]
pub struct ClosedLoop<'a, S: ProposalSource, A: Advisor, O: Objective + ?Sized> {
    goal: Goal,
    source: S,
    advisor: A,
    gate: SafetyGate,
    objective: &'a O,
    history: Vec<Trial>,
    protocol: Protocol,
    report: LoopReport,
}

impl<'a, S: ProposalSource, A: Advisor, O: Objective + ?Sized> ClosedLoop<'a, S, A, O> {
    pub fn new(
        goal: Goal,
        source: S,
        advisor: A,
        gate: SafetyGate,
        objective: &'a O,
    ) -> Self {
        Self {
            goal,
            source,
            advisor,
            gate,
            objective,
            history: Vec::new(),
            protocol: Protocol::new("active-loop"),
            report: LoopReport {
                best_value: f64::NAN,
                best_proposal_id: String::new(),
                evaluations: 0,
                accepted: 0,
                blocked_by_safety: 0,
            },
        }
    }

    /// Runs `iterations` propose → advise → gate → (execute) cycles.
    pub fn run(&mut self, iterations: u32) -> LoopReport {
        for _ in 0..iterations {
            let proposal = self.source.propose(&self.goal, &self.history);
            let advice = self.advisor.advise(&self.goal, &self.history);
            let mut proposal = proposal;
            proposal.rationale = advice;

            let env = proposal_environment(&proposal);
            match self.gate.evaluate(&self.protocol, &env) {
                GateDecision::Allow => {
                    let observed = self.objective.evaluate(&proposal);
                    if !observed.is_finite() {
                        // A proposal the objective cannot read is, here, treated
                        // as unsafe/incomplete and not executed.
                        self.report.blocked_by_safety += 1;
                        continue;
                    }
                    self.history.push(Trial {
                        proposal,
                        observed,
                    });
                    self.report.evaluations += 1;
                    self.report.accepted += 1;
                    if self.report.best_value.is_nan()
                        || better(observed, self.report.best_value, self.goal.maximize)
                    {
                        self.report.best_value = observed;
                        self.report.best_proposal_id =
                            self.history.last().unwrap().proposal.id.clone();
                    }
                }
                _ => {
                    self.report.blocked_by_safety += 1;
                }
            }
        }
        self.report.clone()
    }

    pub fn history(&self) -> &[Trial] {
        &self.history
    }
}

fn better(candidate: f64, incumbent: f64, maximize: bool) -> bool {
    if maximize {
        candidate > incumbent
    } else {
        candidate < incumbent
    }
}

/// A gate that caps a volume set-point at `max_ul` — the only hard constraint
/// the Phase 3 loop must still satisfy (it never relaxes it for AI proposals).
pub fn volume_limit_gate(max_ul: f64) -> SafetyGate {
    let mut gate = SafetyGate::new();
    gate.add_rule(Rule {
        id: "active-volume-limit".into(),
        preconditions: vec![Condition::new(
            Var::VolumeUl,
            Operator::Ge,
            EnvValue::num(0.0),
        )],
        requirement: Condition::new(Var::VolumeUl, Operator::Le, EnvValue::num(max_ul)),
        on_fail: FailureAction::Abort,
    });
    gate
}

/// Benchmark result comparing the active learner to an exhaustive grid search
/// on the same objective: the measured evaluation-cost ratio (spec.txt §5C).
#[derive(Debug, Clone, PartialEq)]
pub struct BenchmarkReport {
    pub active_best: f64,
    pub active_evals: u32,
    pub grid_evals_to_match: u32,
    /// `grid_evals_to_match / active_evals` — the measured efficiency gain.
    pub ratio: f64,
}

/// Runs the active learner and an equivalent grid search, reporting the actual
/// measured evaluation-cost ratio (not the target claim).
pub fn benchmark<O: Objective>(
    goal: Goal,
    objective: &O,
    active_seed: u64,
    iterations: u32,
    grid_steps: u32,
    epsilon: f64,
) -> BenchmarkReport {
    let bounds = objective.bounds();
    let gate = volume_limit_gate(bounds.max);

    let active = QuadraticFitSource::new(objective.var().to_string(), bounds, active_seed);
    let advisor = EveAdvisor;
    let mut loop_ = ClosedLoop::new(goal.clone(), active, advisor, gate, objective);
    let report = loop_.run(iterations);
    let target = report.best_value;

    let grid_evals = grid_search_evals_to_target(
        &goal,
        objective,
        bounds,
        grid_steps,
        target,
        epsilon,
        grid_steps,
    );
    let ratio = grid_evals as f64 / report.evaluations.max(1) as f64;
    BenchmarkReport {
        active_best: target,
        active_evals: report.evaluations,
        grid_evals_to_match: grid_evals,
        ratio,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objective::SimulatedObjective;

    #[test]
    fn loop_reaches_near_optimum_and_counts_safety() {
        let obj = SimulatedObjective::default();
        let goal = Goal {
            id: "max-viability".into(),
            maximize: true,
            description: String::new(),
        };
        let gate = volume_limit_gate(obj.bounds().max);
        let src = QuadraticFitSource::new(obj.var().to_string(), obj.bounds(), 3);
        let mut loop_ = ClosedLoop::new(goal, src, EveAdvisor, gate, &obj);
        let report = loop_.run(12);
        assert!(report.best_value > 0.9, "best = {}", report.best_value);
        assert_eq!(report.evaluations, report.accepted);
        assert_eq!(report.blocked_by_safety, 0);
    }

    #[test]
    fn safety_gate_blocks_out_of_bounds_proposals() {
        let obj = SimulatedObjective::default();
        let goal = Goal {
            id: "g".into(),
            maximize: true,
            description: String::new(),
        };
        // A restrictive gate (<= 60µl) blocks the learner's higher proposals.
        let gate = volume_limit_gate(60.0);
        let src = QuadraticFitSource::new(obj.var().to_string(), obj.bounds(), 5);
        let mut loop_ = ClosedLoop::new(goal, src, EveAdvisor, gate, &obj);
        let report = loop_.run(10);
        assert!(report.blocked_by_safety > 0);
        // Blocked proposals are never executed: evaluations == accepted.
        assert_eq!(report.evaluations, report.accepted);
    }

    #[test]
    fn benchmark_reports_measured_ratio() {
        let obj = SimulatedObjective::default();
        let goal = Goal {
            id: "bench".into(),
            maximize: true,
            description: String::new(),
        };
        let b = benchmark(goal, &obj, 9, 15, 40, 0.02);
        // The active learner should reach the grid's best with fewer evals.
        assert!(b.ratio >= 1.0);
        assert_eq!(b.active_evals, 15);
    }
}
