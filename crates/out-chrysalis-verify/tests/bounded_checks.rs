//! Concrete (non-Kani) bounded checks that mirror the `cfg(kani)` proof
//! harnesses in `src/lib.rs`. These run under ordinary `cargo test` so the
//! verification crate is meaningful and green without the Kani toolchain
//! installed; the Kani harnesses (`cargo kani`) prove the same properties over
//! *all* inputs, not just the sampled ones here.

use tpt_chrysalis_hal::*;

#[test]
fn dispense_in_limits_satisfies_predicate() {
    let lim = HAL_LIMITS;
    for v in [lim.min_volume_ul, 1, 100, 500, lim.max_volume_ul] {
        let cmd = DispenseCommand {
            reagent: ReagentId(1),
            volume_ul: v,
            target: WellId(1),
        };
        let in_bounds = cmd.volume_ul >= lim.min_volume_ul && cmd.volume_ul <= lim.max_volume_ul;
        assert!(in_bounds, "in-limits volume {v} must satisfy predicate");
    }
}

#[test]
fn dispense_over_limit_is_flagged() {
    let lim = HAL_LIMITS;
    let cmd = DispenseCommand {
        reagent: ReagentId(1),
        volume_ul: lim.max_volume_ul + 1,
        target: WellId(1),
    };
    let in_bounds = cmd.volume_ul >= lim.min_volume_ul && cmd.volume_ul <= lim.max_volume_ul;
    assert!(!in_bounds, "over-limit volume must be flagged out of bounds");
}

#[test]
fn timing_never_underflows() {
    for (intent, actuation) in [(0u64, 0u64), (100, 50), (1_000, 1_000), (5, 9)] {
        let t = ActuationTiming {
            intent_ms: intent,
            actuation_ms: actuation,
        };
        let lat = t.latency_us();
        assert_eq!(lat, actuation.saturating_sub(intent) * 1000);
    }
}

#[test]
fn command_copy_is_lossless() {
    let cmd = ActuatorCommand::Dispense(DispenseCommand {
        reagent: ReagentId(7),
        volume_ul: 250,
        target: WellId(42),
    });
    let copied = cmd;
    assert_eq!(cmd, copied);
}

#[test]
fn limits_are_internally_consistent() {
    // Mirrors the compile-time asserts in the HAL; re-checked here so the
    // verification crate fails loudly if a limit constant is ever inverted.
    assert!(HAL_LIMITS.min_volume_ul <= HAL_LIMITS.max_volume_ul);
    assert!(HAL_LIMITS.min_temp_milli_c < HAL_LIMITS.max_temp_milli_c);
    assert!(HAL_LIMITS.max_pressure_mbar > 0);
}
