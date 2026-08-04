//! The `tpt-chrysalis-api` TCP server loop.
//!
//! Single-threaded, blocking, loopback-first: reads a complete HTTP/1.1
//! request, dispatches it through [`tpt_chrysalis_api::handle_request`], and
//! writes the response. Configuration via environment:
//!
//! - `TPT_CHRYSALIS_ADDR`  (default `127.0.0.1:8080`)
//! - `TPT_CHRYSALIS_SCOPE` (default [`tpt_chrysalis_core::DEFAULT_SCOPE`])

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use tpt_chrysalis_api::{ApiState, handle_request, parse_request};
use tpt_chrysalis_core::DEFAULT_SCOPE;

const MAX_REQUEST_BYTES: usize = 64 * 1024;

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Reads a full request (headers + Content-Length body), then serves it.
fn serve_connection(state: &mut ApiState, mut stream: TcpStream) -> std::io::Result<()> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 2048];
    loop {
        if let Ok(req) = parse_request(&buf) {
            let resp = handle_request(state, &req, now_ms());
            stream.write_all(&tpt_chrysalis_api::write_response(&resp))?;
            stream.flush()?;
            return Ok(());
        }
        // Incomplete or malformed: cap buffering, then give up on this peer.
        if buf.len() >= MAX_REQUEST_BYTES {
            return Ok(());
        }
        let n = stream.read(&mut chunk)?;
        if n == 0 {
            return Ok(()); // client closed before a complete request
        }
        buf.extend_from_slice(&chunk[..n]);
    }
}

fn main() -> ExitCode {
    let addr = std::env::var("TPT_CHRYSALIS_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".to_string());
    let scope = std::env::var("TPT_CHRYSALIS_SCOPE").unwrap_or_else(|_| DEFAULT_SCOPE.to_string());

    let mut state = match ApiState::new(&scope) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("tpt-chrysalis-api: failed to build rig: {}", e.message());
            return ExitCode::from(1);
        }
    };

    let listener = match TcpListener::bind(&addr) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("tpt-chrysalis-api: cannot bind {addr}: {e}");
            return ExitCode::from(1);
        }
    };

    eprintln!("tpt-chrysalis-api listening on http://{addr} (scope {scope})");
    for stream in listener.incoming() {
        let Ok(stream) = stream else {
            continue;
        };
        if let Err(e) = serve_connection(&mut state, stream) {
            eprintln!("tpt-chrysalis-api: request failed: {e}");
        }
    }
    ExitCode::SUCCESS
}
