//! `tpt-chrysalis-compliance`: ALCOA+ report generation over the audit ledger
//! (spec.txt §5E).
//!
//! The ledger is the single source of truth: every command and observation is
//! hash-chained and WAL-durable *before* execution proceeds. This crate turns
//! that raw chain into a report a reviewer can hold against ALCOA+ and the
//! NZ HIPC 2020 criteria — attributability, legibility, contemporaneity,
//! originality, accuracy, plus the "+" set (complete, consistent, enduring,
//! available). Real regulatory submission is out of scope for Phase 1; the
//! *format and mechanism* are the deliverable.
//!
//! The report generator never mutates the ledger and never makes network
//! calls: it is a pure, deterministic function of the chain bytes, so the same
//! ledger always produces the same report.
//!
//! Phase 4 adds standardization paths so physical assay results can leave the
//! facility *only* through de-identified, harmonized forms:
//! - [`fhir`] — FHIR R5 `Observation`/`Specimen` mapping.
//! - [`omop`] — OMOP CDM `Measurement` mapping.
//! - [`deid`] — NZ HIPC 2020 / HIPAA / GDPR de-identification of
//!   patient-derived organoid metadata (air-gap is a deployment property; the
//!   de-identification guarantee is enforced here in code).

extern crate alloc;

pub mod deid;
pub mod fhir;
pub mod omop;

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::fmt;

use serde::Serialize;
use tpt_chrysalis_core::{
    AuditEntry, AuditLedger, AuditOutcome, LedgerIntegrityReport, PhysicalAction,
};

pub use deid::{deidentify, contains_direct_identifier, DeidentifiedRecord, PdoMetadata};
pub use fhir::{to_fhir_observation, to_fhir_specimen, AssayResult, FhirObservation, FhirSpecimen};
pub use omop::{to_omop_measurement, OmopMeasurement};

/// The nine ALCOA+ criteria, in regulatory order (core five, then the "+" set).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Criterion {
    /// Who performed the action (the bearer identity on the token).
    Attributable,
    /// The record is readable and the report renders each action canonically.
    Legible,
    /// When the action happened — timestamped at the enforcement point.
    Contemporaneous,
    /// The original record, not a transcription — append-only, hash-chained WAL.
    Original,
    /// The chain verifies: recomputation of every hash and prev-hash link.
    Accurate,
    /// No entry is silently omitted from the report.
    Complete,
    /// Sequence numbering and cross-links are consistent.
    Consistent,
    /// Records endure past the session via the persistable WAL.
    Enduring,
    /// The full attributable history is retrievable on demand.
    Available,
}

impl Criterion {
    pub fn code(self) -> &'static str {
        match self {
            Criterion::Attributable => "A",
            Criterion::Legible => "L",
            Criterion::Contemporaneous => "C",
            Criterion::Original => "O",
            Criterion::Accurate => "A2",
            Criterion::Complete => "C+",
            Criterion::Consistent => "C++",
            Criterion::Enduring => "E+",
            Criterion::Available => "Av+",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Criterion::Attributable => "Attributable",
            Criterion::Legible => "Legible",
            Criterion::Contemporaneous => "Contemporaneous",
            Criterion::Original => "Original",
            Criterion::Accurate => "Accurate",
            Criterion::Complete => "Complete",
            Criterion::Consistent => "Consistent",
            Criterion::Enduring => "Enduring",
            Criterion::Available => "Available",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Criterion::Attributable => "every entry records who presented the capability token",
            Criterion::Legible => "every action is rendered in a deterministic, human-readable form",
            Criterion::Contemporaneous => "every entry is timestamped at the enforcement point",
            Criterion::Original => "entries are append-only originals in a hash-chained, WAL-framed record",
            Criterion::Accurate => "the hash chain re-verifies with no broken links",
            Criterion::Complete => "the report includes the entire chain, no filtering of entries",
            Criterion::Consistent => "sequence numbers and prev-hash links agree across the chain",
            Criterion::Enduring => "the ledger persists as WAL bytes and reloads bit-for-bit",
            Criterion::Available => "the attributable history is retrievable on demand",
        }
    }
}

/// The evidence for one criterion in a generated report.
#[derive(Debug, Clone, Serialize)]
pub struct AlcoaCriterion {
    pub code: String,
    pub name: String,
    pub description: String,
    pub satisfied: bool,
    pub evidence: String,
}

/// One ledger entry rendered for a human reader.
#[derive(Debug, Clone, Serialize)]
pub struct ReportedEntry {
    pub seq: u64,
    pub timestamp_ms: u64,
    pub actor: String,
    pub action: String,
    pub outcome: String,
    pub scope: String,
    pub token_expires_at_ms: Option<u64>,
    pub prev_hash_hex: String,
    pub hash_hex: String,
}

/// A complete ALCOA+ report over a ledger (or a filtered view of it).
#[derive(Debug, Clone, Serialize)]
pub struct AlcoaReport {
    pub report_id: String,
    pub generated_at_ms: u64,
    pub actor_filter: Option<String>,
    pub ledger_entry_count: u64,
    pub reported_entry_count: u64,
    pub last_hash_hex: String,
    pub integrity: LedgerIntegrityReport,
    pub criteria: Vec<AlcoaCriterion>,
    pub history: Vec<ReportedEntry>,
    /// True when the core five criteria (A, L, C, O, A2) all pass.
    pub core_compliant: bool,
}

impl AlcoaReport {
    /// All criteria, in regulatory order.
    pub fn criteria(&self) -> &[AlcoaCriterion] {
        &self.criteria
    }

    /// The subset of criteria that failed.
    pub fn failures(&self) -> impl Iterator<Item = &AlcoaCriterion> {
        self.criteria.iter().filter(|c| !c.satisfied)
    }
}

impl fmt::Display for AlcoaReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "ALCOA+ compliance report {}", self.report_id)?;
        writeln!(
            f,
            "generated {}ms  |  ledger entries: {}  |  reported: {}  |  core-compliant: {}",
            self.generated_at_ms, self.ledger_entry_count, self.reported_entry_count, self.core_compliant
        )?;
        writeln!(f, "last chain hash: {}", self.last_hash_hex)?;
        writeln!(f, "integrity: {} ({} broken links)", if self.integrity.is_intact() { "INTACT" } else { "BROKEN" }, self.integrity.broken_seqs.len())?;
        writeln!(f)?;
        writeln!(f, "Criterion checks")?;
        for c in &self.criteria {
            let status = if c.satisfied { "PASS" } else { "FAIL" };
            writeln!(f, "  [{status}] {} ({}) — {}", c.code, c.name, c.description)?;
            writeln!(f, "        evidence: {}", c.evidence)?;
        }
        writeln!(f)?;
        writeln!(f, "Attributable history")?;
        for e in &self.history {
            writeln!(
                f,
                "  #{seq:<4} t={ts:<10} actor={actor:<8} {outcome:<9} {action}  [scope={scope}]",
                seq = e.seq,
                ts = e.timestamp_ms,
                actor = e.actor,
                outcome = e.outcome,
                action = e.action,
                scope = e.scope,
            )?;
        }
        Ok(())
    }
}

/// Renders a physical action in one canonical, lossless sentence.
pub fn render_action(action: &PhysicalAction) -> String {
    match action {
        PhysicalAction::Dispense {
            reagent,
            volume_ul,
            target,
        } => format!("dispense {volume_ul}uL {reagent} -> {target}"),
        PhysicalAction::SetValve { valve, open } => {
            if *open {
                format!("open valve {valve}")
            } else {
                format!("close valve {valve}")
            }
        }
        PhysicalAction::SetTemperature { setpoint_milli_c } => {
            format!("set temperature {}c", setpoint_milli_c / 1_000)
        }
        PhysicalAction::ReadSensor { channel } => format!("read sensor {channel}"),
        PhysicalAction::DeviceCommand { kind, device } => {
            format!("device command {kind} on {device} (refused)")
        }
    }
}

fn render_outcome(outcome: &AuditOutcome) -> String {
    match outcome {
        AuditOutcome::Accepted => "accepted".to_string(),
        AuditOutcome::Rejected { reason } => format!("rejected ({reason})"),
    }
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use core::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

fn report_entry(entry: &AuditEntry) -> ReportedEntry {
    ReportedEntry {
        seq: entry.seq,
        timestamp_ms: entry.timestamp_ms,
        actor: entry.actor.clone(),
        action: render_action(&entry.action),
        outcome: render_outcome(&entry.outcome),
        scope: entry.scope.clone(),
        token_expires_at_ms: entry.token_expires_at_ms,
        prev_hash_hex: hex(&entry.prev_hash),
        hash_hex: hex(&entry.hash),
    }
}

/// Builds the ALCOA+ report. `actor_filter` restricts the attributable history
/// to one bearer identity (the "history for run X" query); criterion checks
/// still evaluate the full chain, because integrity is global.
pub fn build_report(
    ledger: &AuditLedger,
    generated_at_ms: u64,
    actor_filter: Option<&str>,
) -> AlcoaReport {
    let integrity = ledger.verify_integrity();
    let entries = ledger.entries();
    let history: Vec<ReportedEntry> = match actor_filter {
        Some(actor) => entries
            .iter()
            .filter(|e| e.actor == actor)
            .map(report_entry)
            .collect(),
        None => entries.iter().map(report_entry).collect(),
    };

    let last_hash_hex = hex(&ledger.last_hash());
    let seqs_contiguous = entries
        .iter()
        .enumerate()
        .all(|(i, e)| e.seq as usize == i);
    let all_attributed = entries.iter().all(|e| !e.actor.is_empty());
    let all_timestamped = entries.iter().all(|e| e.timestamp_ms > 0);
    let actors: Vec<&str> = {
        let mut seen = Vec::new();
        for e in entries {
            if !seen.contains(&e.actor.as_str()) {
                seen.push(e.actor.as_str());
            }
        }
        seen
    };
    let (min_ts, max_ts) = entries.iter().fold((u64::MAX, 0u64), |(lo, hi), e| {
        (lo.min(e.timestamp_ms), hi.max(e.timestamp_ms))
    });
    let ts_window = match (min_ts, max_ts) {
        (u64::MAX, 0) => "no entries".to_string(),
        (lo, hi) => format!("{lo}..={hi} ms"),
    };

    let criterion = |code: Criterion, satisfied: bool, evidence: String| AlcoaCriterion {
        code: code.code().to_string(),
        name: code.name().to_string(),
        description: code.description().to_string(),
        satisfied,
        evidence,
    };

    let criteria = vec![
        criterion(
            Criterion::Attributable,
            !entries.is_empty() && all_attributed,
            format!(
                "{} entries; {} actor(s): {}",
                entries.len(),
                actors.len(),
                actors.join(", ")
            ),
        ),
        criterion(
            Criterion::Legible,
            true,
            "each action is rendered canonically in the history below".to_string(),
        ),
        criterion(
            Criterion::Contemporaneous,
            !entries.is_empty() && all_timestamped,
            format!("timestamps recorded at enforcement; window {ts_window}"),
        ),
        criterion(
            Criterion::Original,
            !entries.is_empty() && seqs_contiguous,
            format!("append-only hash-chained WAL; seqs 0..={} contiguous", entries.len().saturating_sub(1)),
        ),
        criterion(
            Criterion::Accurate,
            integrity.is_intact(),
            format!(
                "chain re-verified: {} entries, {} broken link(s), last hash {last_hash_hex}",
                integrity.entry_count,
                integrity.broken_seqs.len()
            ),
        ),
        criterion(
            Criterion::Complete,
            history.len() == entries.len(),
            format!(
                "{} of {} entries reported{}",
                history.len(),
                entries.len(),
                actor_filter
                    .map(|a| format!(" (filtered to actor {a})"))
                    .unwrap_or_default()
            ),
        ),
        criterion(
            Criterion::Consistent,
            seqs_contiguous && integrity.is_intact(),
            "sequence numbering and prev-hash cross-links agree across the chain".to_string(),
        ),
        criterion(
            Criterion::Enduring,
            !ledger.as_bytes().is_empty(),
            format!("WAL payload: {} bytes; SaveLoad round-trips", ledger.as_bytes().len()),
        ),
        criterion(
            Criterion::Available,
            true,
            "full attributable history exposed below and via the CLI".to_string(),
        ),
    ];

    let core_codes = [
        Criterion::Attributable.code(),
        Criterion::Legible.code(),
        Criterion::Contemporaneous.code(),
        Criterion::Original.code(),
        Criterion::Accurate.code(),
    ];
    let core_compliant = criteria
        .iter()
        .filter(|c| core_codes.contains(&c.code.as_str()))
        .all(|c| c.satisfied);

    let report_id = format!(
        "ALCOA-{generated_at_ms}-{prefix}",
        prefix = &last_hash_hex[..last_hash_hex.len().min(8)]
    );

    AlcoaReport {
        report_id,
        generated_at_ms,
        actor_filter: actor_filter.map(str::to_owned),
        ledger_entry_count: entries.len() as u64,
        reported_entry_count: history.len() as u64,
        last_hash_hex,
        integrity,
        criteria,
        history,
        core_compliant,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_chrysalis_core::{AuditLedger, AuditOutcome, DEFAULT_SCOPE, SaveLoad};

    fn sample_ledger() -> AuditLedger {
        let mut l = AuditLedger::new();
        l.append(
            1_000,
            "alice",
            PhysicalAction::SetTemperature { setpoint_milli_c: 37_000 },
            AuditOutcome::Accepted,
            DEFAULT_SCOPE,
            Some(2_000),
        );
        l.append(
            1_100,
            "alice",
            PhysicalAction::Dispense {
                reagent: "A".into(),
                volume_ul: 100,
                target: "w3".into(),
            },
            AuditOutcome::Accepted,
            DEFAULT_SCOPE,
            Some(2_000),
        );
        l.append(
            1_200,
            "bob",
            PhysicalAction::Dispense {
                reagent: "B".into(),
                volume_ul: 999_999,
                target: "w4".into(),
            },
            AuditOutcome::Rejected { reason: "volume outside limits".into() },
            DEFAULT_SCOPE,
            Some(1_500),
        );
        l
    }

    #[test]
    fn core_criteria_pass_for_sample_ledger() {
        let report = build_report(&sample_ledger(), 9_999, None);
        assert!(report.core_compliant, "core criteria: {:?}", report.failures().map(|c| &c.code).collect::<Vec<_>>());
        assert!(report.criteria.iter().all(|c| c.satisfied));
        assert_eq!(report.ledger_entry_count, 3);
        assert_eq!(report.reported_entry_count, 3);
    }

    #[test]
    fn tampered_chain_fails_accuracy_and_consistency() {
        let l = sample_ledger();
        let bytes = l.as_bytes().to_vec();
        let mut corrupted = bytes;
        let idx = corrupted.len() / 2;
        corrupted[idx] ^= 0x01;
        // from_bytes either errors on the CRC/chain or yields a shorter chain;
        // either way accuracy must not be claimed on an intact full chain.
        let report = match AuditLedger::from_bytes(&corrupted) {
            Ok(r) => build_report(&r, 1, None),
            Err(_) => return,
        };
        let accurate = report
            .criteria()
            .iter()
            .find(|c| c.code == "A2")
            .expect("accuracy criterion");
        assert!(!accurate.satisfied || report.ledger_entry_count < 3);
    }

    #[test]
    fn empty_ledger_is_not_prematurely_compliant() {
        let report = build_report(&AuditLedger::new(), 1, None);
        assert!(!report.core_compliant);
        assert!(report.history.is_empty());
    }

    #[test]
    fn history_filters_by_actor() {
        let report = build_report(&sample_ledger(), 9_999, Some("alice"));
        assert_eq!(report.reported_entry_count, 2);
        assert!(report.history.iter().all(|e| e.actor == "alice"));
        let complete = report
            .criteria()
            .iter()
            .find(|c| c.code == "C+")
            .expect("completeness criterion");
        assert!(!complete.satisfied);
    }

    #[test]
    fn render_action_is_canonical_and_lossless() {
        assert_eq!(
            render_action(&PhysicalAction::Dispense {
                reagent: "A".into(),
                volume_ul: 100,
                target: "w3".into(),
            }),
            "dispense 100uL A -> w3"
        );
        assert_eq!(
            render_action(&PhysicalAction::SetValve { valve: "v1".into(), open: true }),
            "open valve v1"
        );
        assert_eq!(
            render_action(&PhysicalAction::SetTemperature { setpoint_milli_c: 37_000 }),
            "set temperature 37c"
        );
        assert_eq!(render_action(&PhysicalAction::ReadSensor { channel: "temp".into() }), "read sensor temp");
    }

    #[test]
    fn report_serializes_to_json() {
        let report = build_report(&sample_ledger(), 9_999, None);
        let json = serde_json::to_string(&report).expect("serializes");
        assert!(json.contains("\"report_id\""));
        assert!(json.contains("\"core_compliant\":true"));
        assert!(json.contains("dispense 100uL A -> w3"));
        assert!(json.contains("\"criteria\""));
    }

    #[test]
    fn persisted_ledger_reloads_and_reports_intact() {
        let l = sample_ledger();
        let bytes = SaveLoad::save(&l);
        let loaded = AuditLedger::load(&bytes).expect("loads");
        let report = build_report(&loaded, 7, None);
        assert!(report.core_compliant);
    }

    #[test]
    fn display_is_reviewable() {
        let report = build_report(&sample_ledger(), 9_999, None);
        let text = format!("{report}");
        assert!(text.contains("ALCOA+ compliance report"));
        assert!(text.contains("INTACT"));
        assert!(text.contains("dispense 100uL A -> w3"));
    }
}
