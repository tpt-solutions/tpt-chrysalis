//! API client for communicating with tpt-chrysalis-api backend

use crate::types::*;
use gloo_net::http::Request;
use js_sys::Promise;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::window;

const API_BASE: &str = "/api/v1";

pub async fn fetch_pdsg_snapshot() -> Result<PdsgSnapshot, ApiError> {
    let url = format!("{}/pdsg", API_BASE);
    let resp = Request::get(&url).send().await?;
    if resp.ok() {
        resp.json().await.map_err(|e| ApiError {
            code: "PARSE_ERROR".into(),
            message: format!("Failed to parse PDSG snapshot: {}", e),
            details: None,
        })
    } else {
        resp.json().await.map_err(|e| ApiError {
            code: "API_ERROR".into(),
            message: format!("API error: {}", e),
            details: None,
        })
    }
}

pub async fn fetch_alcoa_report(actor_filter: Option<&str>) -> Result<AlcoaReport, ApiError> {
    let mut url = format!("{}/audit", API_BASE);
    if let Some(actor) = actor_filter {
        url.push_str(&format!("?actor={}", actor));
    }
    let resp = Request::get(&url).send().await?;
    if resp.ok() {
        resp.json().await.map_err(|e| ApiError {
            code: "PARSE_ERROR".into(),
            message: format!("Failed to parse ALCOA report: {}", e),
            details: None,
        })
    } else {
        resp.json().await.map_err(|e| ApiError {
            code: "API_ERROR".into(),
            message: format!("API error: {}", e),
            details: None,
        })
    }
}

pub async fn submit_intent(request: SubmitIntentRequest) -> Result<PdsgSnapshot, ApiError> {
    let url = format!("{}/intents", API_BASE);
    let resp = Request::post(&url)
        .json(&request)
        .map_err(|e| ApiError {
            code: "SERIALIZE_ERROR".into(),
            message: format!("Failed to serialize request: {}", e),
            details: None,
        })?
        .send()
        .await?;

    if resp.ok() {
        resp.json().await.map_err(|e| ApiError {
            code: "PARSE_ERROR".into(),
            message: format!("Failed to parse response: {}", e),
            details: None,
        })
    } else {
        resp.json().await.map_err(|e| ApiError {
            code: "API_ERROR".into(),
            message: format!("API error: {}", e),
            details: None,
        })
    }
}

pub async fn mint_token(action_id: &str, scope: &str, subject: &str, ttl_seconds: u32) -> Result<CapabilityToken, ApiError> {
    let url = format!("{}/tokens", API_BASE);
    let body = serde_json::json!({
        "action_id": action_id,
        "scope": scope,
        "subject": subject,
        "ttl_seconds": ttl_seconds,
    });
    let resp = Request::post(&url)
        .json(&body)
        .map_err(|e| ApiError {
            code: "SERIALIZE_ERROR".into(),
            message: format!("Failed to serialize token request: {}", e),
            details: None,
        })?
        .send()
        .await?;

    if resp.ok() {
        resp.json().await.map_err(|e| ApiError {
            code: "PARSE_ERROR".into(),
            message: format!("Failed to parse token response: {}", e),
            details: None,
        })
    } else {
        resp.json().await.map_err(|e| ApiError {
            code: "API_ERROR".into(),
            message: format!("API error: {}", e),
            details: None,
        })
    }
}

pub fn connect_sensor_websocket<F>(mut on_message: F) -> Result<web_sys::WebSocket, ApiError>
where
    F: FnMut(serde_json::Value) + 'static,
{
    let window = window().ok_or_else(|| ApiError {
        code: "NO_WINDOW".into(),
        message: "No window object available".into(),
        details: None,
    })?;
    
    let location = window.location();
    let host = location.host().map_err(|_| ApiError {
        code: "NO_HOST".into(),
        message: "Cannot determine host".into(),
        details: None,
    })?;
    
    let protocol = if location.protocol().unwrap_or_default() == "https:" { "wss" } else { "ws" };
    let ws_url = format!("{}://{}/api/v1/ws/sensors", protocol, host);
    
    let ws = web_sys::WebSocket::new(&ws_url).map_err(|e| ApiError {
        code: "WS_CREATE_FAILED".into(),
        message: format!("Failed to create WebSocket: {:?}", e),
        details: None,
    })?;

    let closure = wasm_bindgen::closure::Closure::wrap(Box::new(move |event: web_sys::MessageEvent| {
        if let Ok(text) = event.data().dyn_into::<js_sys::JsString>() {
            let text: String = text.into();
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                on_message(json);
            }
        }
    }) as Box<dyn FnMut(_)>);
    
    ws.set_onmessage(Some(closure.as_ref().unchecked_ref()));
    closure.forget(); // Keep the closure alive
    
    Ok(ws)
}

pub async fn sleep_ms(ms: u32) {
    let promise = Promise::new(&mut |resolve, _| {
        web_sys::window()
            .unwrap()
            .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms as i32)
            .unwrap();
    });
    JsFuture::from(promise).await.ok();
}
