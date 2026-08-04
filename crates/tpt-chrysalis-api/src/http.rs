//! A minimal, dependency-free HTTP/1.1 binding for the API service.
//!
//! This is deliberately tiny: a pure [`handle_request`] over a parsed
//! [`HttpRequest`], plus a [`parse_request`] used by the TCP server loop.
//! There is no connection pooling, no chunked transfer encoding, no TLS — the
//! Phase 1 service is a dev/loopback surface. A real fronting layer (TLS,
//! auth hardening) belongs in deployment.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use serde::Serialize;
use tpt_chrysalis_core::ActionId;

use crate::api::{ApiError, ApiState, IssueTokenRequest, SubmitIntentRequest};
use crate::tokens::ApiAuthError;

/// One parsed HTTP request.
#[derive(Debug, Clone)]
pub struct HttpRequest {
    pub method: String,
    pub path: String,
    /// Lower-cased header names.
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl HttpRequest {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }
}

/// A minimal response.
#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub content_type: &'static str,
    pub body: Vec<u8>,
}

impl HttpResponse {
    fn json(status: u16, value: &impl Serialize) -> Self {
        let body = serde_json::to_vec(value).unwrap_or_else(|_| br#"{"error":"serialization failed"}"#.to_vec());
        Self {
            status,
            content_type: "application/json",
            body,
        }
    }

    fn error(status: u16, message: &str) -> Self {
        Self::json(status, &serde_json::json!({ "error": message }))
    }
}

/// The bearer value from `Authorization: Bearer <token>`, if present.
pub fn extract_bearer(req: &HttpRequest) -> Option<&str> {
    req.header("authorization")?
        .strip_prefix("Bearer ")
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

fn status_text(status: u16) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        409 => "Conflict",
        422 => "Unprocessable Entity",
        500 => "Internal Server Error",
        _ => "Status",
    }
}

fn auth_error_message(e: ApiAuthError) -> &'static str {
    match e {
        ApiAuthError::Missing => "missing bearer credential",
        ApiAuthError::Unknown => "unknown bearer credential",
        ApiAuthError::WrongAction => "token does not authorize this operation",
        ApiAuthError::Unscoped => "token is for a different equipment scope",
        ApiAuthError::Expired => "token expired",
        ApiAuthError::Unauthorized => "token revoked or invalid",
    }
}

fn api_error_response(e: ApiError) -> HttpResponse {
    HttpResponse::error(e.status_code(), &e.message())
}

/// Routes one request to the service handlers. Pure and unit-testable: no I/O.
pub fn handle_request(state: &mut ApiState, req: &HttpRequest, now_ms: u64) -> HttpResponse {
    let path = req.path.as_str();
    match (req.method.as_str(), path) {
        ("POST", "/api/v1/tokens") => handle_issue_token(state, req, now_ms),
        ("GET", "/api/v1/pdsg") => handle_authenticated(state, req, ActionId::ReadState, now_ms, |s, _| {
            match s.pdsg_snapshot() {
                Ok(snapshot) => HttpResponse::json(200, &snapshot),
                Err(e) => api_error_response(e),
            }
        }),
        ("GET", "/api/v1/audit") => handle_authenticated(state, req, ActionId::ReadState, now_ms, |s, _| {
            let actor = query_param(&req.path, "actor");
            HttpResponse::json(200, &s.compliance_report(actor.as_deref(), now_ms))
        }),
        ("POST", "/api/v1/intents") => handle_submit_intent(state, req, now_ms),
        _ => HttpResponse::error(404, "no such endpoint"),
    }
}

fn handle_issue_token(state: &mut ApiState, req: &HttpRequest, now_ms: u64) -> HttpResponse {
    let parsed: IssueTokenRequest = match serde_json::from_slice(&req.body) {
        Ok(v) => v,
        Err(e) => return HttpResponse::error(400, &format!("malformed body: {e}")),
    };
    let token = state.issue_token(&parsed.actor, parsed.action, parsed.ttl_ms, now_ms);
    HttpResponse::json(201, &token)
}

fn handle_submit_intent(state: &mut ApiState, req: &HttpRequest, now_ms: u64) -> HttpResponse {
    let bearer = match extract_bearer(req) {
        Some(b) => b.to_string(),
        None => return HttpResponse::error(401, auth_error_message(ApiAuthError::Missing)),
    };
    let actor = match state.authenticate(&bearer, ActionId::SubmitProtocol, now_ms) {
        Ok(a) => a.to_string(),
        Err(e) => return HttpResponse::error(401, auth_error_message(e)),
    };
    let parsed: SubmitIntentRequest = match serde_json::from_slice(&req.body) {
        Ok(v) => v,
        Err(e) => return HttpResponse::error(400, &format!("malformed body: {e}")),
    };
    match state.submit_intent(&parsed, &actor, now_ms) {
        Ok(exec) => HttpResponse::json(201, &exec),
        Err(e) => api_error_response(e),
    }
}

fn handle_authenticated(
    state: &mut ApiState,
    req: &HttpRequest,
    action: ActionId,
    now_ms: u64,
    f: impl FnOnce(&mut ApiState, String) -> HttpResponse,
) -> HttpResponse {
    let Some(bearer) = extract_bearer(req) else {
        return HttpResponse::error(401, auth_error_message(ApiAuthError::Missing));
    };
    match state.authenticate(bearer, action, now_ms) {
        // Copy the actor out so the closure can borrow `state` mutably.
        Ok(actor) => f(state, actor.to_string()),
        Err(e) => HttpResponse::error(401, auth_error_message(e)),
    }
}

/// Reads a `?key=value` query parameter.
pub fn query_param(path: &str, key: &str) -> Option<String> {
    let query = path.split_once('?')?.1;
    for pair in query.split('&') {
        if let Some((k, v)) = pair.split_once('=') {
            if k == key {
                return Some(v.to_string());
            }
        }
    }
    None
}

/// Parses a raw HTTP/1.1 request (request line + headers + Content-Length
/// body). Returns `Err` when the buffer is incomplete or malformed.
pub fn parse_request(buf: &[u8]) -> Result<HttpRequest, String> {
    let text = core::str::from_utf8(buf).map_err(|_| "non-utf8 request".to_string())?;
    let mut lines = text.split("\r\n");
    let request_line = lines.next().ok_or("missing request line")?.to_string();
    let mut parts = request_line.split_whitespace();
    let method = parts.next().ok_or("missing method")?.to_string();
    let path = parts.next().ok_or("missing path")?.to_string();

    let mut headers = Vec::new();
    let mut content_length = 0usize;
    let mut saw_header_end = false;
    for line in lines {
        if line.is_empty() {
            saw_header_end = true;
            break;
        }
        if let Some((k, v)) = line.split_once(':') {
            let key = k.trim().to_ascii_lowercase();
            let value = v.trim().to_string();
            if key == "content-length" {
                content_length = value.parse().unwrap_or(0);
            }
            headers.push((key, value));
        }
    }
    if !saw_header_end {
        return Err("incomplete headers".to_string());
    }

    let body_start = text
        .find("\r\n\r\n")
        .map(|i| i + 4)
        .ok_or("missing header terminator")?;
    let body = &buf[body_start..];
    if body.len() < content_length {
        return Err("incomplete body".to_string());
    }
    let body = body[..content_length].to_vec();
    Ok(HttpRequest {
        method,
        path,
        headers,
        body,
    })
}

/// Serializes a response for the wire (status line + headers + body).
pub fn write_response(resp: &HttpResponse) -> Vec<u8> {
    let head = alloc::format!(
        "HTTP/1.1 {status} {text}\r\nContent-Type: {ct}\r\nContent-Length: {len}\r\nConnection: close\r\n\r\n",
        status = resp.status,
        text = status_text(resp.status),
        ct = resp.content_type,
        len = resp.body.len(),
    );
    let mut out = head.into_bytes();
    out.extend_from_slice(&resp.body);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::ApiState;
    use tpt_chrysalis_core::DEFAULT_SCOPE;

    fn state() -> ApiState {
        ApiState::new(DEFAULT_SCOPE).expect("state builds")
    }

    fn request(method: &str, path: &str, bearer: Option<&str>, body: &str) -> HttpRequest {
        let mut headers = vec![
            ("host".to_string(), "localhost".to_string()),
            ("content-length".to_string(), body.len().to_string()),
        ];
        if let Some(b) = bearer {
            headers.push(("authorization".to_string(), format!("Bearer {b}")));
        }
        HttpRequest {
            method: method.to_string(),
            path: path.to_string(),
            headers,
            body: body.as_bytes().to_vec(),
        }
    }

    fn bearer_for(state: &mut ApiState, action: ActionId, actor: &str) -> String {
        state
            .issue_token(actor, action, 60_000, 1_000)
            .token
    }

    #[test]
    fn token_issue_and_authenticated_submit() {
        let mut s = state();
        let req = request(
            "POST",
            "/api/v1/tokens",
            None,
            &serde_json::json!({ "actor": "alice", "action": "SubmitProtocol", "ttl_ms": 60000 }).to_string(),
        );
        let resp = handle_request(&mut s, &req, 1_000);
        assert_eq!(resp.status, 201);
        let issued: serde_json::Value = serde_json::from_slice(&resp.body).unwrap();
        let bearer = issued["token"].as_str().unwrap().to_string();

        let body = serde_json::json!({
            "id": "i1", "reagent": "A", "target_well": "w1", "volume_ul": 100,
            "temperature_milli_c": 2000,
            "environment": { "reagent_a": 1.0, "reagent_b": 1.0 }
        });
        let req = request("POST", "/api/v1/intents", Some(&bearer), &body.to_string());
        let resp = handle_request(&mut s, &req, 1_000);
        assert_eq!(resp.status, 201, "body: {}", String::from_utf8_lossy(&resp.body));
        let v: serde_json::Value = serde_json::from_slice(&resp.body).unwrap();
        assert_eq!(v["step_count"], 2);
        assert_eq!(v["accepted_steps"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn pdsg_and_audit_require_valid_bearer() {
        let mut s = state();
        let missing = handle_request(&mut s, &request("GET", "/api/v1/pdsg", None, ""), 1_000);
        assert_eq!(missing.status, 401);

        let read = bearer_for(&mut s, ActionId::ReadState, "alice");
        let ok = handle_request(
            &mut s,
            &request("GET", "/api/v1/pdsg", Some(&read), ""),
            1_000,
        );
        assert_eq!(ok.status, 200);
        let snapshot: serde_json::Value = serde_json::from_slice(&ok.body).unwrap();
        assert!(!snapshot["nodes"].as_array().unwrap().is_empty());

        // A SubmitProtocol bearer cannot read state.
        let submit = bearer_for(&mut s, ActionId::SubmitProtocol, "bob");
        let denied = handle_request(
            &mut s,
            &request("GET", "/api/v1/pdsg", Some(&submit), ""),
            1_000,
        );
        assert_eq!(denied.status, 401);
    }

    #[test]
    fn audit_endpoint_returns_compliance_report() {
        let mut s = state();
        let tok = bearer_for(&mut s, ActionId::SubmitProtocol, "alice");
        let body = serde_json::json!({
            "id": "i1", "reagent": "A", "target_well": "w1", "volume_ul": 100,
            "temperature_milli_c": 2000,
            "environment": { "reagent_a": 1.0, "reagent_b": 1.0 }
        });
        assert_eq!(
            handle_request(&mut s, &request("POST", "/api/v1/intents", Some(&tok), &body.to_string()), 1_000).status,
            201
        );
        let read = bearer_for(&mut s, ActionId::ReadState, "alice");
        let resp = handle_request(
            &mut s,
            &request("GET", "/api/v1/audit", Some(&read), ""),
            1_000,
        );
        assert_eq!(resp.status, 200);
        let report: serde_json::Value = serde_json::from_slice(&resp.body).unwrap();
        assert_eq!(report["core_compliant"], true);
        assert_eq!(report["reported_entry_count"], 6);
    }

    #[test]
    fn unknown_route_is_404() {
        let mut s = state();
        let resp = handle_request(&mut s, &request("GET", "/nope", None, ""), 1_000);
        assert_eq!(resp.status, 404);
    }

    #[test]
    fn parse_request_extracts_method_path_headers_body() {
        let raw = b"POST /api/v1/intents HTTP/1.1\r\nHost: localhost\r\nContent-Length: 7\r\n\r\n{\"x\":1}";
        let req = parse_request(raw).expect("parses");
        assert_eq!(req.method, "POST");
        assert_eq!(req.path, "/api/v1/intents");
        assert_eq!(req.header("content-length"), Some("7"));
        assert_eq!(req.body, b"{\"x\":1}");
    }

    #[test]
    fn parse_request_rejects_incomplete_body() {
        let raw = b"POST /api/v1/intents HTTP/1.1\r\nContent-Length: 10\r\n\r\nshort";
        assert!(parse_request(raw).is_err());
    }

    #[test]
    fn write_response_serializes_wire_form() {
        let resp = HttpResponse::json(201, &serde_json::json!({ "ok": true }));
        let wire = write_response(&resp);
        let text = String::from_utf8_lossy(&wire);
        assert!(text.starts_with("HTTP/1.1 201 Created\r\n"));
        assert!(text.contains("Content-Length:"));
        assert!(text.ends_with("\r\n\r\n{\"ok\":true}"));
    }
}
