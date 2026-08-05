//! FHIR R5 harmonization (spec.txt §7, Phase 4).
//!
//! Maps physical assay results (cytokine levels, TEER, viability) to FHIR R5
//! `Observation` / `Specimen` resources so downstream Digital-Twin systems
//! (`tpt-soma`) can ingest them. The mapping is a pure, deterministic function
//! of an [`AssayResult`] — no network, no mutable global state — matching the
//! additive, auditable philosophy of the rest of the crate.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

/// A physical assay measurement produced by the (simulated) hardware.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssayResult {
    /// LOINC-style assay code, e.g. "TEER", "CYTOKINE_IL6".
    pub code: String,
    /// Numeric value in `unit`.
    pub value: f64,
    /// Unit of `value`.
    pub unit: String,
    /// Subject/run the sample belongs to (a pseudonym after de-identification).
    pub subject_id: String,
    /// When the assay was taken, epoch milliseconds.
    pub effective_ms: u64,
}

/// A FHIR R5 `Observation` resource (subset of fields).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FhirObservation {
    pub resourceType: String,
    pub id: String,
    pub status: String,
    pub code: FhirCodeableConcept,
    pub subject: FhirReference,
    pub effectiveDateTime: String,
    pub valueQuantity: FhirQuantity,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FhirCodeableConcept {
    pub coding: Vec<FhirCoding>,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FhirCoding {
    pub system: String,
    pub code: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FhirReference {
    pub reference: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FhirQuantity {
    pub value: f64,
    pub unit: String,
}

/// A FHIR R5 `Specimen` resource (subset).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FhirSpecimen {
    pub resourceType: String,
    pub id: String,
    pub r#type: FhirCodeableConcept,
    pub subject: FhirReference,
}

/// Maps an assay to a FHIR R5 `Observation`. Deterministic given the input.
pub fn to_fhir_observation(r: &AssayResult) -> FhirObservation {
    FhirObservation {
        resourceType: "Observation".into(),
        id: format!("obs-{}", r.code.to_lowercase()),
        status: "final".into(),
        code: FhirCodeableConcept {
            coding: vec![FhirCoding {
                system: "http://loinc.org".into(),
                code: r.code.clone(),
            }],
            text: r.code.clone(),
        },
        subject: FhirReference {
            reference: format!("Patient/{}", r.subject_id),
        },
        effectiveDateTime: iso8601(r.effective_ms),
        valueQuantity: FhirQuantity {
            value: r.value,
            unit: r.unit.clone(),
        },
    }
}

/// Maps an assay to a FHIR R5 `Specimen` for its subject.
pub fn to_fhir_specimen(r: &AssayResult) -> FhirSpecimen {
    FhirSpecimen {
        resourceType: "Specimen".into(),
        id: format!("specimen-{}", r.subject_id),
        r#type: FhirCodeableConcept {
            coding: vec![FhirCoding {
                system: "http://snomed.info/sct".into(),
                code: "122555007".into(),
            }],
            text: "patient-derived organoid sample".into(),
        },
        subject: FhirReference {
            reference: format!("Patient/{}", r.subject_id),
        },
    }
}

/// Formats epoch milliseconds as an RFC 3339 / FHIR `dateTime` (`Z` suffix).
pub fn iso8601(ms: u64) -> String {
    const MS_PER_DAY: u64 = 86_400_000;
    let days = ms / MS_PER_DAY;
    let rem = ms % MS_PER_DAY;
    let (h, m, s) = (
        rem / 3_600_000,
        (rem % 3_600_000) / 60_000,
        (rem % 60_000) / 1000,
    );
    let (y, mo, d) = days_to_ymd(days as i64);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}Z")
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

fn days_to_ymd(mut days: i64) -> (i64, u32, u32) {
    let mut y = 1970;
    loop {
        let len = if is_leap(y) { 366 } else { 365 };
        if days < len {
            break;
        }
        days -= len;
        y += 1;
    }
    let month_lens = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut mo = 0usize;
    let mut rem = days;
    loop {
        let ml = month_lens[mo] + if mo == 1 && is_leap(y) { 1 } else { 0 };
        if rem < ml {
            break;
        }
        rem -= ml;
        mo += 1;
    }
    (y, (mo + 1) as u32, (rem + 1) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_observation_and_specimen() {
        let r = AssayResult {
            code: "TEER".into(),
            value: 0.85,
            unit: "ohm.cm2".into(),
            subject_id: "subj-7".into(),
            effective_ms: 1_700_000_000_000,
        };
        let obs = to_fhir_observation(&r);
        assert_eq!(obs.resourceType, "Observation");
        assert_eq!(obs.status, "final");
        assert_eq!(obs.subject.reference, "Patient/subj-7");
        assert_eq!(obs.valueQuantity.value, 0.85);
        let spec = to_fhir_specimen(&r);
        assert_eq!(spec.resourceType, "Specimen");
        assert_eq!(spec.subject.reference, "Patient/subj-7");
    }

    #[test]
    fn iso8601_is_deterministic_and_well_formed() {
        assert_eq!(iso8601(0), "1970-01-01T00:00:00Z");
        // 2023-11-14T22:13:20Z ~= 1_700_000_000_000 ms
        let s = iso8601(1_700_000_000_000);
        assert!(s.starts_with("2023-11-"));
        assert!(s.ends_with("Z"));
    }
}
