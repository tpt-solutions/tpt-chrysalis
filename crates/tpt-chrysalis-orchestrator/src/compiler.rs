//! Protocol compilation: `IntentNode` → [`Protocol`] (safety-evaluable).

use alloc::string::String;

use tpt_chrysalis_core::{IntentNode, IntentStatus};
use tpt_chrysalis_hal::HAL_LIMITS;
use tpt_chrysalis_safety::environment::{EnvValue, Var};
use tpt_chrysalis_safety::protocol::{Protocol, ProtocolStep, StepAction};
use tpt_chrysalis_safety::rule::{Condition, Operator};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompileError {
    /// The intent has no reagent.
    MissingReagent,
    /// The intent has no target well.
    MissingTarget,
    /// The requested volume is missing or outside HAL limits.
    VolumeOutOfLimits { volume_ul: u32 },
    /// The requested temperature is outside HAL limits.
    TemperatureOutOfLimits { setpoint_milli_c: i64 },
    /// The intent is not in a state that may be compiled.
    NotCompilable(IntentStatus),
    /// The compiled step could not be mapped to a capability channel.
    UnsupportedStep(String),
}

impl core::fmt::Display for CompileError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}

/// Turns a Digital Intent into a linear [`Protocol`] the safety gate can
/// evaluate *before* anything reaches the HAL.
#[derive(Debug, Clone, Copy, Default)]
pub struct ProtocolCompiler;

impl ProtocolCompiler {
    pub fn compile(&self, intent: &IntentNode) -> Result<Protocol, CompileError> {
        if intent.status != IntentStatus::Pending {
            return Err(CompileError::NotCompilable(intent.status));
        }
        let reagent = intent
            .reagent
            .clone()
            .ok_or(CompileError::MissingReagent)?;
        let target = intent
            .target_well
            .clone()
            .ok_or(CompileError::MissingTarget)?;
        let volume_ul = intent
            .parameters
            .get("volume_ul")
            .copied()
            .ok_or(CompileError::VolumeOutOfLimits { volume_ul: 0 })? as u32;
        if !(HAL_LIMITS.min_volume_ul..=HAL_LIMITS.max_volume_ul).contains(&volume_ul) {
            return Err(CompileError::VolumeOutOfLimits { volume_ul });
        }

        let mut protocol = Protocol::new(format!("protocol-{}", intent.id.0));

        // 1. Temperature precondition (only when the intent specifies one).
        if let Some(&temp) = intent.parameters.get("temperature_milli_c") {
            let setpoint = temp as i64;
            if !(HAL_LIMITS.min_temp_milli_c..=HAL_LIMITS.max_temp_milli_c).contains(&setpoint) {
                return Err(CompileError::TemperatureOutOfLimits {
                    setpoint_milli_c: setpoint,
                });
            }
            protocol.push(ProtocolStep {
                id: format!("{}-temp", intent.id.0),
                action: StepAction::SetTemperature { setpoint_milli_c: setpoint },
                declared_conditions: vec![Condition::new(
                    Var::TemperatureMilliC,
                    Operator::Le,
                    EnvValue::num(setpoint as f64),
                )],
            });
        }

        // 2. The dispense itself.
        let declared = vec![
            Condition::new(Var::ReagentA, Operator::Ge, EnvValue::num(0.0)),
            Condition::new(Var::VolumeUl, Operator::Ge, EnvValue::num(1.0)),
        ];
        protocol.push(ProtocolStep {
            id: format!("{}-dispense", intent.id.0),
            action: StepAction::Dispense {
                reagent,
                volume_ul,
                target_well: target,
            },
            declared_conditions: declared,
        });

        Ok(protocol)
    }
}

/// Convenience for building an intent in tests/tools.
pub fn build_intent(
    id: &str,
    reagent: &str,
    target_well: &str,
    volume_ul: u32,
    temperature_milli_c: Option<i64>,
) -> IntentNode {
    let mut parameters = alloc::collections::BTreeMap::new();
    parameters.insert("volume_ul".into(), volume_ul as f64);
    if let Some(t) = temperature_milli_c {
        parameters.insert("temperature_milli_c".into(), t as f64);
    }
    IntentNode {
        id: tpt_chrysalis_core::NodeId(id.into()),
        intent: format!("Dispense {volume_ul}uL of {reagent} into {target_well}"),
        reagent: Some(reagent.into()),
        target_well: Some(target_well.into()),
        parameters,
        status: IntentStatus::Pending,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compiles_simple_intent() {
        let c = ProtocolCompiler;
        let intent = build_intent("i1", "A", "w3", 100, None);
        let p = c.compile(&intent).unwrap();
        assert_eq!(p.steps.len(), 1);
        assert!(matches!(
            p.steps[0].action,
            StepAction::Dispense {
                ref reagent,
                volume_ul: 100,
                ref target_well,
            } if reagent == "A" && target_well == "w3"
        ));
    }

    #[test]
    fn compiles_with_temperature_step_first() {
        let c = ProtocolCompiler;
        let intent = build_intent("i2", "B", "w4", 50, Some(37_000));
        let p = c.compile(&intent).unwrap();
        assert_eq!(p.steps.len(), 2);
        assert!(matches!(
            p.steps[0].action,
            StepAction::SetTemperature { setpoint_milli_c: 37_000 }
        ));
    }

    #[test]
    fn rejects_volume_outside_limits() {
        let c = ProtocolCompiler;
        let intent = build_intent("i3", "A", "w1", HAL_LIMITS.max_volume_ul + 1, None);
        assert!(matches!(
            c.compile(&intent),
            Err(CompileError::VolumeOutOfLimits { .. })
        ));
    }

    #[test]
    fn rejects_non_pending_intent() {
        let c = ProtocolCompiler;
        let mut intent = build_intent("i4", "A", "w1", 10, None);
        intent.status = IntentStatus::Executing;
        assert_eq!(c.compile(&intent), Err(CompileError::NotCompilable(IntentStatus::Executing)));
    }
}
