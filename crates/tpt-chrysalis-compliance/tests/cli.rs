//! End-to-end CLI checks: persist a ledger, invoke the real binary, and
//! confirm the ALCOA+ report and attributable-history subcommands behave.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use tpt_chrysalis_core::{
    AuditLedger, AuditOutcome, PhysicalAction, SaveLoad, DEFAULT_SCOPE,
};

fn sample_ledger() -> AuditLedger {
    let mut l = AuditLedger::new();
    l.append(
        1_000,
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
        1_100,
        "bob",
        PhysicalAction::Dispense {
            reagent: "B".into(),
            volume_ul: 999_999,
            target: "w4".into(),
        },
        AuditOutcome::Rejected {
            reason: "volume outside limits".into(),
        },
        DEFAULT_SCOPE,
        Some(1_500),
    );
    l
}

fn ledger_file(tag: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!("tpt-chrysalis-cli-{tag}-{}.bin", std::process::id()));
    fs::write(&path, SaveLoad::save(&sample_ledger())).expect("writes ledger bytes");
    path
}

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_tpt-chrysalis-compliance")
}

#[test]
fn cli_report_prints_compliance_summary() {
    let path = ledger_file("report");
    let out = Command::new(bin())
        .arg("report")
        .arg(&path)
        .output()
        .expect("binary runs");
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(
        out.status.success(),
        "exit {:?}: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(stdout.contains("ALCOA+ compliance report"));
    assert!(stdout.contains("core-compliant: true"));
    assert!(stdout.contains("INTACT"));
    assert!(stdout.contains("dispense 100uL A -> w3"));
}

#[test]
fn cli_history_filters_by_actor() {
    let path = ledger_file("history");
    let out = Command::new(bin())
        .arg("history")
        .arg(&path)
        .arg("--actor")
        .arg("alice")
        .output()
        .expect("binary runs");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(stdout.contains("alice"));
    assert!(stdout.contains("dispense 100uL A -> w3"));
    assert!(!stdout.contains("bob"));
}

#[test]
fn cli_json_report_serializes() {
    let path = ledger_file("json");
    let out = Command::new(bin())
        .arg("report")
        .arg(&path)
        .arg("--json")
        .output()
        .expect("binary runs");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(stdout.contains("\"report_id\""));
    assert!(stdout.contains("\"core_compliant\": true"));
}

#[test]
fn cli_rejects_missing_path() {
    let out = Command::new(bin()).arg("report").output().expect("binary runs");
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn cli_rejects_tampered_ledger() {
    let mut path = std::env::temp_dir();
    path.push(format!("tpt-chrysalis-cli-tampered-{}.bin", std::process::id()));
    let mut bytes = SaveLoad::save(&sample_ledger());
    // Flip a byte in the tail entry's region.
    let idx = bytes.len() - 4;
    bytes[idx] ^= 0x01;
    fs::write(&path, bytes).expect("writes tampered bytes");
    let out = Command::new(bin())
        .arg("history")
        .arg(&path)
        .output()
        .expect("binary runs");
    // Two acceptable outcomes, neither of which reports the full chain:
    //  - the tampered tail fails the CRC/chain check on reload (exit 1), or
    //  - the WAL truncates the torn tail, so the replay yields a *shorter*
    //    valid chain that can no longer report both entries.
    if out.status.code() == Some(0) {
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        assert!(
            !stdout.contains("bob"),
            "tampered ledger must not report the full chain:\n{stdout}"
        );
    } else {
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        assert!(
            stderr.contains("ledger load failed"),
            "unexpected stderr: {stderr}"
        );
    }
}
