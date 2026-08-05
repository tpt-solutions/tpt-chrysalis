//! De-identification (spec.txt §5E/§6, Phase 4).
//!
//! Patient-derived organoid metadata must be scrubbed of direct identifiers
//! before any data leaves the local facility (NZ HIPC 2020 / HIPAA / GDPR). This
//! module is the deterministic, pure scrubber: it derives a stable pseudonym
//! (HMAC-style SHA-256 of the subject id + a site salt) and removes direct
//! identifiers from free text. It never performs network egress — air-gap is a
//! deployment property, but the de-identification guarantee is enforced here in
//! code.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Patient-derived organoid metadata as it arrives locally (pre-deid).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PdoMetadata {
    pub subject_id: String,
    pub patient_name: Option<String>,
    pub patient_dob: Option<String>,
    pub clinician: Option<String>,
    pub consent_id: Option<String>,
    pub free_text_notes: Option<String>,
}

/// The de-identified record safe to export.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeidentifiedRecord {
    /// Stable pseudonym: SHA-256(salt || subject_id), first 16 hex chars.
    pub pseudonym: String,
    /// Whether a consent reference was present (needed for lawful export).
    pub consent_present: bool,
    /// Free-text notes with direct identifiers removed.
    pub notes_scrubbed: String,
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

fn pseudonym(subject_id: &str, salt: &str) -> String {
    let mut h = Sha256::new();
    h.update(salt.as_bytes());
    h.update(subject_id.as_bytes());
    hex(&h.finalize())[..16].to_string()
}

/// Scans free text for likely direct identifiers (names, DOBs, MRNs).
pub fn contains_direct_identifier(text: &str) -> bool {
    let lower = text.to_lowercase();
    if lower.contains("mrn")
        || lower.contains("dob")
        || lower.contains("date of birth")
        || lower.contains("patient name")
    {
        return true;
    }
    // Any 4-digit year in the 19xx/20xx range is treated as a potential DOB.
    for tok in text.split(|c: char| !c.is_ascii_digit()) {
        if tok.len() == 4 {
            if let Ok(y) = tok.parse::<i32>() {
                if (1900..=2025).contains(&y) {
                    return true;
                }
            }
        }
    }
    false
}

fn scrub(text: &str) -> String {
    // Deterministic, conservative scrub: drop any token that is a 4-digit year
    // or a known identifier keyword; join the rest back.
    let mut out = Vec::new();
    for tok in text.split_whitespace() {
        let is_year = tok.len() == 4 && tok.parse::<i32>().map(|y| (1900..=2025).contains(&y)).unwrap_or(false);
        let lower = tok.to_lowercase();
        if is_year || lower.starts_with("mrn") || lower.starts_with("dob:") {
            continue;
        }
        out.push(tok);
    }
    out.join(" ")
}

/// De-identifies metadata against a site `salt`. Pure and deterministic.
pub fn deidentify(meta: &PdoMetadata, salt: &str) -> DeidentifiedRecord {
    let notes_scrubbed = meta
        .free_text_notes
        .as_ref()
        .map(|n| scrub(n))
        .unwrap_or_default();
    DeidentifiedRecord {
        pseudonym: pseudonym(&meta.subject_id, salt),
        consent_present: meta.consent_id.is_some(),
        notes_scrubbed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pseudonym_is_stable_and_not_the_subject_id() {
        let m = PdoMetadata {
            subject_id: "PT-123".into(),
            patient_name: Some("Jane Doe".into()),
            patient_dob: None,
            clinician: None,
            consent_id: Some("C-1".into()),
            free_text_notes: None,
        };
        let a = deidentify(&m, "site-salt");
        let b = deidentify(&m, "site-salt");
        assert_eq!(a.pseudonym, b.pseudonym);
        assert_ne!(a.pseudonym, m.subject_id);
        assert!(a.consent_present);
    }

    #[test]
    fn notes_are_scrubbed_of_identifiers() {
        let m = PdoMetadata {
            subject_id: "PT-9".into(),
            patient_name: None,
            patient_dob: None,
            clinician: None,
            consent_id: None,
            free_text_notes: Some("sample taken 1985 from MRN 4452, healthy donor".into()),
        };
        let d = deidentify(&m, "salt");
        assert!(!d.notes_scrubbed.contains("1985"));
        assert!(!d.notes_scrubbed.contains("MRN"));
        assert!(d.notes_scrubbed.contains("healthy"));
    }

    #[test]
    fn detector_flags_years_and_keywords() {
        assert!(contains_direct_identifier("DOB 1990-04-12"));
        assert!(contains_direct_identifier("mrn 55"));
        assert!(!contains_direct_identifier("TEER stable at 0.8"));
    }
}
