# tpt-chrysalis justfile - Standardized dev commands per Open Source/.github/CONTRIBUTING.md
# Usage: just <command>

# Variables
CARGO := cargo
TRUNK := trunk
DOCKER_COMPOSE := docker compose -f deploy/docker-compose.yml -f deploy/docker-compose.override.yml
KIND := kind
HELM := helm

# Default target
default: help

# Help
help:
	@echo "Available commands:"
	@echo "  just lint          - Run cargo fmt --check and cargo clippy -- -D warnings"
	@echo "  just test          - Run cargo test --workspace"
	@echo "  just build         - Run cargo build --workspace --release"
	@echo "  just dev           - Start local dev stack (docker compose)"
	@echo "  just dev-stop      - Stop local dev stack"
	@echo "  just dev-logs      - Follow dev stack logs"
	@echo "  just wasm-build    - Build frontend Wasm (trunk build --release)"
	@echo "  just wasm-dev      - Run frontend dev server (trunk serve)"
	@echo "  just ci            - Run full CI pipeline locally"
	@echo "  just kind-up       - Create kind cluster for Helm testing"
	@echo "  just kind-down     - Delete kind cluster"
	@echo "  just helm-lint     - Lint Helm chart"
	@echo "  just helm-template - Render Helm templates"
	@echo "  just helm-install  - Install Helm chart to kind"
	@echo "  just helm-test     - Run Helm test suite"
	@echo "  just fmt           - Format code (cargo fmt)"
	@echo "  just check         - Check code (cargo check --workspace)"
	@echo "  just clean         - Clean build artifacts"

# Lint: format + clippy
lint:
	$(CARGO) fmt --all -- --check
	$(CARGO) clippy --workspace -- -D warnings

# Test: unit + integration
test:
	$(CARGO) test --workspace

# Build release
build:
	$(CARGO) build --workspace --release

# Development stack
dev:
	$(DOCKER_COMPOSE) up -d
	@echo "Stack started. API: http://localhost:3000, Frontend: http://localhost:8080, MinIO: http://localhost:9001"

dev-stop:
	$(DOCKER_COMPOSE) down

dev-logs:
	$(DOCKER_COMPOSE) logs -f

# Frontend Wasm
wasm-build:
	cd frontend && $(TRUNK) build --release

wasm-dev:
	cd frontend && $(TRUNK) serve --port 8080 --open

# CI pipeline (mirrors GitHub Actions)
ci: lint test build wasm-build helm-lint
	@echo "All CI checks passed!"

# Kind cluster for Helm testing
kind-up:
	$(KIND) create cluster --name tpt-chrysalis --config deploy/kind-config.yaml || true
	kubectl wait --for=condition=Ready nodes --all --timeout=120s

kind-down:
	$(KIND) delete cluster --name tpt-chrysalis

# Helm operations
helm-lint:
	$(HELM) lint deploy/helm/tpt-chrysalis

helm-template:
	$(HELM) template tpt-chrysalis deploy/helm/tpt-chrysalis --namespace tpt-chrysalis --create-namespace

helm-install: kind-up
	kubectl create namespace tpt-chrysalis --dry-run=client -o yaml | kubectl apply -f -
	$(HELM) upgrade --install tpt-chrysalis deploy/helm/tpt-chrysalis \
		--namespace tpt-chrysalis \
		--create-namespace \
		--wait --timeout 5m \
		-f deploy/helm/tpt-chrysalis/values-secrets.yaml.example

helm-test: helm-install
	kubectl wait --for=condition=Available deployment --all -n tpt-chrysalis --timeout=300s
	kubectl get pods -n tpt-chrysalis
	# Run integration tests against the cluster
	$(CARGO) test --workspace -- --test-threads=1

# Coverage (cargo-llvm-cov): line coverage with an 80% floor on the
# Phase 1 safety-critical crates (TODO.md Phase 1, line 92).
cover:
	$(CARGO) llvm-cov --workspace --lcov --output-path lcov.info
	$(CARGO) llvm-cov --workspace --summary-only --fail-under-lines 80 || \
		echo "WARNING: coverage below 80% target (see lcov.info)"

# Check (fast compile check)
check:
	$(CARGO) check --workspace

# Clean
clean:
	$(CARGO) clean
	rm -rf frontend/dist frontend/target
	docker system prune -f

# Secrets generation (for local dev)
gen-secrets:
	@mkdir -p deploy/secrets
	@openssl rand -hex 32 > deploy/secrets/capability_root_key
	@chmod 600 deploy/secrets/capability_root_key
	@echo "Generated capability_root_key in deploy/secrets/"

# Update dependencies
update-deps:
	$(CARGO) update --workspace
	cd frontend && $(CARGO) update

# Generate schema types (when schema-gen feature is added)
gen-schemas:
	@echo "Schema generation not yet implemented"

.PHONY: help lint test build dev dev-stop dev-logs wasm-build wasm-dev ci kind-up kind-down helm-lint helm-template helm-install helm-test fmt check clean gen-secrets update-deps gen-schemas