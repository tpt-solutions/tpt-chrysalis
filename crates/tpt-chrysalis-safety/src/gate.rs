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

/// Regression suite for protocols proposed by an autonomous loop (Phase 3's
/// `tpt-anima`-driven active learning). AI-generated protocols are more likely
/// to probe edge cases in the rule schema, so this suite fuzzes and mutates
/// protocol shapes and asserts the deterministic, fail-closed invariants of the
/// gate never regress.
#[cfg(test)]
mod adversarial_tests {
    use super::*;
    use crate::environment::{EnvValue, Var};
    use crate::protocol::{Protocol, ProtocolStep, StepAction};
    use crate::rule::{Condition, FailureAction, Operator, Rule};
    use alloc::format;
    use alloc::string::String;
    use alloc::vec;
    use alloc::vec::Vec;

    /// Deterministic xorshift PRNG so the fuzz runs are reproducible.
    struct Rng(u64);
    impl Rng {
        fn new(seed: u64) -> Self {
            Rng(seed | 1)
        }
        fn next(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.0 = x;
            x
        }
        fn below(&mut self, n: u64) -> u64 {
            self.next() % n
        }
        fn chance(&mut self, p: u64) -> bool {
            self.below(100) < p
        }
    }

    /// The rule set an autonomous loop would be checked against: a reaction
    /// temperature limit, an always-on dispense volume guard, and a cross-chip
    /// biological-compatibility barrier.
    fn standard_rules() -> RuleSet {
        let mut rs = RuleSet::new();
        rs.add(Rule {
            id: "rxn-temp-limit".into(),
            preconditions: vec![
                Condition::new(Var::ReagentA, Operator::Ge, EnvValue::num(0.5)),
                Condition::new(Var::ReagentB, Operator::Ge, EnvValue::num(0.5)),
            ],
            requirement: Condition::new(Var::TemperatureMilliC, Operator::Lt, EnvValue::num(4_000.0)),
            on_fail: FailureAction::Abort,
        });
        rs.add(Rule {
            id: "dispense-volume-limit".into(),
            preconditions: vec![],
            requirement: Condition::new(Var::VolumeUl, Operator::Le, EnvValue::num(1_000.0)),
            on_fail: FailureAction::Abort,
        });
        rs.add(Rule {
            id: "cross-chip-barrier".into(),
            preconditions: vec![Condition::new(
                Var::CrossChipIncompatible,
                Operator::Eq,
                EnvValue::num(1.0),
            )],
            requirement: Condition::new(Var::FlowRateUlMin, Operator::Le, EnvValue::num(0.0)),
            on_fail: FailureAction::Abort,
        });
        rs
    }

    fn reagent_name(r: u64) -> String {
        match r % 3 {
            0 => "A",
            1 => "B",
            _ => "C",
        }
        .into()
    }

    /// Build a protocol shaped like one an active-learning loop might propose:
    /// random steps, reagent presence, temperatures (sometimes boundary /
    /// non-finite), volumes, and cross-chip flags.
    fn ai_protocol(rng: &mut Rng) -> Protocol {
        let mut p = Protocol::new("ai-protocol");
        let n = 1 + rng.below(4) as usize;
        for i in 0..n {
            let step_kind = rng.below(4);
            let mut conds: Vec<Condition> = Vec::new();
            if rng.chance(50) {
                conds.push(Condition::new(Var::ReagentA, Operator::Ge, EnvValue::num(0.5)));
            }
            if rng.chance(50) {
                conds.push(Condition::new(Var::ReagentB, Operator::Ge, EnvValue::num(0.5)));
            }
            if rng.chance(70) {
                // Temperature in [-20C, 60C], with occasional boundary / non-finite probes.
                let t = match rng.below(10) {
                    0 => 4_000.0,
                    1 => f64::INFINITY,
                    2 => f64::NEG_INFINITY,
                    3 => f64::NAN,
                    _ => (rng.below(80_001) as f64) - 20_000.0,
                };
                conds.push(Condition::new(Var::TemperatureMilliC, Operator::Lt, EnvValue::num(t)));
            }
            if rng.chance(40) {
                let vol = rng.below(2_000) as f64;
                conds.push(Condition::new(Var::VolumeUl, Operator::Le, EnvValue::num(vol)));
            }
            if rng.chance(20) {
                let flow = rng.below(200) as f64;
                conds.push(Condition::new(
                    Var::CrossChipIncompatible,
                    Operator::Eq,
                    EnvValue::num(if rng.chance(50) { 1.0 } else { 0.0 }),
                ));
                conds.push(Condition::new(Var::FlowRateUlMin, Operator::Le, EnvValue::num(flow)));
            }
            let action = match step_kind {
                0 => StepAction::Dispense {
                    reagent: reagent_name(rng.next()),
                    volume_ul: rng.below(2_000) as u32,
                    target_well: format!("w{i}"),
                },
                1 => StepAction::SetValve {
                    valve: format!("v{i}"),
                    open: rng.chance(50),
                },
                _ => StepAction::SetTemperature {
                    setpoint_milli_c: (rng.below(80_001) as i64) - 20_000,
                },
            };
            p.push(ProtocolStep {
                id: format!("s{i}"),
                action,
                declared_conditions: conds,
            });
        }
        p
    }

    #[test]
    fn ai_protocols_evaluate_deterministically_and_terminate() {
        let gate = SafetyGate::with_rules(standard_rules());
        let mut rng = Rng::new(0xDEAD_BEEF_1234);
        for _ in 0..3000 {
            let p = ai_protocol(&mut rng);
            let a = gate.evaluate(&p, &Environment::new());
            let b = gate.evaluate(&p, &Environment::new());
            assert_eq!(a, b, "gate must be deterministic for the same input");
            match a {
                GateDecision::Allow | GateDecision::Abort { .. } | GateDecision::Alert { .. } => {}
            }
        }
    }

    #[test]
    fn declared_reaction_violation_is_always_aborted() {
        let gate = SafetyGate::with_rules(standard_rules());
        let mut rng = Rng::new(0x1234_ABCD);
        for _ in 0..3000 {
            let mut p = Protocol::new("ai");
            let step_kind = rng.below(3);
            let conds = vec![
                Condition::new(Var::ReagentA, Operator::Ge, EnvValue::num(0.5)),
                Condition::new(Var::ReagentB, Operator::Ge, EnvValue::num(0.5)),
                Condition::new(Var::TemperatureMilliC, Operator::Lt, EnvValue::num(25_000.0)),
            ];
            let action = match step_kind {
                0 => StepAction::Dispense {
                    reagent: "A".into(),
                    volume_ul: 10,
                    target_well: "w1".into(),
                },
                _ => StepAction::SetTemperature {
                    setpoint_milli_c: 25_000,
                },
            };
            p.push(ProtocolStep {
                id: "s1".into(),
                action,
                declared_conditions: conds,
            });
            match gate.evaluate(&p, &Environment::new()) {
                GateDecision::Abort { rule_id, .. } => assert_eq!(rule_id, "rxn-temp-limit"),
                other => panic!("AI protocol declaring a hot reaction must abort, got {other:?}"),
            }
        }
    }

    #[test]
    fn exact_boundary_and_nonfinite_temps_fail_closed() {
        let gate = SafetyGate::with_rules(standard_rules());

        // Temperature declared exactly at the limit with a strict `Lt` requirement
        // is a violation, not a pass.
        let mut p = Protocol::new("boundary");
        p.push(ProtocolStep {
            id: "s1".into(),
            action: StepAction::Dispense {
                reagent: "A".into(),
                volume_ul: 10,
                target_well: "w1".into(),
            },
            declared_conditions: vec![
                Condition::new(Var::ReagentA, Operator::Ge, EnvValue::num(0.5)),
                Condition::new(Var::ReagentB, Operator::Ge, EnvValue::num(0.5)),
                Condition::new(Var::TemperatureMilliC, Operator::Lt, EnvValue::num(4_000.0)),
            ],
        });
        assert!(matches!(
            gate.evaluate(&p, &Environment::new()),
            GateDecision::Abort { .. }
        ));

        // Non-finite declared temperatures must never yield Allow.
        for bad in [f64::INFINITY, f64::NEG_INFINITY, f64::NAN] {
            let mut pb = Protocol::new("nonfinite");
            pb.push(ProtocolStep {
                id: "s1".into(),
                action: StepAction::Dispense {
                    reagent: "A".into(),
                    volume_ul: 10,
                    target_well: "w1".into(),
                },
                declared_conditions: vec![
                    Condition::new(Var::ReagentA, Operator::Ge, EnvValue::num(0.5)),
                    Condition::new(Var::ReagentB, Operator::Ge, EnvValue::num(0.5)),
                    Condition::new(Var::TemperatureMilliC, Operator::Lt, EnvValue::num(bad)),
                ],
            });
            assert!(
                !matches!(gate.evaluate(&pb, &Environment::new()), GateDecision::Allow),
                "non-finite temperature must not Allow: {bad}"
            );
        }
    }

    #[test]
    fn oversized_dispense_volume_is_aborted() {
        let gate = SafetyGate::with_rules(standard_rules());
        let mut p = Protocol::new("bigdispense");
        let mut conds = common_safe_conditions();
        conds.push(Condition::new(Var::VolumeUl, Operator::Le, EnvValue::num(5_000.0)));
        p.push(ProtocolStep {
            id: "s1".into(),
            action: StepAction::Dispense {
                reagent: "A".into(),
                volume_ul: 5_000,
                target_well: "w1".into(),
            },
            declared_conditions: conds,
        });
        match gate.evaluate(&p, &Environment::new()) {
            GateDecision::Abort { rule_id, .. } => assert_eq!(rule_id, "dispense-volume-limit"),
            other => panic!("oversized dispense must abort, got {other:?}"),
        }
    }

    #[test]
    fn cross_chip_incompatible_pairing_blocked() {
        let gate = SafetyGate::with_rules(standard_rules());
        let mut p = Protocol::new("crosschip");
        let mut conds = common_safe_conditions();
        conds.push(Condition::new(Var::CrossChipIncompatible, Operator::Eq, EnvValue::num(1.0)));
        conds.push(Condition::new(Var::FlowRateUlMin, Operator::Le, EnvValue::num(50.0)));
        p.push(ProtocolStep {
            id: "s1".into(),
            action: StepAction::SetValve {
                valve: "v1".into(),
                open: true,
            },
            declared_conditions: conds,
        });
        match gate.evaluate(&p, &Environment::new()) {
            GateDecision::Abort { rule_id, .. } => assert_eq!(rule_id, "cross-chip-barrier"),
            other => panic!("incompatible cross-chip flow must abort, got {other:?}"),
        }
    }

    /// A well-formed protocol declares the common physical/chemical variables
    /// so that the always-on / cross-cutting rules (reaction temperature,
    /// dispense volume, cross-chip barrier) are *satisfied* rather than failing
    /// closed on a missing variable. Targeted adversarial tests build on top.
    fn common_safe_conditions() -> Vec<Condition> {
        vec![
            Condition::new(Var::ReagentA, Operator::Ge, EnvValue::num(0.5)),
            Condition::new(Var::ReagentB, Operator::Ge, EnvValue::num(0.5)),
            Condition::new(Var::TemperatureMilliC, Operator::Lt, EnvValue::num(3_000.0)),
            Condition::new(Var::VolumeUl, Operator::Le, EnvValue::num(100.0)),
            Condition::new(Var::CrossChipIncompatible, Operator::Eq, EnvValue::num(0.0)),
        ]
    }

    /// A known-safe baseline the autonomous loop might converge to.
    fn safe_baseline() -> Protocol {
        let mut p = Protocol::new("baseline");
        p.push(ProtocolStep {
            id: "s1".into(),
            action: StepAction::Dispense {
                reagent: "A".into(),
                volume_ul: 100,
                target_well: "w1".into(),
            },
            declared_conditions: common_safe_conditions(),
        });
        p
    }

    /// Mutation battery: a single adversarial tweak to a safe protocol must not
    /// silently become Allow.
    #[test]
    fn mutation_battery_stays_safe_or_aborts() {
        let gate = SafetyGate::with_rules(standard_rules());
        assert_eq!(
            gate.evaluate(&safe_baseline(), &Environment::new()),
            GateDecision::Allow
        );

        // Heat the reaction.
        let mut m = safe_baseline();
        m.steps[0].declared_conditions[2] =
            Condition::new(Var::TemperatureMilliC, Operator::Lt, EnvValue::num(25_000.0));
        match gate.evaluate(&m, &Environment::new()) {
            GateDecision::Abort { rule_id, .. } => assert_eq!(rule_id, "rxn-temp-limit"),
            other => panic!("heated reaction must abort, got {other:?}"),
        }

        // Drop the temperature declaration entirely (incomplete protocol).
        let mut m2 = safe_baseline();
        m2.steps[0].declared_conditions.remove(2);
        assert!(matches!(
            gate.evaluate(&m2, &Environment::new()),
            GateDecision::Abort { .. }
        ));

        // Spike the dispense volume past the limit.
        let mut m3 = safe_baseline();
        m3.steps[0].declared_conditions[3] =
            Condition::new(Var::VolumeUl, Operator::Le, EnvValue::num(5_000.0));
        m3.steps[0].action = StepAction::Dispense {
            reagent: "A".into(),
            volume_ul: 5_000,
            target_well: "w1".into(),
        };
        match gate.evaluate(&m3, &Environment::new()) {
            GateDecision::Abort { rule_id, .. } => assert_eq!(rule_id, "dispense-volume-limit"),
            other => panic!("oversized dispense must abort, got {other:?}"),
        }

        // Declare an incompatible cross-chip pairing carrying flow.
        let mut m4 = safe_baseline();
        m4.steps[0].declared_conditions.push(Condition::new(
            Var::CrossChipIncompatible,
            Operator::Eq,
            EnvValue::num(1.0),
        ));
        m4.steps[0].declared_conditions.push(Condition::new(
            Var::FlowRateUlMin,
            Operator::Le,
            EnvValue::num(50.0),
        ));
        match gate.evaluate(&m4, &Environment::new()) {
            GateDecision::Abort { rule_id, .. } => assert_eq!(rule_id, "cross-chip-barrier"),
            other => panic!("incompatible cross-chip flow must abort, got {other:?}"),
        }
    }
}
