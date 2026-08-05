//! `tpt-chrysalis-api`: the minimal Phase 1 service surface (spec §7).
//!
//! Exposes the orchestrator's state and command path over HTTP with
//! capability-token bearer credentials:
//!
//! - `POST /api/v1/tokens` — mint a bearer credential for an actor/action.
//! - `GET  /api/v1/pdsg`   — PDSG snapshot (requires `ReadState`).
//! - `GET  /api/v1/audit`  — ALCOA+ compliance report over the audit ledger
//!   (requires `ReadState`; optional `?actor=` filter).
//! - `POST /api/v1/intents`— submit a protocol intent (requires
//!   `SubmitProtocol`); the bearer's actor is recorded in the ledger.
//!
//! The service is single-threaded and dependency-light on purpose: a tiny
//! HTTP binding ([`http`]) over pure handlers ([`api`]), with bearer
//! credentials issued by [`tokens`]. See `src/main.rs` for the TCP loop.

extern crate alloc;

pub mod api;
pub mod domain;
pub mod http;
pub mod tokens;

pub use api::{
    ApiAcceptedStep, ApiError, ApiExecution, ApiState, PdsgSnapshot, SubmitIntentRequest,
    OrganoidVerifyRequest, OrganoidVerifyResponse, PdoScreenRequest, PdoScreenResponse,
    VisionCaptureRequest, VisionCaptureResponse, default_gate,
};
pub use domain::{OrganoidDomain, VisionDomain};
pub use http::{HttpRequest, HttpResponse, extract_bearer, handle_request, parse_request, write_response};
pub use tokens::{ApiAuthError, ApiToken, ApiTokenManager};
