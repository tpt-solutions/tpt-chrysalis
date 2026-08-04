//! `tpt-chrysalis-compliance` CLI: ALCOA+ report / attributable-history
//! retrieval over a persisted audit ledger (TODO.md Phase 1 compliance).
//!
//! Usage:
//!   tpt-chrysalis-compliance report  <ledger.bin> [--actor NAME] [--json]
//!   tpt-chrysalis-compliance history <ledger.bin> [--actor NAME]
//!
//! `ledger.bin` is the WAL byte payload from `AuditLedger::save()`.

use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use tpt_chrysalis_compliance::build_report;
use tpt_chrysalis_core::{AuditLedger, LedgerError};

fn usage() {
    eprintln!(
        "usage: tpt-chrysalis-compliance {{report|history}} <ledger.bin> [--actor NAME] [--json]"
    );
}

fn parse_args(args: &[String]) -> Result<(String, String, Option<String>, bool), String> {
    let Some((sub, rest)) = args.split_first() else {
        return Err("missing subcommand".to_string());
    };
    if sub != "report" && sub != "history" {
        return Err(format!("unknown subcommand `{sub}`"));
    }
    let mut path: Option<String> = None;
    let mut actor: Option<String> = None;
    let mut json = false;
    let mut i = 0usize;
    while i < rest.len() {
        match rest[i].as_str() {
            "--actor" => {
                i += 1;
                let Some(name) = rest.get(i) else {
                    return Err("`--actor` requires a name".to_string());
                };
                actor = Some(name.clone());
            }
            "--json" => json = true,
            other if other.starts_with('-') => return Err(format!("unknown option `{other}`")),
            other => {
                if path.is_some() {
                    return Err(format!("unexpected positional argument `{other}`"));
                }
                path = Some(other.to_string());
            }
        }
        i += 1;
    }
    let path = path.ok_or_else(|| "missing ledger path".to_string())?;
    Ok((sub.clone(), path, actor, json))
}

fn load_ledger(path: &str) -> Result<AuditLedger, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("cannot read {path}: {e}"))?;
    AuditLedger::from_bytes(&bytes).map_err(|e: LedgerError| format!("ledger load failed: {e}"))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (sub, path, actor, json) = match parse_args(&args) {
        Ok(parsed) => parsed,
        Err(msg) => {
            eprintln!("error: {msg}");
            usage();
            return ExitCode::from(2);
        }
    };
    let ledger = match load_ledger(&path) {
        Ok(l) => l,
        Err(msg) => {
            eprintln!("error: {msg}");
            return ExitCode::from(1);
        }
    };

    let report = build_report(&ledger, now_ms(), actor.as_deref());
    match sub.as_str() {
        "report" => {
            if json {
                let out = serde_json::to_string_pretty(&report).expect("report serializes");
                println!("{out}");
            } else {
                println!("{report}");
            }
        }
        "history" => {
            for e in &report.history {
                let expiry = e
                    .token_expires_at_ms
                    .map(|x| x.to_string())
                    .unwrap_or_else(|| "-".to_string());
                println!(
                    "#{seq:<4} t={ts:<10} actor={actor:<8} {outcome:<9} {action}  [scope={scope}, expires={expiry}]",
                    seq = e.seq,
                    ts = e.timestamp_ms,
                    actor = e.actor,
                    outcome = e.outcome,
                    action = e.action,
                    scope = e.scope,
                );
            }
        }
        _ => unreachable!("parse_args restricted subcommand"),
    }
    ExitCode::SUCCESS
}
