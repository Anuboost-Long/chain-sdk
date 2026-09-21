//! AgentServer capability — see
//! /agent-docs/capabilities/agent-server/CONTRACT.md.
//! A local, `127.0.0.1`-only HTTP listener, made portable by `tiny_http`
//! — a synchronous server with no async-runtime dependency, matching
//! chain-dev-inspector's own plain-thread `TcpListener` approach
//! (dev_inspector.rs) rather than pulling tokio into crates/core's
//! production path (unlike http.rs's async `reqwest` client, which only
//! needs tokio because it runs as a task on Tauri's own existing
//! runtime). There is no per-OS branching here.
//!
//! Native only understands "HTTP request in, HTTP response out" — no
//! JSON-RPC/MCP awareness. `dispatch` is how the Tauri bridge layer
//! plugs in "forward this to the webview and wait for its reply"; this
//! module has no idea a webview exists (see CONTRACT.md's "design
//! fork").

use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentServerRequest {
    pub method: String,
    /// The raw request-target from the HTTP request line, including any
    /// query string — this capability does not parse or split query
    /// parameters, that's app-level.
    pub path: String,
    /// Duplicate header names collapse to the last occurrence — matches
    /// the structural contract's `Record<string, string>` shape
    /// (contract.ts), which is already single-valued per key.
    pub headers: HashMap<String, String>,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentServerResponse {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: String,
}

/// What the dispatcher (the Tauri bridge layer, forwarding to the
/// webview and waiting for its reply) produced for one request. Kept
/// distinct from a plain `Result` so the timeout/failure-to-500/504
/// mapping below is real, unit-tested chain_core logic rather than
/// untested Tauri glue — see CONTRACT.md's Errors section for why a
/// handler exception or a timeout must never reject the whole server,
/// only that one caller's response.
pub enum DispatchOutcome {
    Response(AgentServerResponse),
    HandlerFailed(String),
    Timeout,
}

#[derive(Debug)]
pub enum AgentServerError {
    /// `port` was set but outside the valid TCP port range.
    InvalidPort(String),
    /// The requested port (explicit or ephemeral) couldn't be bound
    /// because it's already in use.
    Unavailable(String),
    /// The OS refused the bind for permission reasons (e.g. an explicit
    /// privileged port).
    PermissionDenied(String),
    Other(String),
}

impl AgentServerError {
    pub fn message(&self) -> &str {
        match self {
            AgentServerError::InvalidPort(m)
            | AgentServerError::Unavailable(m)
            | AgentServerError::PermissionDenied(m)
            | AgentServerError::Other(m) => m,
        }
    }
}

/// A running server. `stop()` is the documented, idempotent way to shut
/// it down; dropping the handle without calling it still shuts the
/// listener down the same way (see `Drop`), so a caller that forgets
/// never leaks a live socket.
pub struct ServerHandle {
    port: u16,
    server: Arc<tiny_http::Server>,
    accept_thread: Option<std::thread::JoinHandle<()>>,
}

impl ServerHandle {
    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn stop(mut self) {
        self.shut_down();
    }

    fn shut_down(&mut self) {
        self.server.unblock();
        if let Some(t) = self.accept_thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for ServerHandle {
    fn drop(&mut self) {
        self.shut_down();
    }
}

/// Binds a `127.0.0.1`-only listener — an explicit `port` (`0` or `None`
/// requests an OS-assigned ephemeral one, same default
/// `chain-dev-inspector`'s own listener already uses) — and forwards
/// every accepted request to `dispatch`, **serially**: one in-flight
/// request at a time, the next connection isn't accepted until the
/// current one's response has been written (see CONTRACT.md's Non-goals
/// on concurrency). `dispatch` runs on a dedicated background thread,
/// never the calling thread.
pub fn start(
    port: Option<i64>,
    dispatch: impl Fn(AgentServerRequest) -> DispatchOutcome + Send + Sync + 'static,
) -> Result<ServerHandle, AgentServerError> {
    let requested: u16 = match port {
        None => 0,
        Some(p) if p == 0 => 0,
        Some(p) if (1..=65535).contains(&p) => p as u16,
        Some(p) => return Err(AgentServerError::InvalidPort(format!("port out of range: {p}"))),
    };

    let addr = format!("127.0.0.1:{requested}");
    let server = tiny_http::Server::http(&addr).map_err(classify_bind_error)?;
    let server = Arc::new(server);
    let bound_port = match server.server_addr() {
        tiny_http::ListenAddr::IP(addr) => addr.port(),
        #[allow(unreachable_patterns)]
        _ => return Err(AgentServerError::Other("server bound to a non-IP address".to_string())),
    };

    let accept_server = Arc::clone(&server);
    let accept_thread = std::thread::spawn(move || {
        for mut request in accept_server.incoming_requests() {
            let agent_request = to_agent_request(&mut request);
            let response = match dispatch(agent_request) {
                DispatchOutcome::Response(r) => r,
                DispatchOutcome::HandlerFailed(message) => {
                    AgentServerResponse { status: 500, headers: HashMap::new(), body: message }
                }
                DispatchOutcome::Timeout => AgentServerResponse {
                    status: 504,
                    headers: HashMap::new(),
                    body: "agent-server handler timed out".to_string(),
                },
            };
            let _ = respond(request, response);
        }
    });

    Ok(ServerHandle { port: bound_port, server, accept_thread: Some(accept_thread) })
}

fn classify_bind_error(e: Box<dyn std::error::Error + Send + Sync>) -> AgentServerError {
    if let Some(io_err) = e.downcast_ref::<std::io::Error>() {
        return match io_err.kind() {
            std::io::ErrorKind::AddrInUse => AgentServerError::Unavailable(io_err.to_string()),
            std::io::ErrorKind::PermissionDenied => {
                AgentServerError::PermissionDenied(io_err.to_string())
            }
            _ => AgentServerError::Other(io_err.to_string()),
        };
    }
    AgentServerError::Other(e.to_string())
}

fn to_agent_request(request: &mut tiny_http::Request) -> AgentServerRequest {
    let method = request.method().as_str().to_string();
    let path = request.url().to_string();
    let headers = request
        .headers()
        .iter()
        .map(|h| (h.field.as_str().as_str().to_string(), h.value.as_str().to_string()))
        .collect();

    let mut bytes = Vec::new();
    let _ = request.as_reader().read_to_end(&mut bytes);
    let body = String::from_utf8_lossy(&bytes).into_owned();

    AgentServerRequest { method, path, headers, body }
}

fn respond(request: tiny_http::Request, response: AgentServerResponse) -> std::io::Result<()> {
    let mut http_response =
        tiny_http::Response::from_string(response.body).with_status_code(response.status);
    for (name, value) in &response.headers {
        if let Ok(header) = tiny_http::Header::from_bytes(name.as_bytes(), value.as_bytes()) {
            http_response.add_header(header);
        }
    }
    request.respond(http_response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::time::Duration;

    /// Sends a raw HTTP/1.1 request over a fresh loopback connection and
    /// reads the full response back as text — exercises the real
    /// tiny_http parse/respond path, not just `dispatch`'s inputs.
    /// `Connection: close` makes the server close the socket once it's
    /// responded, so reading to EOF gives us the whole response cleanly.
    fn send_request(port: u16, method: &str, path: &str, body: &str) -> String {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let request = format!(
            "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(request.as_bytes()).unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        response
    }

    fn status_line_of(response: &str) -> &str {
        response.lines().next().unwrap()
    }

    fn body_of(response: &str) -> &str {
        response.split("\r\n\r\n").nth(1).unwrap_or("")
    }

    #[test]
    fn round_trips_a_request_to_the_dispatcher_and_back() {
        let handle = start(None, |req| {
            assert_eq!(req.method, "POST");
            assert_eq!(req.path, "/mcp");
            assert_eq!(req.body, "hello from the client");
            DispatchOutcome::Response(AgentServerResponse {
                status: 200,
                headers: HashMap::new(),
                body: "hello from the handler".to_string(),
            })
        })
        .unwrap();

        assert_ne!(handle.port(), 0, "an ephemeral bind should resolve to a real port");

        let response = send_request(handle.port(), "POST", "/mcp", "hello from the client");
        assert_eq!(status_line_of(&response), "HTTP/1.1 200 OK");
        assert_eq!(body_of(&response), "hello from the handler");

        handle.stop();
    }

    #[test]
    fn a_handler_failure_becomes_a_500_not_a_dropped_connection() {
        let handle = start(None, |_req| DispatchOutcome::HandlerFailed("boom".to_string())).unwrap();

        let response = send_request(handle.port(), "GET", "/", "");
        assert_eq!(status_line_of(&response), "HTTP/1.1 500 Internal Server Error");
        assert_eq!(body_of(&response), "boom");

        handle.stop();
    }

    #[test]
    fn a_handler_timeout_becomes_a_504_not_a_hang() {
        let handle = start(None, |_req| DispatchOutcome::Timeout).unwrap();

        let response = send_request(handle.port(), "GET", "/", "");
        assert_eq!(status_line_of(&response), "HTTP/1.1 504 Gateway Timeout");

        handle.stop();
    }

    #[test]
    fn rejects_a_port_already_in_use() {
        let first = start(None, |_req| {
            DispatchOutcome::Response(AgentServerResponse {
                status: 200,
                headers: HashMap::new(),
                body: String::new(),
            })
        })
        .unwrap();

        let second = start(Some(first.port() as i64), |_req| {
            DispatchOutcome::Response(AgentServerResponse {
                status: 200,
                headers: HashMap::new(),
                body: String::new(),
            })
        });
        match second {
            Err(AgentServerError::Unavailable(_)) => {}
            Err(other) => panic!("expected Unavailable, got {}", other.message()),
            Ok(_) => panic!("expected the second bind to the same port to fail"),
        }

        first.stop();
    }

    #[test]
    fn rejects_an_out_of_range_port() {
        match start(Some(-1), |_req| unreachable!()) {
            Err(AgentServerError::InvalidPort(_)) => {}
            other => panic!("expected InvalidPort, got {:?}", other.map(|h| h.port())),
        }

        match start(Some(70000), |_req| unreachable!()) {
            Err(AgentServerError::InvalidPort(_)) => {}
            other => panic!("expected InvalidPort, got {:?}", other.map(|h| h.port())),
        }
    }
}
