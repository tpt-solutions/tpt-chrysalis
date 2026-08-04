//! The protocol representation the gate symbolically evaluates before the
//! orchestrator compiles anything to the HAL.

use alloc::string::String;
use alloc::vec::Vec;

use crate::environment::Var;
use crate::rule::Condition;

/// A typed action a step asks the HAL to perform.
#[derive(Debug, Clone, PartialEq)]
pub enum StepAction {
    Dispense {
        reagent: String,
        volume_ul: u32,
        target_well: String,
    },
    SetValve {
        valve: String,
        open: bool,
    },
    SetTemperature {
        setpoint_milli_c: i64,
    },
}

impl StepAction {
    /// The canonical action name used to map to capability resources.
    pub fn kind(&self) -> &'static str {
        match self {
            StepAction::Dispense { .. } => "dispense",
            StepAction::SetValve { .. } => "open_valve",
            StepAction::SetTemperature { .. } => "set_temperature",
        }
    }
}

/// One step of a protocol: the action plus the conditions it declares must
/// hold (used as the symbolic environment for gate evaluation).
#[derive(Debug, Clone, PartialEq)]
pub struct ProtocolStep {
    pub id: String,
    pub action: StepAction,
    /// Conditions this step declares will hold at execution time. Derived from
    /// the intent; the gate evaluates rules against them.
    pub declared_conditions: Vec<Condition>,
}

impl ProtocolStep {
    /// The variables this step's declared conditions reference.
    pub fn referenced_vars(&self) -> Vec<Var> {
        self.declared_conditions
            .iter()
            .map(|c| c.var)
            .collect::<Vec<_>>()
    }
}

/// A full experimental protocol: an ordered list of steps.
#[derive(Debug, Clone, PartialEq)]
pub struct Protocol {
    pub id: String,
    pub steps: Vec<ProtocolStep>,
}

impl Protocol {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            steps: Vec::new(),
        }
    }

    pub fn push(&mut self, step: ProtocolStep) {
        self.steps.push(step);
    }

    /// Builder-style push for ergonomic test/DSL construction.
    pub fn with_step(mut self, step: ProtocolStep) -> Self {
        self.push(step);
        self
    }

    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// Combines all steps' declared conditions into one environment. A step
    /// that declares nothing contributes nothing.
    pub fn declared_environment(&self) -> crate::environment::Environment {
        let mut env = crate::environment::Environment::new();
        for step in &self.steps {
            for c in &step.declared_conditions {
                env.set(c.var, c.rhs);
            }
        }
        env
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::environment::EnvValue;
    use alloc::vec;

    #[test]
    fn declared_environment_merges_steps() {
        let p = Protocol::new("p1").with_step(ProtocolStep {
            id: "s1".into(),
            action: StepAction::SetTemperature {
                setpoint_milli_c: 3_000,
            },
            declared_conditions: vec![Condition::new(
                Var::TemperatureMilliC,
                crate::rule::Operator::Lt,
                EnvValue::num(4_000.0),
            )],
        });
        assert_eq!(
            p.declared_environment().get(Var::TemperatureMilliC),
            Some(EnvValue::num(4_000.0))
        );
    }
}
