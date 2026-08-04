//! The typed rule language: preconditions over physical/chemical variables.

use alloc::vec::Vec;

use crate::environment::{EnvValue, Var, cmp};

/// Comparison operators available in conditions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operator {
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
}

/// One typed comparison: `var <op> rhs`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Condition {
    pub var: Var,
    pub op: Operator,
    pub rhs: EnvValue,
}

impl Condition {
    pub const fn new(var: Var, op: Operator, rhs: EnvValue) -> Self {
        Self { var, op, rhs }
    }

    /// Evaluates against an environment. Returns `None` when the variable is
    /// absent — callers (the gate) treat that as fail-closed.
    pub fn evaluate(&self, env: &crate::environment::Environment) -> Option<bool> {
        env.get(self.var).map(|lhs| cmp(self.op, lhs.0, self.rhs.0))
    }
}

/// What the gate does when a rule's requirement fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureAction {
    /// Execution is blocked. This is the default for physical-safety rules.
    Abort,
    /// Record a warning only. Use for non-safety constraints; the gate still
    /// reports it, but execution may continue.
    Alert,
}

/// A hard constraint: `IF all preconditions THEN requirement MUST hold ELSE on_fail`.
///
/// This mirrors spec.txt §4 Layer 3's example directly:
/// `IF reagent_A AND reagent_B THEN temperature MUST BE < 4°C ELSE abort`.
#[derive(Debug, Clone, PartialEq)]
pub struct Rule {
    pub id: alloc::string::String,
    /// All preconditions must evaluate true for the rule to fire.
    pub preconditions: Vec<Condition>,
    /// The requirement that must hold once the rule fires.
    pub requirement: Condition,
    /// What happens if the requirement fails.
    pub on_fail: FailureAction,
}

/// The outcome of checking one rule against an environment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleCheck {
    /// Preconditions were not all true; the rule does not apply.
    NotFired,
    /// The rule fired and its requirement holds.
    Satisfied,
    /// The rule fired and its requirement failed.
    Violated,
    /// A variable referenced by the rule is unknown to the environment.
    /// The gate treats this as fail-closed (blocked).
    Unknown,
}

impl Rule {
    /// Checks the rule against `env`, distinguishing "not fired" from
    /// "fired and violated" (the gate must not conflate the two).
    pub fn check(&self, env: &crate::environment::Environment) -> RuleCheck {
        for p in &self.preconditions {
            match p.evaluate(env) {
                Some(true) => {}
                Some(false) => return RuleCheck::NotFired,
                None => return RuleCheck::Unknown,
            }
        }
        match self.requirement.evaluate(env) {
            Some(true) => RuleCheck::Satisfied,
            Some(false) => RuleCheck::Violated,
            None => RuleCheck::Unknown,
        }
    }
}

/// An ordered set of rules. Order matters only for reporting; every rule is
/// evaluated independently and any `Abort` decision blocks execution.
#[derive(Debug, Clone, Default)]
pub struct RuleSet {
    pub rules: Vec<Rule>,
}

impl RuleSet {
    pub fn new() -> Self {
        Self { rules: Vec::new() }
    }

    pub fn add(&mut self, rule: Rule) -> &mut Self {
        self.rules.push(rule);
        self
    }

    pub fn iter(&self) -> impl Iterator<Item = &Rule> {
        self.rules.iter()
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }
}
