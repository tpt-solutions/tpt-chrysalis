//! End-to-end wiring: intent → protocol compiler → safety gate →
//! capability-gated simulated actuation → audit ledger → ALCOA+ compliance
//! report that reflects the run (TODO.md Phase 1 "full end-to-end test").
//!
//! The orchestrator is exercised through its public API only; the compliance
//! crate reads the exact ledger the orchestrator produced.

use tpt_chrysalis_compliance::build_report;
use tpt_chrysalis_core::DEFAULT_SCOPE;
use tpt_chrysalis_orchestrator::compiler::build_intent;
use tpt_chrysalis_orchestrator::{OrchestrationError, Orchestrator};
use tpt_chrysalis_safety::environment::{EnvValue, Var};
use tpt_chrysalis_safety::gate::SafetyGate;
use tpt_chrysalis_safety::rule::{Condition, FailureAction, Operator, Rule};
use tpt_chrysalis_safety::Environment;
use tpt_chrysalis_sim::model::{SimConfig, SimLiquidHandler, TickClock};
use tpt_chrysalis_sim::sensor::{SimSensorBank, SimSensorConfig};

fn rig() -> (SimLiquidHandler<TickClock>, SimSensorBank<TickClock>, TickClock) {
    let clock = TickClock::new();
    let device = SimLiquidHandler::new(SimConfig::default(), clock);
    let state = device.shared_state();
    let sensors = SimSensorBank::new(
        SimSensorConfig { noise: 0.0, seed: 2 },
        state,
        clock,
    );
    (device, sensors, clock)
}

/// Both reagents present; the reaction must stay below 4°C.
fn gate() -> SafetyGate {
    SafetyGate::new()
        .add_rule(Rule {
            id: "rxn-temp-limit".into(),
            preconditions: vec![
                Condition::new(Var::ReagentA, Operator::Ge, EnvValue::num(0.5)),
                Condition::new(Var::ReagentB, Operator::Ge, EnvValue::num(0.5)),
            ],
            requirement: Condition::new(
                Var::TemperatureMilliC,
                Operator::Lt,
                EnvValue::num(4_000.0),
            ),
            on_fail: FailureAction::Abort,
        })
        .clone()
}

/// 2°C: passes the temperature gate.
fn cold_env() -> Environment {
    Environment::new()
        .set(Var::ReagentA, EnvValue::present())
        .set(Var::ReagentB, EnvValue::present())
        .set(Var::TemperatureMilliC, EnvValue::num(2_000.0))
        .set(Var::VolumeUl, EnvValue::num(100.0))
        .clone()
}

/// 37°C: the temperature gate aborts.
fn hot_env() -> Environment {
    let mut e = cold_env();
    e.set(Var::TemperatureMilliC, EnvValue::num(37_000.0));
    e
}

#[test]
fn successful_run_is_fully_attributed_in_compliance_report() {
    let (device, sensors, clock) = rig();
    let mut o = Orchestrator::new(DEFAULT_SCOPE, device, sensors, clock, gate(), 5_000).unwrap();
    let intent = build_intent("i1", "A", "w1", 100, Some(2_000));
    let exec = o.submit_intent(&intent, &cold_env(), "alice", 1_000).unwrap();
    assert_eq!(exec.step_count, 2);

    // The ledger the orchestrator produced must yield a core-compliant report.
    let report = build_report(o.ledger(), 2_000, None);
    assert!(
        report.core_compliant,
        "failed criteria: {:?}",
        report.failures().map(|c| &c.code).collect::<Vec<_>>()
    );
    assert!(report.integrity.is_intact());

    // 2 actuator steps + 4 sensor reads, every one accepted and attributed
    // to the token bearer.
    assert_eq!(report.ledger_entry_count, 6);
    assert_eq!(report.reported_entry_count, 6);
    assert!(report.history.iter().all(|e| e.outcome == "accepted"));
    assert!(report.history.iter().all(|e| e.actor == "alice"));
    assert!(report
        .history
        .iter()
        .any(|e| e.action == "dispense 100uL A -> w1"));
    assert!(report
        .history
        .iter()
        .any(|e| e.action == "set temperature 2c"));
}

#[test]
fn safety_blocked_run_is_recorded_and_reflected_in_report() {
    let (device, sensors, clock) = rig();
    let mut o = Orchestrator::new(DEFAULT_SCOPE, device, sensors, clock, gate(), 5_000).unwrap();
    let intent = build_intent("i1", "A", "w1", 100, Some(37_000));
    let err = o
        .submit_intent(&intent, &hot_env(), "bob", 1_000)
        .expect_err("hot run must be blocked");
    assert!(matches!(err, OrchestrationError::SafetyBlocked { .. }));

    // The block is itself a record: attributable, timestamped, hash-chained,
    // and the compliance report reflects that nothing was executed.
    let report = build_report(o.ledger(), 2_000, None);
    assert!(report.integrity.is_intact());
    assert_eq!(report.ledger_entry_count, 1);
    assert!(report
        .history
        .iter()
        .any(|e| e.actor == "bob" && e.outcome.contains("rejected")));
    assert!(report
        .history
        .iter()
        .all(|e| !e.outcome.starts_with("accepted")));
}

#[test]
fn tampered_ledger_after_a_run_never_reports_core_compliant() {
    let (device, sensors, clock) = rig();
    let mut o = Orchestrator::new(DEFAULT_SCOPE, device, sensors, clock, gate(), 5_000).unwrap();
    let intent = build_intent("i1", "A", "w1", 100, Some(2_000));
    o.submit_intent(&intent, &cold_env(), "alice", 1_000).unwrap();

    // Persist, tamper, reload: the report must not claim accuracy on a broken
    // chain (it either refuses to load or reports the shorter prefix).
    use tpt_chrysalis_core::SaveLoad;
    let mut bytes = SaveLoad::save(o.ledger());
    let idx = bytes.len() - 4;
    bytes[idx] ^= 0x01;
    match tpt_chrysalis_core::AuditLedger::from_bytes(&bytes) {
        Ok(truncated) => {
            let report = build_report(&truncated, 2_000, None);
            assert!(report.ledger_entry_count < 6);
        }
        Err(tpt_chrysalis_core::LedgerError::ChainViolation { .. }) => {}
        Err(e) => panic!("unexpected ledger error: {e}"),
    }
}
