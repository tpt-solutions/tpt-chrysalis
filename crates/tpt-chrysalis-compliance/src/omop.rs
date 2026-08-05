//! OMOP CDM mapping (spec.txt §7, Phase 4).
//!
//! Alongside FHIR R5, assay results are harmonized to OMOP CDM `Measurement`
//! rows for analytics warehouses. The mapping is a deterministic code→concept
//! lookup; unknown codes fall back to a generic measurement concept so no data
//! is silently dropped.

use alloc::string::{String, ToString};

use serde::{Deserialize, Serialize};

use crate::fhir::AssayResult;

/// An OMOP CDM `measurement` row (subset of columns).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OmopMeasurement {
    pub measurement_concept_id: i64,
    pub value_as_number: f64,
    pub unit_concept_id: i64,
    pub measurement_datetime: String,
}

/// Maps an assay code to an OMOP measurement concept id.
fn concept_for_code(code: &str) -> i64 {
    match code {
        "TEER" => 4_000_001,
        "VIABILITY" => 4_000_002,
        "CYTOKINE_IL6" => 4_000_003,
        "CYTOKINE_TNF" => 4_000_004,
        _ => 4_000_000, // generic measurement
    }
}

/// Maps a unit string to an OMOP unit concept id.
fn unit_concept(unit: &str) -> i64 {
    match unit {
        "ohm.cm2" => 8_828_100,
        "percent" => 8_558_000,
        "pg/mL" => 8_824_900,
        _ => 0,
    }
}

/// Harmonizes an assay to an OMOP `Measurement` row.
pub fn to_omop_measurement(r: &AssayResult) -> OmopMeasurement {
    OmopMeasurement {
        measurement_concept_id: concept_for_code(&r.code),
        value_as_number: r.value,
        unit_concept_id: unit_concept(&r.unit),
        measurement_datetime: crate::fhir::iso8601(r.effective_ms),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_known_and_unknown_codes() {
        let teer = AssayResult {
            code: "TEER".into(),
            value: 0.9,
            unit: "ohm.cm2".into(),
            subject_id: "s".into(),
            effective_ms: 0,
        };
        let m = to_omop_measurement(&teer);
        assert_eq!(m.measurement_concept_id, 4_000_001);
        assert_eq!(m.unit_concept_id, 8_828_100);

        let unknown = AssayResult {
            code: "MYSTERY".into(),
            value: 1.0,
            unit: "n/a".into(),
            subject_id: "s".into(),
            effective_ms: 0,
        };
        assert_eq!(to_omop_measurement(&unknown).measurement_concept_id, 4_000_000);
    }
}
