//! The gate: the deterministic evaluator over a protocol + environment.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::environment::Environment;
use crate::protocol::Protocol;
use crate::rule::{FailureAction, Rule, RuleCheck, RuleSet};

/// The result of a gate evaluation. Only [`Allow`](GateDecision::Allow)
/// permits compilation to the HAL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GateDecision {
    /// Safe to proceed.
    Allow,
    /// Blocked. `reason` identifies the rule and the failure.
    Abort { rule_id: String, reason: String },
    /// A non-aborting rule flagged a violation (warning only). Execution may
    /// proceed but the report must carry it.
    Alert { rule_id: String, reason: String },
}

/// The deterministic constraint engine. Holds the rule set; evaluation is
/// pure with respect to `(protocol, environment)`.
#[derive(Debug, Clone)]
pub struct SafetyGate {
    rules: RuleSet,
}

impl Default for SafetyGate {
    fn default() -> Self {
        Self::new()
    }
}

impl SafetyGate {
    pub fn new() -> Self {
        Self {
            rules: RuleSet::new(),
        }
    }

    pub fn with_rules(rules: RuleSet) -> Self {
        Self { rules }
    }

    pub fn add_rule(&mut self, rule: Rule) -> &mut Self {
        self.rules.add(rule);
        self
    }

    pub fn rules(&self) -> &RuleSet {
        &self.rules
    }

    /// Evaluates `protocol` against the rule set under `env`.
    ///
    /// Semantics: every rule is evaluated against the protocol's declared
    /// environment merged with `env`. A rule that fires and whose requirement
    /// fails yields an `Abort` (fail-closed) or `Alert` depending on the
    /// rule's `FailureAction`. Any referenced variable that is unknown to the
    /// merged environment yields an `Abort` — incomplete protocols are
    /// rejected, never defaulted to permissive.
    ///
    /// An `Abort` result means the orchestrator must NOT compile the protocol
    /// to the HAL: physical execution is unreachable, not merely logged.
    pub fn evaluate(&self, protocol: &Protocol, env: &Environment) -> GateDecision {
        let mut merged = protocol.declared_environment();
        for v in env.vars() {
            if let Some(val) = env.get(v) {
                merged.set(v, val);
            }
        }

        let mut first_abort: Option<GateDecision> = None;
        let mut alerts: Vec<GateDecision> = Vec::new();

        for rule in self.rules.iter() {
            match rule.check(&merged) {
                RuleCheck::NotFired | RuleCheck::Satisfied => {}
                RuleCheck::Violated => {
                    let reason = format!("rule {}: requirement violated", rule.id);
                    match rule.on_fail {
                        FailureAction::Abort => {
                            first_abort.get_or_insert_with(|| GateDecision::Abort {
                                rule_id: rule.id.clone(),
                                reason,
                            });
                        }
                        FailureAction::Alert => alerts.push(GateDecision::Alert {
                            rule_id: rule.id.clone(),
                            reason,
                        }),
                    }
                }
                RuleCheck::Unknown => {
                    let reason = format!(
                        "rule {}: incomplete environment (a referenced variable is unknown)",
                        rule.id
                    );
                    first_abort.get_or_insert_with(|| GateDecision::Abort {
                        rule_id: rule.id.clone(),
                        reason,
                    });
                }
            }
        }

        first_abort.unwrap_or_else(|| {
            if alerts.is_empty() {
                GateDecision::Allow
            } else {
                alerts.into_iter().next().expect("non-empty")
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::environment::{EnvValue, Var};
    use crate::protocol::{ProtocolStep, StepAction};
    use crate::rule::{Condition, FailureAction, Operator, Rule};
    use alloc::vec;

    fn temp_rule() -> Rule {
        // IF reagent_A AND reagent_B THEN temperature MUST BE < 4°C ELSE abort
        Rule {
            id: "rxn-temp-limit".into(),
            preconditions: vec![
                Condition::new(Var::ReagentA, Operator::Ge, EnvValue::num(0.5)),
                Condition::new(Var::ReagentB, Operator::Ge, EnvValue::num(0.5)),
            ],
            requirement: Condition::new(Var::TemperatureMilliC, Operator::Lt, EnvValue::num(4_000.0)),
            on_fail: FailureAction::Abort,
        }
    }

    fn dispense_protocol(temp_condition: Option<Condition>) -> Protocol {
        let mut p = Protocol::new("p1");
        let mut conds = vec![
            Condition::new(Var::ReagentA, Operator::Ge, EnvValue::num(0.5)),
            Condition::new(Var::ReagentB, Operator::Ge, EnvValue::num(0.5)),
        ];
        if let Some(c) = temp_condition {
            conds.push(c);
        }
        p.push(ProtocolStep {
            id: "s1".into(),
            action: StepAction::Dispense {
                reagent: "A".into(),
                volume_ul: 10,
                target_well: "w1".into(),
            },
            declared_conditions: conds,
        });
        p
    }

    #[test]
    fn known_safe_protocol_passes() {
        let gate = SafetyGate::new().add_rule(temp_rule()).clone();
        let p = dispense_protocol(Some(Condition::new(
            Var::TemperatureMilliC,
            Operator::Lt,
            EnvValue::num(3_000.0),
        )));
        assert_eq!(gate.evaluate(&p, &Environment::new()), GateDecision::Allow);
    }

    #[test]
    fn known_unsafe_protocol_is_blocked() {
        let gate = SafetyGate::new().add_rule(temp_rule()).clone();
        // Both reagents present but the protocol declares a warm temperature.
        let p = dispense_protocol(Some(Condition::new(
            Var::TemperatureMilliC,
            Operator::Gt,
            EnvValue::num(25_000.0),
        )));
        match gate.evaluate(&p, &Environment::new()) {
            GateDecision::Abort { rule_id, .. } => assert_eq!(rule_id, "rxn-temp-limit"),
            other => panic!("expected Abort, got {other:?}"),
        }
    }

    #[test]
    fn incomplete_protocol_is_rejected_not_permissive() {
        let gate = SafetyGate::new().add_rule(temp_rule()).clone();
        // Declares reagents but no temperature at all.
        let p = dispense_protocol(None);
        match gate.evaluate(&p, &Environment::new()) {
            GateDecision::Abort { .. } => {}
            other => panic!("expected Abort for incomplete env, got {other:?}"),
        }
    }

    #[test]
    fn missing_variable_fails_closed() {
        let gate = SafetyGate::new().add_rule(temp_rule()).clone();
        let p = dispense_protocol(Some(Condition::new(
            Var::TemperatureMilliC,
            Operator::Lt,
            EnvValue::num(3_000.0),
        )));
        // Empty external env is fine here because the protocol declares the
        // temperature; a protocol that declares it correctly passes.
        let decision = gate.evaluate(&p, &Environment::new());
        assert_eq!(decision, GateDecision::Allow);
        // A protocol that does NOT declare a var referenced by the rule must
        // abort.
        let mut incomplete = Protocol::new("p2");
        incomplete.push(ProtocolStep {
            id: "s2".into(),
            action: StepAction::SetValve {
                valve: "v1".into(),
                open: true,
            },
            declared_conditions: vec![Condition::new(
                Var::ReagentA,
                Operator::Ge,
                EnvValue::num(0.5),
            )],
        });
        match gate.evaluate(&incomplete, &Environment::new()) {
            GateDecision::Abort { .. } => {}
            other => panic!("expected Abort, got {other:?}"),
        }
    }
}
