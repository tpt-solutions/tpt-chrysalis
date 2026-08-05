//! `tpt-chrysalis-compliance` CLI: ALCOA+ report / attributable-history
//! retrieval over a persisted audit ledger (TODO.md Phase 1 compliance).
//!
//! Usage:
//!   tpt-chrysalis-compliance report   <ledger.bin> [--actor NAME] [--json]
//!   tpt-chrysalis-compliance history  <ledger.bin> [--actor NAME]
//!   tpt-chrysalis-compliance integrity <ledger.bin> [--watch] [--interval-ms N]
//!
//! `ledger.bin` is the WAL byte payload from `AuditLedger::save()`. The
//! `integrity` subcommand is the scheduled audit-chain-integrity job: it
//! re-walks the hash chain and exits non-zero on any broken link, so it can
//! gate CI or feed an alerting pipeline.

use std::process::ExitCode;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tpt_chrysalis_compliance::build_report;
use tpt_chrysalis_core::{AuditLedger, LedgerError};

fn usage() {
    eprintln!(
        "usage: tpt-chrysalis-compliance {{report|history|integrity}} <ledger.bin> [--actor NAME] [--json] [--watch] [--interval-ms N]"
    );
}

#[derive(Default)]
struct CliArgs {
    sub: String,
    path: String,
    actor: Option<String>,
    json: bool,
    watch: bool,
    interval_ms: u64,
}

fn parse_args(args: &[String]) -> Result<CliArgs, String> {
    let Some((sub, rest)) = args.split_first() else {
        return Err("missing subcommand".to_string());
    };
    if sub != "report" && sub != "history" && sub != "integrity" {
        return Err(format!("unknown subcommand `{sub}`"));
    }
    let mut out = CliArgs {
        sub: sub.clone(),
        ..Default::default()
    };
    let mut i = 0usize;
    while i < rest.len() {
        match rest[i].as_str() {
            "--actor" => {
                i += 1;
                let Some(name) = rest.get(i) else {
                    return Err("`--actor` requires a name".to_string());
                };
                out.actor = Some(name.clone());
            }
            "--json" => out.json = true,
            "--watch" => out.watch = true,
            "--interval-ms" => {
                i += 1;
                let Some(v) = rest.get(i) else {
                    return Err("`--interval-ms` requires a value".to_string());
                };
                out.interval_ms = v
                    .parse::<u64>()
                    .map_err(|_| "`--interval-ms` must be a positive integer".to_string())?;
            }
            other if other.starts_with('-') => return Err(format!("unknown option `{other}`")),
            other => {
                if !out.path.is_empty() {
                    return Err(format!("unexpected positional argument `{other}`"));
                }
                out.path = other.to_string();
            }
        }
        i += 1;
    }
    if out.path.is_empty() {
        return Err("missing ledger path".to_string());
    }
    Ok(out)
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

/// The scheduled integrity job. Re-walks the hash chain and reports any broken
/// link. Exits `1` when the chain is broken so it can gate CI / alerting.
fn run_integrity(ledger: &AuditLedger, watch: bool, interval_ms: u64) -> ExitCode {
    let check = || {
        let integrity = ledger.verify_integrity();
        let status = if integrity.is_intact() { "INTACT" } else { "BROKEN" };
        eprintln!(
            "[integrity] {} entries, {} broken link(s) — chain {}",
            integrity.entry_count,
            integrity.broken_seqs.len(),
            status
        );
        integrity.is_intact()
    };
    let mut intact = check();
    if watch {
        loop {
            thread::sleep(Duration::from_millis(interval_ms.max(250)));
            intact = check();
        }
    }
    if intact {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cli = match parse_args(&args) {
        Ok(parsed) => parsed,
        Err(msg) => {
            eprintln!("error: {msg}");
            usage();
            return ExitCode::from(2);
        }
    };
    let ledger = match load_ledger(&cli.path) {
        Ok(l) => l,
        Err(msg) => {
            eprintln!("error: {msg}");
            return ExitCode::from(1);
        }
    };

    let report = build_report(&ledger, now_ms(), cli.actor.as_deref());
    match cli.sub.as_str() {
        "report" => {
            if cli.json {
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
        "integrity" => {
            return run_integrity(&ledger, cli.watch, cli.interval_ms);
        }
        _ => unreachable!("parse_args restricted subcommand"),
    }
    ExitCode::SUCCESS
}
