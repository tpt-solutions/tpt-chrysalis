# Compliance Mechanism — `tpt-chrysalis` Phase 1

> Non-code deliverable for `TODO.md` Phase 1 → `tpt-chrysalis-compliance` (line 85).
> Documents the ALCOA+ report *mechanism* and how it maps onto the NZ HIPC 2020
> and (later) FDA 21 CFR Part 11 criteria. Real regulatory submission is out of
> scope for Phase 1; this document makes the report *format and mechanism*
> reviewable.

## 1. Source of truth

The audit ledger (`crates/tpt-chrysalis-core/src/ledger.rs`) is the single
immutable source of truth for every physical action. It is built on
`tpt-archon-core`'s `Wal` (append-only, LSN-ordered, CRC-framed — spec §4
Layer 2) and extended with a SHA-256 hash chain. Every entry carries its own
hash computed over `prev_hash || canonical(entry \ hash)`; the previous entry's
hash is embedded, so tampering anywhere in the chain is detectable by
recomputation.

No raw command executes outside this logging choke-point: the orchestrator
appends an `AuditEntry` (accepted *or* rejected) *before* the HAL runs, and the
entry is WAL-durable before the caller proceeds.

## 2. What gets recorded

For each enforcement decision the ledger records, losslessly:

| Field | Meaning |
|---|---|
| `seq` | Monotonic sequence number (mirrors the WAL LSN ordering) |
| `timestamp_ms` | Wall-clock time at the enforcement point (contemporaneity) |
| `actor` | Bearer identity on the capability token (attributability) |
| `action` | The physical action attempted (`PhysicalAction`) |
| `outcome` | `Accepted` or `Rejected { reason }` (refusals are never swallowed) |
| `scope` | The capability scope the token was issued for |
| `token_expires_at_ms` | Token expiry at enforcement (if token-gated) |
| `prev_hash` / `hash` | The hash-chain links |

Rejected commands are recorded too — a forged/expired/unscoped token is an
auditable security event, not a silent no-op.

## 3. ALCOA+ mapping

The report generator (`build_report` in `lib.rs`) is a **pure, deterministic
function of the chain bytes** — it never mutates the ledger and never makes
network calls, so the same ledger always produces the same report. Each of the
nine ALCOA+ criteria is evaluated with concrete evidence:

| Criterion | How `tpt-chrysalis` satisfies it | Evidence computed |
|---|---|---|
| **A**ttributable | Every entry records the token bearer | Distinct actor count over the chain |
| **L**egible | Actions rendered canonically (e.g. `dispense 100uL A -> w3`) | Rendered history rows |
| **C**ontemporaneous | Timestamped at the enforcement point | Timestamp window `min..=max` |
| **O**riginal | Append-only, hash-chained WAL originals | Contiguous `seq` 0..=N |
| **A**ccurate | Chain re-verifies with no broken links | `verify_integrity()` broken-link count |
| **C**omplete | No entry silently omitted from the report | `reported == ledger` entry count |
| **C**onsistent | Sequence numbers and cross-links agree | `seq` contiguity + intact chain |
| **E**nduring | Ledger persists as WAL bytes, reloads bit-for-bit | `SaveLoad::save/load` round-trip |
| **Av**ailable | Full attributable history retrievable on demand | CLI + API exposure |

`core_compliant` is `true` only when the five core criteria
(A, L, C, O, A2) pass. An empty ledger is **not** prematurely compliant.

## 4. Chain-integrity job

`LedgerIntegrityReport` (`verify_integrity`) re-walks the chain, recomputing
every hash and cross-checking `prev_hash`. Any mismatch is reported as a broken
sequence number. This is exposed three ways:

1. Inside every ALCOA+ report (the `Accurate` criterion).
2. As a dedicated runnable job — `tpt-chrysalis-compliance integrity <ledger.bin>`
   (see §6), which exits non-zero on a broken chain so it can gate CI/alerting.
3. As a scheduled `CronJob` in the Helm chart and a compose service (see §7).

## 5. Regulatory mapping

- **NZ HIPC 2020** (Health Information Privacy Code): the `actor` field
  provides attribution; Phase 4 adds the de-identification pipeline
  (`tpt-chrysalis-compliance`) that strips patient-derived organoid metadata
  before any data leaves the facility. The append-only, hash-chained ledger
  gives the *integrity* limb of HIPC's security safeguards.
- **FDA 21 CFR Part 11**: attributable (signed/token-bearer), legible
  (canonical render), contemporaneous (enforcement-point timestamp), original
  (append-only WAL), accurate (hash chain). The chain-hash + WAL CRC together
  provide the "complete and accurate" record and the tamper-evidence required
  by §11.10.
- **ALCOA+** is the umbrella reviewed in §3.

## 6. Running the report / integrity job

```bash
# Human/JSON ALCOA+ report over a persisted ledger
tpt-chrysalis-compliance report ledger.bin [--actor alice] [--json]

# Attributable history only
tpt-chrysalis-compliance history ledger.bin [--actor alice]

# Scheduled integrity check (exit 1 on a broken chain)
tpt-chrysalis-compliance integrity ledger.bin
tpt-chrysalis-compliance integrity ledger.bin --watch --interval-ms 60000
```

`ledger.bin` is the WAL byte payload produced by `AuditLedger::save()`
(`as_bytes()`).

## 7. Deployment wiring

- **Helm**: `deploy/helm/tpt-chrysalis/templates/integrity-cronjob.yaml`
  runs the `integrity` subcommand on a schedule against the mounted ledger
  volume.
- **Compose**: `deploy/docker-compose.yml` includes an `integrity-check`
  service that loops the `integrity --watch` mode against the shared ledger.

## 8. Limitations (Phase 1)

- No real subject/patient data flows; the *format and mechanism* are validated,
  not an actual submission.
- De-identification (HIPC/GDPR/HIPAA) is a Phase 4 deliverable.
- The hash chain is integrity-protecting, not confidentiality-protecting; the
  ledger volume is encrypted at rest via the deploy secret/volume strategy.
