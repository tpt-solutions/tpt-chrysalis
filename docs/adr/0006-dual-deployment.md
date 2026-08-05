# ADR 0006: Dual Docker Compose + Kubernetes/Helm Deployment from Day One

**Status**: Accepted
**Date**: 2026-08-05

## Context

The specification and ecosystem precedent (`tpt-soma`, `tpt-cerebrum`) require maintaining **dual deployment paths** — Docker Compose for local development and Kubernetes/Helm for staging/production — from the first phase, with genuine parity maintained every phase.

This is not a "deploy later" item; the deployment topology (services, networking, secrets, volumes) must be exercised in CI and local dev continuously so that Phase 4's larger deployment is the same chart grown incrementally, not a rewrite.

## Decision

**Scaffold both deployment paths in Phase 0 and keep them in strict parity every phase.**

### Docker Compose (`deploy/docker-compose.yml`)
- `tpt-archon` service (kernel + storage + relational)
- `minio` service (object storage for imaging, added Phase 2 but scaffolded now)
- `api` service (`tpt-chrysalis-api` binary)
- `frontend` dev service (Leptos + Trunk hot-reload)
- Named volumes for `tpt-archon` WAL, MinIO data, frontend dist
- `.env` for secrets (capability-token root signing key, MinIO credentials)

### Helm Chart (`deploy/helm/tpt-chrysalis/`)
- `Chart.yaml` with version, appVersion, dependencies
- `values.yaml` mirroring Compose services 1:1 (image tags, resource limits, replicas)
- Templates: `Deployment` + `Service` + `ConfigMap` + `Secret` per service
- `Ingress` for API + frontend (TLS via cert-manager)
- `PersistentVolumeClaim` for `tpt-archon` WAL and MinIO data
- `values-secrets.yaml.example` for GitOps-safe secret injection

### Parity Rules (enforced in CI)
1. **Service count match** — every Compose service has a Helm Deployment (and vice versa)
2. **Environment variable parity** — same `ENV` keys in both; Compose uses `.env`, Helm uses `valueFrom.secretKeyRef`
3. **Volume mount parity** — same mount paths, same subPath conventions
4. **Health check parity** — same `/health` endpoints, same liveness/readiness probes
5. **Network policy parity** — Compose `networks:` maps to K8s `NetworkPolicy` (deny-by-default, explicit allows)

### CI Validation
- `kind` cluster in GitHub Actions spins up Helm chart; runs integration tests
- `docker compose up` in GitHub Actions runs same integration tests
- Both must pass on every PR

## Consequences

### Positive
- **Zero deployment drift** — local dev and production are the same topology
- **Early ops maturity** — Helm chart is battle-tested by Phase 4; no "deploy day" surprises
- **Ecosystem consistency** — matches `tpt-soma`/`tpt-cerebrum` precedent
- **GitOps ready** — Helm chart + `values-secrets.yaml` is the single source of truth for ArgoCD/Flux

### Negative / Risks
- **Maintenance overhead** — two configs to keep in sync
- **Compose limitations** — no rolling updates, no autoscaling, no network policies (simulated via `networks:`)
- **Secrets divergence risk** — `.env` vs K8s `Secret` can drift

### Mitigations
- **Single source of truth for service definitions** — a `deploy/services.yaml` (or similar) generates both Compose and Helm templates (future automation; for now, manual parity enforced by CI)
- **Pre-commit hook** runs `docker compose config` + `helm template` lint
- **Secrets strategy** — `.env.example` and `values-secrets.yaml.example` committed; actual secrets never committed; CI injects from vault

## Alternatives Considered

1. **Compose only for dev, write Helm later** — rejected: spec says "dual Docker Compose + Kubernetes/Helm deployment maintained from day one"; `tpt-cerebrum` proved this pays off
2. **Helm only, use `helm template` for local dev** — rejected: no hot-reload, no Trunk integration, slower inner loop; Compose is the documented ecosystem standard for local dev
3. **K3d/kind for local dev instead of Compose** — rejected: adds K8s complexity to inner loop; Compose is simpler for `cargo run` + Trunk workflow

## References
- `spec.txt` §7: "dual Docker Compose + Kubernetes/Helm deployment maintained from day one"
- `tpt-soma` / `tpt-cerebrum` deployment precedent
- ADR 0005 (MinIO scaffolded in both from Phase 0)
- ADR 0007 (schema evolution — deployment configs versioned with schema)
- `deploy/` directory (to be created)