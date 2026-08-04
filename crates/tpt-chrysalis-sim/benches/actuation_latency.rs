//! Deterministic-timing benchmark baseline: "valve open intent → actuation"
//! latency through the simulated HAL path (see TODO Phase 1 and ADR 0003).
//!
//! The number produced here is the reference point for later real-hardware /
//! FPGA comparison. It is a *baseline*, not a target: when real drivers land,
//! the same harness measures them and the ratio is reported.

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use tpt_chrysalis_hal::{ActuatorCommand, Clock, HalDevice, SimInstant, ValveCommand, ValveId};
use tpt_chrysalis_sim::{SimConfig, SimLiquidHandler};

struct TimedActuation {
    intent_ms: u64,
    actuation_ms: u64,
}

fn run_cycle() -> TimedActuation {
    let clock = SimInstant;
    let mut handler = SimLiquidHandler::new(SimConfig::default(), clock);
    let intent_ms = clock.now_ms();
    let outcome = handler
        .execute(ActuatorCommand::SetValve(ValveCommand {
            valve: ValveId(1),
            open: true,
        }))
        .expect("sim execute");
    black_box(outcome);
    let actuation_ms = clock.now_ms();
    TimedActuation {
        intent_ms,
        actuation_ms,
    }
}

fn bench_actuation_latency(c: &mut Criterion) {
    c.bench_function("valve_open_intent_to_actuation", |b| {
        b.iter(|| {
            let t = run_cycle();
            black_box((t.intent_ms, t.actuation_ms));
        })
    });
}

criterion_group!(benches, bench_actuation_latency);
criterion_main!(benches);
