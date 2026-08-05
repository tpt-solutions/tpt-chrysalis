//! Integration spikes confirming the `tpt-archon` integration surface *before*
//! any chrysalis-specific logic is built on top of it (TODO.md Phase 1,
//! lines 40–41). These use `tpt-archon`'s APIs directly, with no
//! `tpt-chrysalis` wrapper, to prove the substrate behaves as spec §4 Layer 2
//! requires: unforgeable, revocable capability tokens, and a zero-copy
//! append-only WAL ledger.

use serde_json::json;
use tpt_archon_bridge::capability::{CapabilityIssuer, Resource, Right};
use tpt_archon_core::wal::{RecordKind, Wal};

/// Spike (line 40): issue a capability token via `tpt-archon-bridge` and verify
/// it end-to-end — including revocation and scope mismatch — against the
/// issuing kernel.
#[test]
fn spike_archon_capability_token_issue_and_verify() {
    let mut issuer = CapabilityIssuer::new();

    // Mint a token scoped to channel 1 / Write.
    let cap = issuer.mint(Resource::Channel(1), Right::Write);

    // A freshly minted token authorizes exactly its resource + right.
    assert!(
        issuer.authorizes(&cap, Resource::Channel(1), Right::Write),
        "token must authorize its own channel/right"
    );
    // A different channel (or right) must NOT authorize.
    assert!(
        !issuer.authorizes(&cap, Resource::Channel(2), Right::Write),
        "token must not authorize a different channel"
    );
    assert!(
        !issuer.authorizes(&cap, Resource::Channel(1), Right::Read),
        "token must not authorize a different right"
    );

    // Revocation is live: after revoke the token no longer authorizes.
    issuer.revoke(&cap);
    assert!(
        !issuer.authorizes(&cap, Resource::Channel(1), Right::Write),
        "revoked token must not authorize"
    );
}

/// Spike (line 41): write a record through `tpt-archon-core`'s WAL and read it
/// back, confirming the "zero-copy, append-only ledger" behaves as required.
#[test]
fn spike_archon_wal_write_and_read_back() {
    let mut wal = Wal::new();
    let payload = serde_json::to_vec(&json!({
        "seq": 0,
        "action": "dispense",
        "reagent": "A",
        "volume_ul": 100
    }))
    .expect("serialize");

    // Write-ahead: frame and append before any other work proceeds.
    wal.append(RecordKind::Commit, 0, &payload);

    // Persist as raw bytes (the form that hits disk / the audit volume).
    let bytes = wal.as_bytes().to_vec();
    assert!(!bytes.is_empty(), "WAL must produce non-empty bytes");

    // Reload from bytes and replay the chain.
    let reloaded = Wal::from_bytes(&bytes);
    let mut seen = Vec::new();
    reloaded.replay(|rec| {
        seen.extend_from_slice(&rec.payload);
    });
    assert_eq!(seen, payload, "replayed WAL payload must match what was appended");
}
