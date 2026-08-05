# ADR 0005: MinIO/S3-Compatible Object Storage for High-Content Microscopy Imaging

**Status**: Accepted
**Date**: 2026-08-05

## Context

Phase 2 (`tpt-chrysalis-vision`, per spec §5D) requires **high-content microscopy imaging** — storing and serving large image files (multi-GB TIFF/OMERO datasets) and derived feature data (cell morphology, neurite outgrowth, barrier integrity metrics).

Neither `tpt-archon` nor `tpt-keystone-db` provide object/blob storage:
- `tpt-archon-core` is a WAL/page-manager for append-only audit logs (small, sequential records)
- `tpt-archon-relational` is a SQL engine for graph/relational data (PDSG nodes/edges)
- `tpt-keystone-db` (if it exists) is unconfirmed and not in the current dependency graph

The sibling project `tpt-cerebrum` faced the same need for volumetric imaging and solved it by adding a **MinIO/S3-compatible store** alongside `tpt-archon`. This is a proven pattern in the ecosystem.

## Decision

**Add a MinIO/S3-compatible object store** (MinIO for local/dev, S3-compatible for cloud) as a dedicated service for Phase 2 imaging data. It runs alongside `tpt-archon` (not replacing it) and is scaffolded in Phase 0 so the deployment topology is stable from day one.

Specifically:
- **MinIO** in `docker-compose.yml` and Helm chart for local/dev/staging
- **S3 API** as the stable interface — production can swap to AWS S3, GCS, Azure Blob, or on-prem MinIO without code changes
- **Checksum-on-write** — every object stores SHA-256 (via `sha2` crate) at ingest; verified on read
- **Structured metadata** — imaging metadata (experiment ID, chip ID, timestamp, modality) stored as S3 object tags + a parallel `tpt-archon-relational` index for query
- **Access control** — capability-token-scoped presigned URLs (via `tpt-archon-kernel` integration) for read/write; no public access

## Consequences

### Positive
- **Proven ecosystem pattern** — mirrors `tpt-cerebrum`'s imaging storage architecture
- **S3 API portability** — no vendor lock-in; local dev uses MinIO, production uses any S3-compatible backend
- **Separation of concerns** — audit ledger (WAL, small records) stays in `tpt-archon`; imaging blobs (large, random access) go to object store
- **Checksum integrity** — matches the hash-chained audit philosophy for data-at-rest

### Negative / Risks
- **Additional operational component** — MinIO must be deployed, monitored, backed up
- **Consistency boundary** — PDSG (in `tpt-archon-relational`) and imaging objects (in MinIO) are separate stores; distributed transactions not supported
- **Cost at scale** — multi-GB imaging datasets accumulate; lifecycle policies needed

### Mitigations
- Scaffold MinIO in Phase 0 deployment (Compose + Helm) so ops tooling matures early
- Use **presigned URLs with short TTL** (capability-token-gated) so the API never proxies blob bytes
- Define a **compensating transaction pattern** for PDSG + imaging writes: write object first, then PDSG index; on PDSG failure, mark object orphaned for async cleanup
- Add lifecycle rules (transition to cold storage after 90 days, delete after retention policy) in Helm values

## Alternatives Considered

1. **Store images in `tpt-archon-relational` as BLOBs** — rejected: SQL engines are not optimized for multi-GB random-access blobs; kills WAL/page-manager performance
2. **Use `tpt-keystone-db` if it provides object storage** — rejected: unconfirmed existence; MinIO is battle-tested and `tpt-cerebrum` already validated the pattern
3. **Filesystem storage (NFS/local disk)** — rejected: no S3 API, no presigned URLs, no cloud portability, hard to scale across K8s pods

## References
- `spec.txt` §5D: `tpt-chrysalis-vision` — high-content microscopy imaging
- `spec.txt` §7: "object/blob storage that neither `tpt-archon` nor `tpt-keystone-db` provide"
- `tpt-cerebrum` precedent: MinIO/S3 for volumetric imaging
- ADR 0001 (archon reuse — ledger stays in archon, imaging goes to MinIO)
- ADR 0006 (dual Compose/Helm — MinIO scaffolded in both from Phase 0)