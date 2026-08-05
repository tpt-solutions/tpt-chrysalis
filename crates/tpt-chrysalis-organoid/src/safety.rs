//! Deterministic organoid safety rules (spec.txt §4 Layer 3 extension, Phase 2).
//!
//! The organoid extension adds two hard constraints on top of the Phase 1 gate:
//!
//! 1. **Cross-chip biological compatibility** — when [`super::flow_graph::OrganLinkGraph::contamination_env`]
//!    flags an incompatible pairing carrying a reactive reagent, the gate must
//!    abort. This is encoded as a single always-firing rule over
//!    [`Var::CrossChipIncompatible`], so the gate's fail-closed semantics apply.
//! 2. **Per-link flow limits** — enforced structurally by
//!    [`super::flow_graph::OrganLinkGraph::check_flow`] rather than the flat
//!    rule language (each link has its own rated limit), but the result is
//!    folded into the same gate decision so a single choke-point remains.

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use tpt_chrysalis_safety::environment::{EnvValue, Var};
use tpt_chrysalis_safety::gate::SafetyGate;
use tpt_chrysalis_safety::rule::{Condition, FailureAction, Operator, Rule};
use tpt_chrysalis_safety::RuleSet;

use crate::flow_graph::{FlowPlan, OrganLinkGraph};

/// The organoid compatibility rule: aborts whenever cross-chip contamination
/// is flagged (1.0). The precondition is always-true (the environment always
/// carries the flag), so the rule fires unconditionally and the requirement
/// fails exactly when contamination is present. Because the flag variable is
/// always present, a missing-variable "Unknown" (which would also abort) cannot
/// be constructed here — but the gate still aborts on the violated requirement.
pub fn organoid_compatibility_rules() -> RuleSet {
    let mut rs = RuleSet::new();
    rs.add(Rule {
        id: "organoid-cross-chip-incompatible".into(),
        preconditions: vec![Condition::new(
            Var::CrossChipIncompatible,
            Operator::Ge,
            EnvValue::num(0.0),
        )],
        requirement: Condition::new(
            Var::CrossChipIncompatible,
            Operator::Lt,
            EnvValue::num(0.5),
        ),
        on_fail: FailureAction::Abort,
    });
    rs
}

/// The combined organoid gate: the Phase 1 base rules plus the organoid
/// compatibility rule, so every later-phase (and AI-generated) protocol still
/// routes through one deterministic choke-point.
pub fn organoid_gate(base: SafetyGate) -> SafetyGate {
    let mut gate = base;
    for rule in organoid_compatibility_rules().iter() {
        gate.add_rule(rule.clone());
    }
    gate
}

/// Evaluates the cross-chip safety of a proposed flow plan. Returns the errors
/// (flow over-limit, contamination) so callers can report every failure, not
/// just the first.
#[derive(Debug, Clone, PartialEq)]
pub struct CrossChipVerdict {
    pub flow_over_limit: Vec<String>,
    pub contaminated: bool,
    pub allowed: bool,
}

impl CrossChipVerdict {
    pub fn errors(&self) -> Vec<String> {
        let mut out = Vec::new();
        for link_id in &self.flow_over_limit {
            out.push(format!("flow over limit on {link_id}"));
        }
        if self.contaminated {
            out.push("cross-chip biological incompatibility with reactive reagent".into());
        }
        out
    }
}

/// Checks a flow plan against the graph: structural flow limits and the
/// contamination flag. Pure and deterministic; the contamination flag is folded
/// into `env` so the gate (if also run) sees it.
pub fn verify_cross_chip(
    graph: &OrganLinkGraph,
    plan: &FlowPlan,
    active_reagents: &[String],
    env: &mut tpt_chrysalis_safety::Environment,
) -> CrossChipVerdict {
    let contaminated = graph.contamination_env(active_reagents) >= 0.5;
    env.set(
        Var::CrossChipIncompatible,
        if contaminated {
            EnvValue::present()
        } else {
            EnvValue::absent()
        },
    );
    let mut flow_over_limit = Vec::new();
    if let Err(e) = graph.check_flow(plan) {
        // Report every over-limit link by re-scanning.
        for link in graph.links() {
            if let Some(rate) = plan.rate_for(link) {
                if rate > link.max_ul_per_min {
                    flow_over_limit.push(link.id());
                }
            }
        }
        let _ = e;
    }
    let allowed = flow_over_limit.is_empty() && !contaminated;
    CrossChipVerdict {
        flow_over_limit,
        contaminated,
        allowed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flow_graph::{Chip, Link, OrganKind};
    use tpt_chrysalis_safety::Environment;

    fn graph() -> OrganLinkGraph {
        let mut g = OrganLinkGraph::new();
        g.add_chip(Chip {
            id: "liver".into(),
            organ: OrganKind::Liver,
            tolerated_reagents: vec!["A".into(), "B".into()],
        })
        .unwrap();
        g.add_chip(Chip {
            id: "bbb".into(),
            organ: OrganKind::BloodBrainBarrier,
            tolerated_reagents: vec!["A".into()],
        })
        .unwrap();
        g.add_chip(Chip {
            id: "pancreas".into(),
            organ: OrganKind::Pancreas,
            tolerated_reagents: vec!["A".into()],
        })
        .unwrap();
        // Safe conditioning link (Liver -> BBB is a compatible pairing).
        g.add_link(Link {
            from: "liver".into(),
            to: "bbb".into(),
            max_ul_per_min: 20.0,
            allows_reactive: false,
        })
        .unwrap();
        // Incompatible direct pairing (Pancreas -> BBB) used to exercise the
        // contamination flag.
        g.add_link(Link {
            from: "pancreas".into(),
            to: "bbb".into(),
            max_ul_per_min: 20.0,
            allows_reactive: false,
        })
        .unwrap();
        g
    }

    #[test]
    fn contamination_rule_aborts() {
        let gate = organoid_gate(SafetyGate::new());
        let mut env = Environment::new();
        env.set(Var::CrossChipIncompatible, EnvValue::present());
        let p = tpt_chrysalis_safety::protocol::Protocol::new("p");
        match gate.evaluate(&p, &env) {
            tpt_chrysalis_safety::gate::GateDecision::Abort { rule_id, .. } => {
                assert_eq!(rule_id, "organoid-cross-chip-incompatible");
            }
            other => panic!("expected abort, got {other:?}"),
        }
    }

    #[test]
    fn clean_cross_chip_passes_and_reports() {
        let g = graph();
        let mut plan = FlowPlan::default();
        plan.set(g.link("liver->bbb").unwrap(), 10.0);
        let mut env = Environment::new();
        let v = verify_cross_chip(&g, &plan, &["A".into()], &mut env);
        assert!(v.allowed);
        assert!(v.errors().is_empty());
        // flag is 0.0 for tolerated reagents
        assert_eq!(env.get(Var::CrossChipIncompatible), Some(EnvValue::absent()));
    }

    #[test]
    fn over_limit_and_contamination_reported() {
        let g = graph();
        let mut plan = FlowPlan::default();
        plan.set(g.link("liver->bbb").unwrap(), 50.0);
        let mut env = Environment::new();
        let v = verify_cross_chip(&g, &plan, &["B".into()], &mut env);
        assert!(!v.allowed);
        assert!(v.contaminated);
        assert_eq!(v.flow_over_limit, vec!["liver->bbb".to_string()]);
        assert_eq!(v.errors().len(), 2);
    }
}
